# The update path — diagnosis

**Answers §1 of the update-path research brief.**

> **Status.** This document records the state of the update path *before* the
> fix, and is kept as written. §2 has since landed against it:
> `INSTALLER_ARGUMENTS` in `src/updates.rs` now passes `/UPDATE /P /R /NS`,
> `install_update` verifies before tearing anything down, and `bundle.targets`
> is NSIS alone. Six tests assert those invariants. **None of it is verified on
> a real machine yet** — that is §2.5, and until it is run this is a fix that
> compiles and passes unit tests, not a fix that is known to work.

Inspected at v1.20.0, working tree `C:\Users\huzai\Downloads\CURSORFORGE\cursorforge`.
Line numbers in `installer.nsi` and `utils.nsh` refer to the **generated** script at
`src-tauri/target/x86_64-pc-windows-msvc/release/nsis/x64/`, which is the one the
shipping installer is compiled from (`PRODUCTNAME "Cursed"`, `INSTALLMODE
"currentUser"`). It is regenerated on every build and is not in version control.

---

## The short version

`updates::verify_and_launch` runs the downloaded installer **with no command-line
arguments at all**. Every symptom in the brief follows from that one line, through
the stock Tauri NSIS template, in a chain that ends with `RMDir /r "$APPDATA\Cursed"`.

The update is not merely ugly. **It restores the user's original Windows pointer
scheme and then offers to delete their presets, custom cursors and the
original-scheme snapshot, with "delete" as the default answer.** The guard written
to prevent exactly this cannot fire, because the flag it keys on is never passed.

Two of the brief's four hypothesised failure modes are live; two are already
ruled out by the current Tauri template.

---

## 1.1 What is actually shipping

### Does the release publish both an NSIS `.exe` and an MSI? What does the update feed point at?

**Published: NSIS only.** `gh release view v1.20.0` returns eight assets and no MSI:

```
Cursed-Setup.exe                11588609
Cursed-Setup-ARM64.exe          11187852
Cursed-Setup-x86.exe            11419954
Cursed-Setup-Offline-x64.exe   223967479
Cursed_1.20.0_x64-setup.exe     11588609
Cursed_1.20.0_arm64-setup.exe   11187852
Cursed_1.20.0_x86-setup.exe     11419954
SHA256SUMS.txt                        638
```

**But the config asks for both** — `src-tauri/tauri.conf.json:48`:

```json
"targets": ["nsis", "msi"],
```

The release escapes it because `scripts/build-release.mjs:76` overrides the target
list per invocation:

```js
run(["--target", triple, "--bundles", "nsis"]);
```

CI does not. `.github/workflows/build.yml` runs a bare `npm run tauri build`, so an
MSI *is* produced on every push and uploaded as a workflow artifact:

```yaml
      - uses: actions/upload-artifact@v4
        with:
          name: cursed-installers
          path: |
            src-tauri/target/release/bundle/nsis/*-setup.exe
            src-tauri/target/release/bundle/msi/*.msi
```

There is no updater feed in the Tauri sense. The app asks the GitHub releases API
directly and re-validates the asset name it gets back — `src-tauri/src/updates.rs:411`:

```rust
fn is_our_installer(name: &str) -> bool {
    name.starts_with("Cursed_")
        && name.ends_with(INSTALLER_SUFFIX)
        && !name.contains('/')
        && !name.contains('\\')
        && !name.contains("..")
        && name.chars().all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))
}
```

`INSTALLER_SUFFIX` is `_x64-setup.exe` / `_arm64-setup.exe` / `_x86-setup.exe`,
chosen by `#[cfg(target_arch)]` at `updates.rs:399-404`. An MSI cannot satisfy that
predicate, so the updater cannot download one even if a release published it.

### `installMode` in the bundle config

`currentUser` — `tauri.conf.json:69`:

```json
      "nsis": {
        "installMode": "currentUser",
```

Which the generated script turns into a non-elevating manifest, `installer.nsi:104`:

```nsis
!if "${INSTALLMODE}" == "currentUser"
  RequestExecutionLevel user
!endif
```

### `installMode` under the updater's Windows config

**There is none.** `tauri.conf.json:79` is `"plugins": {}` — the Tauri updater
plugin is not installed and not configured, and `"createUpdaterArtifacts": false`
(line 49) means no updater manifest is generated. There is no config field to set;
the arguments are whatever the app passes at the call site, and it passes none.

### Is `deleteAppDataOnUninstall` set?

**Not in the config**, so Tauri's own deletion is inert by default —
`installer.nsi:823` gates it on a checkbox the user has to tick:

```nsis
  ${If} $DeleteAppDataCheckboxState = 1
  ${AndIf} $UpdateMode <> 1
```

