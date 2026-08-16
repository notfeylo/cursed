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

## What has not

Every row below needs a Windows VM snapshotted clean and rolled back between
runs. **None have been run.**

| Row | Why it matters | Status |
| --- | --- | --- |
| **N → N+1, presets and applied cursor intact** | The headline. This is the one that proves the data loss is gone | ⬜ |
| One click, one progress bar, no uninstall step, no prompts | The reported symptom | ⬜ |
| App relaunches itself on the new version | `/R`, never observed | ⬜ |
| `%APPDATA%\Cursed` byte-identical across an update | The brief's §2.3 assertion; cannot be a unit test | ⬜ |
| All 17 registry paths still resolve after updating | Cursor survives the swap | ⬜ |
| Network dropped mid-download | | ⬜ |
| Disk full | | ⬜ |
| Antivirus quarantines the installer | | ⬜ |
| User cancels SmartScreen | Unsigned build, so this is the common path today | ⬜ |
| Machine sleeps mid-download | | ⬜ |
| Power loss mid-install | The one `state::store` was written for; still needs proving end to end | ⬜ |
| Update triggered twice | | ⬜ |
| Update while a cursor is applied | | ⬜ |
| Update while the catalog is rendering | | ⬜ |
| `%APPDATA%` read-only or OneDrive-redirected | | ⬜ |
| Two Windows users signed in simultaneously | Ruled out by reading `FindProcessCurrentUser`; unproven | ⬜ |
| Remote Desktop session | | ⬜ |
| App in tray vs window open | | ⬜ |

### Why they have not been run here

The development machine has the live v1.20.0 install on it, with 395 MB of the
author's own imported packs, custom cursors and the original-scheme snapshot in
`%APPDATA%\Cursed`. Testing an update path whose failure mode is *deleting that
directory* against that machine is not a test, it is a coin toss with the thing
being protected. Windows Sandbox is unavailable — this is Windows 11 Home, and
Sandbox needs Pro, the same wall `v1.7.0.md` hit.

So the matrix needs a VirtualBox VM with a Windows 11 evaluation image, as the
brief's §9 specifies.

## How to run it when there is a VM

Two releases are needed, because an update needs something to update *from*.

```powershell
# On the clean snapshot, install the older build first.
.\Cursed_1.20.0_x64-setup.exe

# Make it worth losing: apply a cursor, import an image, save a preset.
# Then record what should survive.
powershell -File scripts\verify-uninstall.ps1 -Snapshot
Get-ChildItem -Recurse "$env:APPDATA\Cursed" | Get-FileHash | Export-Csv before.csv

# Update in-app: Settings -> CHECK FOR UPDATES -> DOWNLOAD -> INSTALL & RESTART.
# Watch for: an uninstall step, any prompt, any question about keeping data.

# Afterwards
Get-ChildItem -Recurse "$env:APPDATA\Cursed" | Get-FileHash | Export-Csv after.csv
Compare-Object (Import-Csv before.csv) (Import-Csv after.csv) -Property Hash,Path
```

`settings.json`, `presets.json`, `applied.json` and `backup\original_scheme.json`
must all be unchanged. `cache\` and `logs\` are expected to differ and do not
count.

The pass condition for the headline row is the brief's own wording: **one click,
one progress bar, no uninstall, no repetition, no not-responding, and the app
relaunches itself on the new version with presets, custom cursors and the applied
cursor all intact.**

## A note on what the fix cannot do

`updates::settle_pending_install` reports whether an update took, by comparing
the running version against the intended one on the next launch. It cannot
detect a new version that installs and then fails to start — nothing runs to
notice, because the thing that would notice is the binary that will not run. The
previous binary is kept beside the installer for that case, but recovering it is
a manual step today. Closing that gap needs a launcher process outside the app,
which is a larger decision than the update path.
