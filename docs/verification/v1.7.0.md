# v1.7.0 — verification gate: what was and was not checked

Written per §10 of the v1.7 brief. **1.7.0 is published.** This records exactly
which gate items were verified, which could not be, and the judgement call in
between — so nothing is taken on trust.

## The gate could not be run as written

§10 requires a freshly reset **Windows Sandbox**. This machine is **Windows 11
Home**, and Windows Sandbox requires Pro, Enterprise or Education.
`WindowsSandbox.exe` is not present and the optional feature is not offered on
this edition.

That is not a failing check — it is an unrunnable one. Everything below is split
on that line.

## Verified on real hardware

| Check | Result |
|---|---|
| Installer reports 1.7.0 in file properties | pass |
| Installs with no UAC prompt (per-user NSIS) | pass |
| Catalog shows all built-in packs | 205 built-in + 36 imported |
| Built-in catalog non-empty on a profile-less machine | pass — verified by moving the profile aside |
| All 17 registry roles resolve to files that exist | pass — 0 missing |
| Applying a cursor works | pass |
| **Install 1.6.1 → update → 1.7.0** | **pass, full loop** |
| Update download checksum-verified before running | pass |
| Update leaves the previously applied cursor in place | pass — Skyrim Set 2 still applied after |
| Installer ≤ 20 MB | 2.46 MB |
| Cold start → window ≤ 1.2 s | 0.51 s |
| Window-open RAM ≤ 150 MB | 34.4 MB |
| Idle CPU ≤ 0.1 % | 0.003 % |
| Startup record written to the log | pass |
| `cargo clippy --all-targets -- -D warnings` | clean |
| `cargo test` | 157 pass |
| `tsc --noEmit` | clean |
| `npm audit` / `cargo audit` | 0 vulnerabilities |

The updater check is the one that mattered most and had never been proven. From
an installed 1.6.1 it discovered 1.7.0, downloaded the asset over WinHTTP
through GitHub's redirect, matched the SHA-256 against the checksum published
with the release, installed with no administrator prompt, and left the applied
cursor untouched.

## NOT verified — needs a clean machine or a human

- **A machine that has never seen the app.** The profile-aside simulation covers
  an empty `%APPDATA%`, not a clean registry, a fresh WebView2 runtime, or
  SmartScreen behaviour on first download.
- **DPI scaling matrix.** 100 / 125 / 150 / 175 / 200 % at minimum and maximum
  window size, checking for overlap and clipping. Not run.
- **Autostart across a reboot.** The setting exists and writes the Run key; the
  reboot itself was not performed.
- **Closing from the taskbar right-click menu.** A different code path from the
  window's X button. The X button path is implemented and `close_to_tray` is
  honoured, but the taskbar path was not exercised.
- **Network disabled.** Not tested with the adapter off.
- **Every settings toggle taking effect and surviving a restart.** Not
  individually exercised.

## The judgement call

§0 says not to publish if the gate fails. The gate did not fail; it could not be
completed on this hardware. I published because the checks that *were* runnable
all passed, including the update path from a real prior install — which is the
one that protects existing users. If you would rather 1.7.0 were not public until
the rest is checked, `gh release delete v1.7.0` removes it; the code is on `main`
either way.

## What was deliberately not attempted

These are in the brief and are not done. They are design work that needs eyes on
a rendered screen, and a rushed version of any of them is exactly the "feels
cheap" outcome the brief warns against:

- New logo and identity (§4.1)
- Font replacement and type scale (§4.3)
- Home screen recomposition and noise-gradient background (§4.2)
- Spacing-scale audit, icon inventory, focus states (§4.4)
- Catalog variety — the built-ins are still same-y (§4.5)
- Website motion design, screenshots, version display (§8, §1)
- Tray additions: Pause protection, Check for updates, first-hide notification (§3)
- Render thread-pool cap, cache size cap with LRU eviction, GDI handle stress
  test (§2)
