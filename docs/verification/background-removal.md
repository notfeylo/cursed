# Background removal — the contact sheet

**Covers §2.5 of the original brief and §2/§4 of the matte fix.** Updated in
place.

```bash
cargo run --manifest-path src-tauri/Cargo.toml --release --bin genpacks -- --matte-sheet
```

Writes [`matte-sheet.png`](matte-sheet.png): eight test images, before on the
top row and after on the bottom, composited over a checkerboard so transparency
reads as transparency. Exits non-zero if any case misbehaves, so it is a check
as well as a picture.

---

## What went wrong, and what the fix actually is

A user imported a photograph — an American football on grass, dark vignetted
studio background — in a clean VM. What came back was unusable: the subject
destroyed, most of the image eaten, an unrecognisable dark blob.

**The flood fill did not malfunction.** It was handed an input it cannot key.
That photograph has a gradient background, grass (thousands of distinct greens,
many closer to each other than to any sampled corner), a shadow blending
continuously into the grass with no edge to stop at, and vignetting, so the four
corners are not even the same colour as each other. Flood-filling that either
stops at the first blade of grass or walks straight through the subject. **There
is no tolerance value that produces a correct result.**

So the bug was never "the flood fill is bad". It was that the app attempted a
removal it could not do, with no confidence check, and returned the wreckage.

The fix is to ask a different question first: not *what tolerance*, but *is this
keyable at all*.

## The four signals

`matte::assess` measures the image before anything is attempted. Any one signal
firing is enough — they measure different failures, and an image only needs one
of them to be unkeyable.

| Signal | What it catches | Threshold |
| --- | --- | --- |
| Corner disagreement | vignettes and gradients: the corners do not agree | > 40 |
| Border variance | texture rather than sensor noise | > 20 |
| Border colours per 1,000 | a busy backdrop | > 8.0 |
| Border edge density | **texture** — pixel-to-pixel contrast | > 22% |

Measured, not guessed. The first set of thresholds *was* guessed, and let the
football photograph through on three signals out of four. These are what the
cases actually produce:

| Case | Corner | Var | Col/1k | Edge% | Keyable |
| --- | --- | --- | --- | --- | --- |
| 1 logo on white | 0 | 0 | 0.5 | 0% | yes |
| 2 checkerboard screenshot | 0 | 0 | 0.5 | 0% | yes |
| 3 jpeg fringing | 1 | 2 | 2.0 | 0% | yes |
| 4 dark art on black | 0 | 0 | 0.5 | 0% | yes |
| 5 already clean alpha | 0 | 0 | 0.0 | 0% | yes |
| 6 grey on grey | 0 | 0 | 0.5 | 0% | yes |
| **7 football on grass** | **42** | 14 | **14.6** | **50%** | **NO** |
| 8 animation | 0 | 0 | 0.5 | 0% | yes |

Three signals catch case 7 independently, and every flat case sits far below
every threshold. That margin is the point: a threshold that only just catches a
photograph is one that lets the next one through.

**Border variance is the weakest of the four** and is kept anyway, because it is
the only one that fires on a *bright* textured background. A vignette compresses
the border's values toward black, which drags this figure down precisely when
the background is least keyable — case 7 measures only 14 here.

A colour *density* rather than a count, because a count is as much a function of
the image's size as its content: the same photograph measures three times as
many colours at 4K as at 720p.

## What happens on a refusal

Nothing. That is the whole point.

```
7 football on grass    0.0%  ok, refused (LooksLikeAPhotograph)
```

The sheet asserts the output is **byte-identical** to the input, not merely
"mostly unchanged" — refusing while still having modified the image would be the
same bug with a message attached.

The user is told, in the import screen, above the toggle:

> This looks like a photo. Automatic background removal works on flat
> backgrounds — logos, icons, screenshots — and it will not do a good job here.
> Use the image as it is, or cut it out yourself in the editor.

