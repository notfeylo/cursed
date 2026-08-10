# Verification records

One file per release that needed one, named for the version: `v1.7.0.md`.

A record exists to make the difference between *checked*, *not checked* and
*could not be checked* survive past the day of the release. A release note says
what changed. This says what is actually known to work, and — more usefully —
what is not.

## What goes in one

- What was verified, on what hardware, with the result.
- What could **not** be verified, and why. An unrunnable check is not a passing
  check, and recording it as one is how a gap becomes permanent.
- Any judgement call made between those two, with the reasoning.

Write it whether or not the release ships. `v1.7.0.md` records a gate that could
not be run as written — the machine is Windows 11 Home and Windows Sandbox needs
Pro — and that fact is worth more written down than remembered.

## The standing gate

Run before tagging:

```
npm run check:bundle                      # fonts, and no specimen in production
cargo clippy --all-targets -- -D warnings # on every shipped target
cargo test
npx tsc --noEmit
```

And the one that needs a real machine, or a VM rolled back to a clean snapshot:

```powershell
# before installing
powershell -File scripts/verify-uninstall.ps1 -Snapshot
# install, apply a cursor, import an image, save a preset, then uninstall
powershell -File scripts/verify-uninstall.ps1
```

It asserts all seventeen pointer roles came back byte-identical to their
pre-install values, and that no file, registry key, cursor scheme, autostart
entry or shortcut remains — including `%LOCALAPPDATA%\dev.feylo.cursed`, the
WebView2 folder keyed by bundle identifier rather than product name, which is
the one almost every app of this kind leaves behind.

Exit code is the number of failed assertions, so it can gate a release
automatically.