**That is not the risk.** The risk is `src-tauri/installer-hooks.nsh`, which deletes
app data itself, on its own default, and whose only guard is `$UpdateMode` —
lines 73-83 and 135-136:

```nsis
  Var /GLOBAL CursedKeepData
  StrCpy $CursedKeepData "0"

  IfSilent cursed_keep_decided
  MessageBox MB_YESNO|MB_ICONQUESTION|MB_DEFBUTTON2 \
    "Keep your presets and custom cursors?..." \
    IDNO cursed_keep_decided
  StrCpy $CursedKeepData "1"
  cursed_keep_decided:
```

```nsis
  StrCmp $CursedKeepData "1" cursed_keep_data 0
    RMDir /r "$APPDATA\Cursed"
```

The default is `"0"` — remove — and `MB_DEFBUTTON2` puts the focus on **No**, so
pressing Enter erases everything. That is a deliberate and correct choice *for an
uninstall*. §1.3 shows it running during an **update**.

### Is the updater the Tauri plugin, or hand-rolled?

**Hand-rolled, on WinHTTP.** `src-tauri/updates.rs:655` is the whole launch path:

```rust
pub fn verify_and_launch(tag: &str, asset: &str) -> AppResult<()> {
    let file = download_dir()?.join(asset);
    if !is_our_installer(asset) || !file.exists() {
        return Err(AppError::invalid("there is no downloaded installer to run"));
    }

    let expected = with_retry("checksum fetch", || published_hash(tag, asset))?;
    let actual = sha256_file(&file)?;
    if !crate::hash::hex_eq(&actual, &expected) {
        let _ = std::fs::remove_file(&file);
        return Err(AppError::msg(
            "the downloaded installer does not match the checksum published with the release, so it was deleted",
        ));
    }

    let child = std::process::Command::new(&file)
        .current_dir(file.parent().unwrap_or(&file))
        .spawn()
        .map_err(|e| AppError::msg(format!("the installer would not start: {e}")))?;

    log::info!("update: installer {} started as pid {}", file.display(), child.id());
    Ok(())
}
```

**`Command::new(&file).spawn()` — no `.arg()` anywhere.** No `/P`, no `/S`, no `/R`,
no `/NS`, no `/UPDATE`, no `/ARGS`. This is the root cause of everything below.

The verification either side of it is sound: the file is SHA-256'd against
`SHA256SUMS.txt` from the same release and deleted on mismatch, and `CreateProcess`
is used rather than `ShellExecuteW` for the documented reason. The problem is not
what it checks. It is what it omits.

### Custom `installer.nsi` or `installerHooks`?

Stock template, plus hooks. `tauri.conf.json:74`:

```json
        "installerHooks": "installer-hooks.nsh",
```

There is no `installer.nsi` in the repository — the only copies are generated under
`target/`. `installer-hooks.nsh` implements all four hook macros: `PREINSTALL`
(a Windows 10 1803 build-number floor), `POSTINSTALL` (empty), `PREUNINSTALL` and
`POSTUNINSTALL`.

---

## 1.2 The four documented failure modes

### A. Mixed NSIS + MSI artifacts → duplicate installs — **RULED OUT for the update path**

No MSI is published to any release, and `is_our_installer` cannot match one. A user
cannot be handed an MSI by the updater.

**Latent, not live.** `bundle.targets` still lists `"msi"`, so every CI build
produces one and uploads it as a workflow artifact. Anyone who downloads the CI
artifact rather than the release gets an MSI that installs to a different location
than the NSIS build — the exact duplicate-install condition, reachable by hand
rather than by the updater. It also costs build time on every push for an artifact
that is never shipped.

### B. Process-kill race → "failed to kill, close it first" — **CONFIRMED LIVE**

Two separate problems, one structural and one a race.

**The app launches the installer and only then exits** — `commands.rs:817`:

```rust
pub fn install_update(app: AppHandle, version: String, installer: String) -> AppResult<()> {
    updates::verify_and_launch(&version, &installer)?;
    crate::begin_shutdown();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(1_000));
        app.exit(0);
    });
    Ok(())
}
```

The ordering is backwards relative to the brief's §2.2: the installer is running
before the app has begun shutting down, and the one-second sleep is a guess, not a
verification. Nothing polls for the process to be gone, and nothing reports a
failure if it is not.

**The template prompts about it in exactly the mode the app invokes.**
`installer.nsi:636` and `:754` call `CheckIfAppIsRunning` in both the install and
uninstall sections; `utils.nsh:39`:

```nsis
  ${If} $R0 = 0
      IfSilent kill_${UniqueID} 0
      ${IfThen} $PassiveMode != 1 ${|} MessageBox MB_OKCANCEL $R2 IDOK kill_${UniqueID} IDCANCEL cancel_${UniqueID} ${|}
```

