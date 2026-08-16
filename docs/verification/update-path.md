# The update path — verification record

**Covers §2.5 of the update-path research brief.** Not a release record: this
tracks one mechanism across releases, and is updated in place as rows are run.

Diagnosis: [`../UPDATE_PATH_DIAGNOSIS.md`](../UPDATE_PATH_DIAGNOSIS.md).
Fix: `INSTALLER_ARGUMENTS` in `src-tauri/src/updates.rs`, `install_update` in
`src-tauri/src/commands.rs`, `bundle.targets` in `src-tauri/tauri.conf.json`.

---

## Status, plainly

**The fix is unverified on a real machine.** Everything below marked ✅ is a
static check — a unit test, or a line traced through the generated NSIS script.
Not one of them observes an actual update.

That distinction matters more here than usual, because the bug being fixed was
invisible to exactly this class of checking: the app compiled, its tests passed,
and the installer destroyed users' data anyway. **Nothing in the ✅ column would
have caught the original bug either.** The rows that would have caught it are all
in the ⬜ column, and they need a VM.

---

## What has been checked

| Check | How | Result |
| --- | --- | --- |
| The installer is told this is an update | `the_installer_is_told_that_this_is_an_update` asserts `/UPDATE` is passed | ✅ |
| `/R` is never passed without `/P` | same test — the template ignores `/R` outside passive/silent mode | ✅ |
| Not silent | same test asserts `/S` is absent; an install the user asked for should show something | ✅ |
| The uninstall hooks still guard on `$UpdateMode` | `the_uninstall_hooks_still_check_update_mode` reads `installer-hooks.nsh` and counts the guards | ✅ |
| Only NSIS is built | `the_bundle_ships_nsis_only` reads `tauri.conf.json` | ✅ |
| No elevation is ever required | `the_installer_never_needs_elevation` asserts `installMode: currentUser` | ✅ |
| The published release contains no MSI | `gh release view v1.20.0 --json assets` | ✅ |
| Verification happens before teardown | `install_update` calls `verified_installer` first; a checksum failure returns before `prepare_for_shutdown` | ✅ read, not run |
| State survives a crash mid-write | `state::store` tests: atomic write, `.bak` fallback, corrupt file preserved, primary healed | ✅ |
| The flags reach the spawned process | `the_flags_reach_the_command_that_is_actually_run` reads the `Command`'s own argument list, not the constant | ✅ |
| An update cannot reach the uninstall hooks' destructive lines | `an_update_cannot_reach_anything_the_uninstaller_destroys` traces the guard's `Goto` and its landing label | ✅ |
| Nothing is torn down before the checksum passes | `nothing_is_torn_down_before_the_installer_is_verified` pins the order in `install_update` | ✅ |
| The generated NSIS script skips the reinstall page in update mode | `npm run check:bundle`, against the script the installer is compiled from | ✅ |
| `/UPDATE` survives `strip` and LTO into the release binary | `npm run check:bundle` | ✅ |
| Nothing but NSIS is bundled, and nothing staged is an MSI | `npm run check:bundle` | ✅ |
| The fingerprint notices a changed byte, a deletion and an addition | `dataprint` tests | ✅ |

## What has not

Every row below needs a Windows VM snapshotted clean and rolled back between
runs. **None have been run. All are BLOCKED — awaiting VM.**

The status is written as `BLOCKED` rather than as an empty box on purpose. An
empty box reads like a queue; blocked names the reason, and the reason is the
thing that has to change before any of them can move.

| Row | Why it matters | Status | Covered by |
| --- | --- | --- | --- |
| **N → N+1, presets and applied cursor intact** | The headline. This is the one that proves the data loss is gone | BLOCKED | `verify-release.ps1` §5 |
| One click, one progress bar, no uninstall step, no prompts | The reported symptom | BLOCKED | `verify-release.ps1` §4, by eye |
| App relaunches itself on the new version | `/R`, never observed | BLOCKED | `verify-release.ps1` §4 |
| `%APPDATA%\Cursed` byte-identical across an update | Cannot be a unit test — the process that would assert it is the one being replaced | BLOCKED | `verify-release.ps1` §5, on `--data-print` |
| All 17 registry paths still resolve after updating | Cursor survives the swap | BLOCKED | `verify-release.ps1` §6 |
| First install leaves a working app on a machine that never had one | The other half of the installer | BLOCKED | `verify-release.ps1` §2 |
| Uninstall leaves nothing | Already scripted; never run against an *updated* install | BLOCKED | `verify-release.ps1` §7 → `verify-uninstall.ps1` |
| No duplicate desktop shortcut after updating | What `/NS` is for | BLOCKED | `verify-release.ps1` §4 |
| Network dropped mid-download | | BLOCKED | by hand |
| Disk full | | BLOCKED | by hand |
| Antivirus quarantines the installer | | BLOCKED | by hand |
| User cancels SmartScreen | Unsigned build, so this is the common path today | BLOCKED | by hand |
| Machine sleeps mid-download | | BLOCKED | by hand |
| Power loss mid-install | The one `state::store` was written for; still needs proving end to end | BLOCKED | by hand |
| Update triggered twice | | BLOCKED | by hand |
| Update while a cursor is applied | | BLOCKED | `verify-release.ps1` seeds this |
| Update while the catalog is rendering | | BLOCKED | by hand |
| `%APPDATA%` read-only or OneDrive-redirected | | BLOCKED | by hand |
| Two Windows users signed in simultaneously | Ruled out by reading `FindProcessCurrentUser`; unproven | BLOCKED | by hand |
| Remote Desktop session | | BLOCKED | by hand |
| App in tray vs window open | | BLOCKED | by hand |

### Why they have not been run here

The development machine has the live v1.20.0 install on it, with 395 MB of the
author's own imported packs, custom cursors and the original-scheme snapshot in
`%APPDATA%\Cursed`. Testing an update path whose failure mode is *deleting that
directory* against that machine is not a test, it is a coin toss with the thing
being protected. Windows Sandbox is unavailable — this is Windows 11 Home, and
Sandbox needs Pro, the same wall `v1.7.0.md` hit.

`verify-release.ps1` refuses to run against a data directory holding more than
20 MB without `-Force`, for exactly that reason.

## How to run it when there is a VM

[`VM_SETUP.md`](VM_SETUP.md) is the setup — VirtualBox plus the free Windows 11
Enterprise evaluation image, snapshotted pristine. Then one command inside the
guest:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\verify-release.ps1 `
  -From C:\builds\Cursed_1.20.0_x64-setup.exe -To 1.21.0
```

It walks the sequence — baseline, first install, seed the data, fingerprint,
update, compare, roles, uninstall — pausing where a person has to click
something, and prints a Markdown table at the end to paste into this file.

A **skip is not a pass** and the table counts them separately. That distinction
is the entire reason this document exists.

The pass condition for the headline row is unchanged: **one click, one progress
bar, no uninstall, no repetition, no not-responding, and the app relaunches
itself on the new version with presets, custom cursors and the applied cursor
all intact.**

## A note on what the fix cannot do

`updates::settle_pending_install` reports whether an update took, by comparing
the running version against the intended one on the next launch. It cannot
detect a new version that installs and then fails to start — nothing runs to
notice, because the thing that would notice is the binary that will not run. The
previous binary is kept beside the installer for that case, but recovering it is
a manual step today. Closing that gap needs a launcher process outside the app,
which is a larger decision than the update path.
