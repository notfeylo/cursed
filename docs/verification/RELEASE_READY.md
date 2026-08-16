# v1.21.0 — is it ready?

**Published 2026-08-16, with one row still owed.**

The signing key exists, the release is signed, and the update path has *not* yet
been observed working on a clean machine. That ordering is deliberate and is
explained under "the chicken and the egg" below: the update path can only be
verified against a published release, so the release goes out and the VM run
follows it immediately, with the release pulled if a row fails.

**The VM matrix is the outstanding item.** Until it is run, this release is
verified the same way the bug it fixes was verified — which is to say, not in
the way that would have caught it.

---

## Why this release of all releases must not go out unverified

The change being shipped is the update path. The bug it fixes deleted users'
data on every update from 1.0.0 to 1.20.0, and it did so while every test in the
suite passed. **Nothing in the green column below would have caught it**, because
it was invisible to exactly this class of checking: the app compiled, its tests
passed, and the installer destroyed `%APPDATA%\Cursed` anyway.

Publishing a fix for that, verified the same way the bug survived, would be
repeating the mistake with more confidence.

---

## What passed

Everything here runs on this machine or in CI, on every push.

| | |
| --- | --- |
| `cargo test` | 297 passed |
| `cargo test --features dev-channel` | 297 passed |
| `cargo clippy --all-targets -- -D warnings` | clean |
| `npx tsc --noEmit` | clean |
| `npm run check:bundle` | all checks passed |
| `npm run check:roles` | 17 of 17, 0 faults |
| Fuzzing, ~24,600 damaged inputs across six parsers | 0 panics |
| Handle harness, 20,000 load/release cycles | GDI +0, USER +0 |
| Soak, 11h50m, 711 samples | GDI and USER never moved; handles +0; memory drift inside the noise |
| Background-removal contact sheet, 8 cases + animation | all behave; the halo is fixed |
| The photograph that started the matte fix | refused, byte-identical output |

The update-path specifics, all static:

- `/UPDATE /P /R /NS` reach the `Command` that is one call from `CreateProcess`,
  asserted against the command's own argument list rather than the constant.
- An update cannot reach either uninstall hook's destructive statements — the
  guard's `Goto` is traced to its landing label and every destructive line is
  proven to sit between them.
- `install_update` verifies before it tears anything down, and the order is
  pinned.
- The generated NSIS script skips the reinstall page in update mode.
- `/UPDATE` survives `strip` and LTO into the release binary.
- Nothing but NSIS is bundled and nothing staged is an MSI.

## What is blocked

### 1. The VM matrix — **BLOCKED, awaiting a VM**

Every row in [`update-path.md`](update-path.md) marked BLOCKED. The headline is
still unrun: **N → N+1 with presets, custom cursors and the applied cursor
intact.**

Not run here because the development machine holds the live install with 395 MB
of the author's own data in the directory this bug deletes. Testing that against
that machine is not a test.

[`VM_SETUP.md`](VM_SETUP.md) is the setup — VirtualBox plus the free Windows 11
Enterprise evaluation image, both free, snapshotted pristine.

**The one command:**

```powershell
powershell -ExecutionPolicy Bypass -File scripts\verify-release.ps1 `
  -From C:\builds\Cursed_1.20.0_x64-setup.exe -To 1.21.0
```

It prints a Markdown table. Paste it into `update-path.md`.

### 2. The signing key — **CLEARED 2026-08-16**

All three secrets were added by the owner and confirmed present before tagging:

| Secret | Added |
| --- | --- |
| `CURSED_UPDATE_PUBLIC_KEY` | 2026-08-16T20:17:12Z |
| `TAURI_SIGNING_PRIVATE_KEY` | 2026-08-16T20:17:12Z |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | 2026-08-16T20:26:20Z |

Only the names are readable, which is the point of a secret — the values are
proven by the build succeeding and by a `.minisig` appearing beside every
versioned installer.

The password is deliberately not checked for emptiness by the workflow: a key
generated without one has an empty password, and an empty value is a legitimate
answer.

[`SIGNING.md`](../SIGNING.md) has the rotation procedure, which is the part that
is easy to get backwards.

---

## The chicken and the egg, stated plainly

The update path can only be verified against a **published** release — the
updater reads the GitHub releases API, and a draft is not returned by it. So a
draft release cannot be tested against either.

There is no way to verify this before publishing it. What there is:

1. Generate the signing key and add the three secrets.
2. Tag `v1.21.0`. Let the workflow build and sign, and publish the draft.
3. **Immediately** run `verify-release.ps1` in the VM, updating 1.20.0 → 1.21.0.
4. If a row fails, delete the release within the hour. Anyone who took it in
   that window is on a build whose update path is at least no worse than the one
   they were already running.
5. If it passes, paste the table into `update-path.md` and update the website.

Step 3 is not optional and is not a formality. It is the only step in this
entire list that would have caught the bug being fixed.

## No artifacts staged

`dist-release/` is deliberately empty. Building four installers that must not be
published, on a machine that cannot sign them, produces four files whose only
possible use is to be published by mistake. The release workflow builds them, on
the tag, with the key — which is the only place they should come from.

## Suggested release note

> **Updating no longer deletes your data.** Every update up to and including
> 1.20.0 ran the previous version's uninstaller, which restored the stock Windows
> pointer scheme and offered to delete your presets and custom cursors with "no"
> as the pre-selected answer. If your cursors disappeared after an update, that
> is why — it was our bug, not something you did. See the changelog for the full
> account and for what the app now does about an original pointer scheme that
> was lost.

The changelog entry says it in longer form and in the user's words. Do not
soften it.