And they can overrule it. `Cut::Force` skips the refusal, because "try it and
let me look" is a reasonable thing to mean. What `force` does **not** skip is the
check on the result: no button in this app means "hand me back something
unrecognisable".

## The order that makes it safe

1. Flatten a transparency checkerboard, so everything below sees one background.
2. **Assess.** A photograph gets no tolerance tried on it.
3. **Key on a copy.** The caller's bitmap is untouched until there is a result
   worth having.
4. **Check what came back.** Under 5% claimed means nothing happened; over 85%
   with nothing coherent left means the subject was eaten.
5. Commit, or revert and say why.

Step 3 is what makes "never destroy the input" true rather than intended, and
step 4 is why trimming downstream is safe: `pipeline` only ever trims an image
that is either correctly keyed or fully opaque. Run against a bad matte, trim is
what turns a poor cutout into a crop of garbage.

### Coverage cannot judge a cut

"Over 85% means it ate the subject" is nearly right and fails on the most common
shape of cursor art there is: **a small logo on a large canvas legitimately keys
away 99% of the image.** So coherence is measured instead — the largest
connected run of surviving opacity as a fraction of all of it. A logo is one
blob and scores near 1.0; shredded grass is a thousand blobs and scores near 0.

This exposed a bug that had been shipping: `cut` bailed out at 97% coverage on
exactly that reasoning, so every sparse logo came back with **no background
removed at all**, silently. That guard is gone and the coherence check replaces
it.

## The halo is fixed

Case 3 — JPEG ringing — used to come back with a bright ring traced around the
subject. Compression puts an overshoot just outside a hard edge; those pixels
sit too far from the background to be keyed and too close to the edge to be
subject, and everything upstream is deliberately edge-*preserving*, so they
survived.

`matte::despill` corrects them. For each kept pixel near the flood, it takes the
median of the kept pixels further in — uncontaminated by definition — and if the
pixel leans toward the background relative to that median, replaces its colour
and keeps its alpha. Colour only, never alpha: the alpha is the shape, and the
shape was decided by the flood.

Two details that took a test each:

- **The fringe is two pixels, not one.** A despill that only reached pixels
  touching the flood left the inner half of a ring behind — a bright line one
  pixel in from the edge.
- **A pixel with too few clean neighbours is left alone.** On a one-pixel-wide
  feature there is no interior to sample, and guessing there eats the feature.

## Animation

Checked across frames rather than on the sheet, because a per-frame decision is
invisible in a still: every frame looks right on its own and the artefact only
exists in motion, as the edge crawls.

Eight frames of a moving subject on a fixed background, coverage compared
against the first: **consistent to 0.00%.**

## Two more bugs the tests found

- **Transparency was being measured as black.** A PNG with a transparent margin
  carries `[0, 0, 0, 0]` around its edge, and counting that as a colour made an
  ordinary logo look like a high-contrast, many-coloured, disagreeing border —
  which is to say, like a photograph. It was refused for having been cut out
  properly.
- **A synthetic test case that tested nothing.** The first version of case 7 was
  a smooth gradient, which *is* background by every definition in `matte`;
  removing it is arguably correct. The second version had a vignette deep enough
  to crush the border to near-uniform black, which scored as flat and keyable.
  Both said more about the generator than about the matte. The current one keeps
  the grass visible at the edges, the way the photograph it reproduces does.

## Still not built

| | Status |
| --- | --- |
| Refuse what cannot be keyed | **done** |
| Never destroy the input | **done** — keyed on a copy, reverted on failure |
| Trim only after the sanity check | **done** |
| Border-connected fill, enclosed regions protected | done, and predates this pass |
| Despill | **done** |
| Luminance keying across the fringe | done |
| Morphological refinement / despeckle | done |
| Premultiplied alpha end to end | done |
| Consistent matte across animation frames | done, now measured |
| **Manual editor: brush, undo, tolerance slider** | **not built** |

The editor is the honest gap. A low-confidence refusal without one is a dead
end: the app says it cannot do this and offers no way for the user to do it
themselves.