The MessageBox is skipped only when silent **or** passive. The app passes neither,
so a running app produces a modal "Cursed is running — OK to close it?" prompt.
Whether it appears depends on whether the user clicked through the wizard's pages
faster than the one-second timer, which is why the brief describes it as
intermittent.

The TOCTOU failure the brief cites is downstream of the same call — `utils.nsh:50-64`
aborts with "failed to kill" if `KillProcessCurrentUser` returns anything other
than 0 or 2.

### C. Elevation → OS error 740 — **RULED OUT**

`installMode` is `currentUser` and the generated manifest is
`RequestExecutionLevel user` (`installer.nsi:104-106`). No UAC prompt is possible,
and error 740 cannot occur. `MULTIUSER_EXECUTIONLEVEL Highest` at `installer.nsi:121`
is inside `!if "${INSTALLMODE}" == "both"` and is not compiled.

### D. Multi-user / process-name matching — **RULED OUT**

The brief describes matching on process name only. The current template scopes to
the session owner — `utils.nsh:33`:

```nsis
  !if "${INSTALLMODE}" == "currentUser"
    nsis_tauri_utils::FindProcessCurrentUser "${executableName}"
  !else
    nsis_tauri_utils::FindProcess "${executableName}"
  !endif
```

and the same split for `KillProcessCurrentUser` at line 43. Because this build is
`currentUser`, a second signed-in Windows user running Cursed is not seen and not
terminated. This appears to be a template improvement since the issues the brief
draws on.

---

## 1.3 The current update, step by step

### 1. What downloads it, and is it verified before execution?

`updates::download` (`updates.rs:532`) fetches over WinHTTP into
`%APPDATA%\Cursed\updates\<asset>`, retrying, refusing anything under 512 KB as
truncated. `verify_and_launch` then hashes it and compares against `SHA256SUMS.txt`
from the same release, deleting the file on mismatch.

**Verified before execution: yes, against a checksum.** Note the brief's §7 point
stands — a checksum fetched from the same host as the file protects against
corruption, not against someone who controls both. There is no signature.

### 2. Does the app exit before the installer starts, and does it exit completely?

**No, and not verified.** See failure mode B. The installer starts first; the app
exits one second later; nothing confirms the process is gone.

### 3. What exact command line launches the installer?

```
C:\Users\<user>\AppData\Roaming\Cursed\updates\Cursed_1.20.0_x64-setup.exe
```

with the working directory set to that folder, and **no arguments**.

### 4. Does the NSIS script hit its uninstall section during an update?

**Yes — by default, on every update.** This is the core finding, and it is four
steps.

**Step 1 — `$UpdateMode` is 0.** It is set from the command line and nowhere else.
`installer.nsi:479` (installer) and `:742` (uninstaller) are identical:

```nsis
  ${GetOptions} $CMDLINE "/UPDATE" $UpdateMode
  ${IfNot} ${Errors}
    StrCpy $UpdateMode 1
  ${EndIf}
```

No `/UPDATE` on the command line means `$UpdateMode = 0` for the whole run.

**Step 2 — the "uninstall first" radio button is pre-selected.** For an upgrade,
`installer.nsi:237`:

```nsis
  ; Upgrading
  ${ElseIf} $R0 = 1
    StrCpy $R1 "$(olderOrUnknownVersionInstalled)"
    StrCpy $R2 "$(uninstallBeforeInstalling)"
    StrCpy $R3 "$(dontUninstall)"
```

`$R2` is the **first** radio, and the first radio is checked on entry —
`installer.nsi:287`:

```nsis
    ${If} $ReinstallPageCheck <> 2
      SendMessage $R2 ${BM_SETCHECK} ${BST_CHECKED} 0
```

So the option presented, focused and pre-selected is *Uninstall before installing*.
A user pressing Next accepts it. `PageLeaveReinstall` at `:328` then routes that
choice to `reinst_uninstall`.

Had `/UPDATE` been passed, `:314` would have short-circuited the whole page:

```nsis
  ; In update mode, always proceeds without uninstalling
  ${If} $UpdateMode = 1
    Goto reinst_done
  ${EndIf}
```

**Step 3 — the old uninstaller is run without `/UPDATE`.** `installer.nsi:349`:

```nsis
      ReadRegStr $4 SHCTX "${MANUPRODUCTKEY}" ""
      ReadRegStr $R1 SHCTX "${UNINSTKEY}" "UninstallString"
      ${IfThen} $UpdateMode = 1 ${|} StrCpy $R1 "$R1 /UPDATE" ${|} ; append /UPDATE
      ${IfThen} $PassiveMode = 1 ${|} StrCpy $R1 "$R1 /P" ${|} ; append /P
      StrCpy $R1 "$R1 _?=$4" ; append uninstall directory
      ExecWait '$R1' $0
```

