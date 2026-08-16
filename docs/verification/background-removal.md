# Background removal — the contact sheet

**Covers the visual half of §2.5 of the research brief.** Updated in place.

```bash
cargo run --manifest-path src-tauri/Cargo.toml --release --bin genpacks -- --matte-sheet
```

Writes [`matte-sheet.png`](matte-sheet.png): seven test images, before on the
top row and after on the bottom, composited over a checkerboard so transparency
reads as transparency rather than as the page.

## Why a picture

The acceptance criteria here are visual and there is no assertion that
distinguishes a clean edge from a chewed one. "63% of pixels removed" is the
same number for a perfect cut and a cut that ate the subject's left side. The
unit tests in `build::matte` pin the decisions — what counts as already cut out,
where the tolerance comes from, that the flood stops at texture — and none of
them can tell you whether the result looks right.

So the deliverable is the sheet, and the sheet is generated rather than shipped:
no test artwork in the repository, reproducible on any machine.

## The seven cases

Each one breaks something different. A case set where everything passes for the
same reason is one case.

| | Case | What it catches | Result |
| --- | --- | --- | --- |
| 1 | logo on white | the ordinary one; a naive global colour match also passes it, which is why it cannot be the only case | clean |
| 2 | **checkerboard screenshot** | an editor's transparency grid, photographed. Two background colours, not one | clean |
| 3 | JPEG fringing | ringing around a hard edge | cut, **with a halo** |
| 4 | anti-aliased dark art on black | subject and background share a value range | clean |
| 5 | already clean alpha | must be left completely alone | untouched, 0.0% |
| 6 | grey subject on a grey card | a subject a few levels from its own background | clean |
| 7 | photograph | must fail *gracefully* | sky removed, subject and texture intact |

## What the sheet found

Two things, neither of which any existing test reports.

### Case 3 leaves a bright halo

The black disc comes out with a visible light ring around it. That is the JPEG
ringing: a bright overshoot sits just outside the edge, close enough to the
subject to survive the key and far enough from the background to be kept.

This is what **despill** is for, and there is no despill step. The brief lists
one; it is not built. Everything else in the pipeline is edge-*preserving*,
which is correct and is exactly why the overshoot survives.

Not fixed here on purpose: a bad despill desaturates every genuinely coloured
edge in the catalog, and shipping one without this sheet to judge it against is
how that happens quietly.

### Case 2 is newly correct

The checkerboard case did not work before this pass and failed in a way that
looked like a different bug entirely. A screenshotted transparency grid has two
background colours, so `sample_border` returned a colour present in neither
square, `border_spread` measured the distance between the two greys and declared
the image a noisy photographic backdrop, and `tolerance_for` handed out enough
slack to swallow anything grey in the subject.

`matte::detect_checkerboard` now finds the grid from the border — two
near-neutral colours covering 85% of it, alternating on a consistent run length
between 2 and 64 px — and flattens both squares to one colour before anything
samples anything. The rest of the pipeline then sees an ordinary flat background.

Flattening rather than keying both colours out directly is deliberate: keying
would clear every grey pixel in the subject that matched a square, wherever it
sat. Flattening keeps the connectivity rule, so a grey patch in the middle of
the artwork survives because it is not joined to the edge.

### A smaller one

Case 7 leaves a one-pixel light line along the horizon, where the removed sky
meets the kept ground. Cosmetic on a photograph, and it would be visible on a
cursor.

## What is still not built

The algorithm side of §2.5 is done except for despill. The **interaction** side
is not started:

| | Status |
| --- | --- |
| Ask-first flow, with a clear No | done — `Cut::{auto, force, keep}`, and `auto` never touches an image that is already cut out |
| Corner flood fill with tolerance, never global colour matching | done |
| Checkerboard detection | done, this pass |
| Luminance keying for anti-aliased edges | done |
| Morphological edge refinement / despeckle | done |
| Premultiplied alpha end to end | done — resampling is in linear light with premultiplied alpha |
| Same parameters across every frame of an animation | done — one master decision applied to the sequence |
| **Despill** | **not built** — see case 3 |
| **Tolerance slider with live checkerboard preview** | **not built** |
| **Manual erase / restore brush** | **not built** |
| **Non-destructive with undo** | **not built** |

The last three are one feature wearing three names: a canvas editor. It is a
substantial piece of frontend work — a paint surface, a history stack, and a
preview that re-cuts as a slider moves — and half of it is worse than none,
because a brush with no undo is a way to destroy someone's import.