`/UPDATE` is appended **only if the installer itself received it**. It did not, so
the uninstaller is launched without it, and parses `$UpdateMode = 0` in its own
`un.onInit`.

**Step 4 — the hooks run in full uninstall mode.** `installer-hooks.nsh:60` is
guarded on precisely the variable that is now 0:

```nsis
  ${If} $UpdateMode = 1
    DetailPrint "Updating, so the cursor scheme and your data are left alone."
    Goto cursed_preuninstall_done
  ${EndIf}
```

The guard does not fire. What runs instead, during what the user believes is an
update:

- `PREUNINSTALL` executes `"$INSTDIR\Cursed.exe" --restore-defaults`, which
  **puts the machine back on the stock Windows pointer scheme**.
- `PREUNINSTALL` shows the keep-your-data MessageBox, defaulting to **No**.
- `POSTUNINSTALL` deletes `$LOCALAPPDATA\dev.feylo.cursed`, `$APPDATA\Cursed\cache`
  and `$APPDATA\Cursed\logs` unconditionally.
- `POSTUNINSTALL` runs `RMDir /r "$APPDATA\Cursed"` unless the user actively chose
  to keep it — taking presets, custom cursors, `settings.json`, `applied.json` and
  **`backup\original_scheme.json`**, which §6 of the brief correctly calls
  irreplaceable.

The comment at `installer-hooks.nsh:49-59` describes this exact disaster and
believes it has been prevented. The guard is correct. It is simply never reached,
because the flag it depends on is decided a process and a half away, in a `spawn()`
call with no arguments.

### 5. What relaunches the app?

**Nothing.** `/R` is not passed, and it is only honoured in silent or passive mode
anyway. The app does not come back on its own after an update.

Two further consequences of the missing flags:

- Shortcuts are recreated on every update. `installer.nsi:893` and `:922` skip
  shortcut creation when `$UpdateMode = 1` or `$NoShortcutMode = 1`; neither is set.
- The installer does not auto-close. `installer.nsi:844` sets `SetAutoClose true`
  only for passive or update mode.

### 6. What happens if any step fails?

Download and checksum failures are surfaced properly — recorded in the shared update
state, logged, and the file deleted on a checksum mismatch. **After `spawn()`
succeeds, nothing is monitored.** The app has already committed to exiting one
second later, so an installer that aborts — the user cancelling the "app is running"
prompt, the uninstaller failing, SmartScreen — leaves the app closed, the update not
installed, and no error anywhere. There is no rollback and no post-update
verification.

---

## What this means for §2

The brief asks for passive mode, no uninstall, no repetition, and a relaunch. Every
one of those is a flag that is currently not being passed, on `updates.rs:680`:

| Flag | Currently | Effect of adding it |
|---|---|---|
| `/UPDATE` | absent | `$UpdateMode = 1` end to end: no reinstall page, no uninstaller run, hooks correctly skipped, shortcuts preserved |
| `/P` | absent | One progress bar; suppresses the reinstall page and the app-is-running prompt |
| `/R` | absent | The app relaunches itself after installing |
| `/NS` | absent | Stops shortcuts being recreated every update |
| `/ARGS` | absent | Could restore `--silent` when updating from the tray |

`/UPDATE` alone removes the data loss. `/P` removes the prompt storm. `/R` returns
the app. This is consistent with the brief's §2.1, including its warning to verify
the arguments at the call site rather than trusting a config field — here there is
no config field at all, only the call site.

Two things the flags do **not** fix, and which §2 should still address:

1. **The exit ordering** (failure mode B). The installer must not start until the
   process is confirmed gone, with a timeout and a real error if it is not.
2. **No post-launch verification or rollback** (§2.4). Nothing checks that the new
   version started.

---

## Confidence, and what is not yet proven

Everything above is read from the shipping configuration, the committed hooks, the
committed Rust, and the generated NSIS script that the shipping installer is
compiled from. The `$UpdateMode` chain is traced end to end through quoted template
code rather than inferred.

**Not verified by observation.** No update has been run on a clean VM as part of
this diagnosis, so the following are deductions from the code, not measurements:

- that the reinstall page's pre-selected radio is what users are actually accepting
- the precise ordering of the app-is-running prompt against the one-second exit
  timer, which is a race and will not reproduce identically every time
- whether `%APPDATA%\Cursed` has in fact been destroyed on a real update, as
  opposed to being destroyed by the code path shown above

The first item of §9's gate — N → N+1 on a snapshotted VM, checking that presets and
the applied cursor survive — is what would turn these deductions into evidence, and
it should be run *before* the fix as well as after, so there is a recorded
before-and-after rather than only a claim.
