; NSIS hooks for the Cursed installer.
;
; The important one is PREUNINSTALL. Removing the app without putting the
; pointer scheme back would leave the registry holding paths to files the
; uninstaller is about to delete — which is how a machine ends up with an
; invisible cursor and no obvious way to fix it (PRD §4.4).
;
; The restore runs through the app itself, so the uninstaller and the Settings
; button use exactly the same code path.

!macro NSIS_HOOK_PREINSTALL
  ; Cursed installs per-user and touches no shared state, so there is nothing to
  ; prepare. What there is to do is turn away a machine that cannot run it.
  ;
  ; The floor is not ours to choose. The window is Microsoft Edge WebView2, and
  ; Microsoft ended WebView2 support for Windows 7, 8 and 8.1 — the runtime will
  ; not install there at all. Build 17134 is Windows 10 version 1803, the oldest
  ; release it does support.
  ;
  ; Without this the install *succeeds* on such a machine and the failure
  ; surfaces later as an app that starts, shows no window and exits. That reads
  ; as "this app is broken" rather than "this PC is too old", and leaves nobody
  ; anything to act on. Better to say it while there is still a dialog to say it
  ; in.
  SetRegView 64
  ReadRegStr $0 HKLM "SOFTWARE\Microsoft\Windows NT\CurrentVersion" "CurrentBuild"
  SetRegView lastused

  ; An unreadable build number is not evidence of an old machine, so it installs
  ; anyway. Blocking on a failed read would turn one unusual registry into an
  ; unusable installer, and refusing to install is the one failure the user
  ; cannot work around.
  StrCmp $0 "" cursed_windows_ok 0
  IntCmp $0 17134 cursed_windows_ok cursed_windows_old cursed_windows_ok

  cursed_windows_old:
    ; Silent first, and this is not a nicety.
    ;
    ; An in-app update runs this installer with /S, which means there is no
    ; installer window on screen for a MessageBox to belong to. NSIS shows one
    ; anyway: a modal dialog with no parent, from a process the user cannot
    ; see, while the app that started the update has already exited. The update
    ; does not fail - it waits, forever, for a click on a box nobody can find.
    ;
    ; A silent run refuses silently. The exit code is what the caller reads.
    IfSilent cursed_windows_refuse
    MessageBox MB_ICONSTOP|MB_OK "Cursed needs Windows 10 version 1803 (build 17134) or newer.$\r$\n$\r$\nThis PC reports build $0. Cursed's window is built on Microsoft Edge WebView2, which Microsoft no longer supports on older versions of Windows, so it cannot run here."
    cursed_windows_refuse:
    DetailPrint "This PC reports Windows build $0, below the 17134 floor."
    Abort

  cursed_windows_ok:
!macroend

!macro NSIS_HOOK_POSTINSTALL
  ; Nothing to do. The first launch captures the original scheme before it
  ; changes anything, so there is nothing to prepare here.
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ; An update is not an uninstall, and this hook runs during both.
  ;
  ; Tauri's installer upgrades by running the *previous* version's uninstaller
  ; with `/UPDATE` before laying down the new files. Its own app-data deletion is
  ; guarded by `$UpdateMode <> 1`; these hooks are inserted unguarded, so
  ; everything below would otherwise run on every single update.
  ;
  ; That is two disasters rather than one. Restoring the Windows scheme would put
  ; the user's pointer back to the stock arrow every time they updated, and the
  ; prompt below would offer to delete their presets and custom cursors with
  ; "remove" as the default answer. An update must not touch either.
  ${If} $UpdateMode = 1
    DetailPrint "Updating, so the cursor scheme and your data are left alone."
    Goto cursed_preuninstall_done
  ${EndIf}

  ; Asked here, before anything is removed, and acted on in POSTUNINSTALL once
  ; the files are gone. `Var /GLOBAL` is how a variable gets declared from inside
  ; a section, which is where these macros are expanded.
  ;
  ; **The default is to KEEP.** It used to be to delete, on the reasoning that
  ; somebody uninstalling wants the app gone and a folder of leftovers is not a
  ; kindness. That reasoning is sound for an uninstall somebody chose and wrong
  ; for every other way this code is reached — and it is reached other ways.
  ;
  ; Running the installer over an existing install shows an "Already Installed"
  ; page whose *pre-selected* option is "Uninstall before installing". A user
  ; downloading the latest version from the website and pressing Next through
  ; the defaults therefore lands here, and used to lose every preset, every
  ; cursor they had built, and the record of their original Windows pointers —
  ; having asked for an upgrade.
  ;
  ; Deleting somebody's work must be something they chose, not something they
  ; failed to prevent. Leftover files can be removed later by anyone who wants
  ; them gone; a preset cannot be un-deleted.
  Var /GLOBAL CursedKeepData
  StrCpy $CursedKeepData "1"

  ; A silent uninstall has nobody to answer, and must never sit on a dialog
  ; waiting. It now takes the safe default rather than the destructive one:
  ; every automated path — a management tool, a packaged upgrade, an installer
  ; running its own uninstaller — is silent, and none of those is a person
  ; asking for their data to be deleted.
  IfSilent cursed_keep_decided
  MessageBox MB_YESNO|MB_ICONQUESTION|MB_DEFBUTTON1 \
    "Also delete your presets and custom cursors?$\r$\n$\r$\nChoose No to keep them, in case you reinstall. Choose Yes to remove everything Cursed created and leave this PC exactly as it was before it was installed." \
    IDYES cursed_delete_data
  Goto cursed_keep_decided
  cursed_delete_data:
    StrCpy $CursedKeepData "0"
  cursed_keep_decided:

  DetailPrint "Restoring your original Windows pointer scheme..."
  ; Runs synchronously and exits immediately; a failure here must not block the
  ; uninstall, so the exit code is popped and ignored.
  ;
  ; Both names are tried on purpose. Builds before the executable was renamed
  ; shipped `cursorforge.exe`, and naming only the current one meant the restore
  ; silently did nothing on those installs — leaving the registry pointing at
  ; cursor files the uninstaller was about to delete, which is the exact state
  ; this hook exists to prevent.
  IfFileExists "$INSTDIR\Cursed.exe" 0 +3
    nsExec::ExecToLog '"$INSTDIR\Cursed.exe" --restore-defaults'
    Pop $0
  IfFileExists "$INSTDIR\cursorforge.exe" 0 +3
    nsExec::ExecToLog '"$INSTDIR\cursorforge.exe" --restore-defaults'
    Pop $0

  cursed_preuninstall_done:
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ; Same reason as PREUNINSTALL: an upgrade runs the old uninstaller first, and
  ; this hook is inserted without the `$UpdateMode` guard Tauri puts on its own
  ; removal. Every line below would run on every update, and the last of them
  ; deletes the user's presets and custom cursors.
  ${If} $UpdateMode = 1
    Goto cursed_postuninstall_done
  ${EndIf}

  ; Uninstalling should return the machine to its pre-install state. Everything
  ; below is something the stock per-user Tauri uninstaller does not remove.

  ; The autostart entry. Written by the autostart plugin at runtime rather than
  ; by the installer, so the uninstaller has no record of it — and a Run value
  ; pointing at a deleted .exe is a failed launch on every sign-in, forever.
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "Cursed"

  ; The WebView2 user-data folder. Keyed by bundle identifier, NOT by product
  ; name, which is why looking for `$LOCALAPPDATA\Cursed` finds nothing and the
  ; folder is missed: it is `$LOCALAPPDATA\dev.feylo.cursed\EBWebView` and it
  ; measured 72 MB on the development machine. Nothing else writes there, so the
  ; whole identifier directory goes.
  RMDir /r "$LOCALAPPDATA\dev.feylo.cursed"

  ; Rendered cursors and logs are disposable and are never worth keeping.
  RMDir /r "$APPDATA\Cursed\cache"
  RMDir /r "$APPDATA\Cursed\logs"

  ; Everything else in %APPDATA%\Cursed is the user's own work — presets, the
  ; cursors they built from their own images, their settings, and the record of
  ; what their pointers were before any of this. $CursedKeepData is set by the
  ; prompt in PREUNINSTALL, which **defaults to keeping it**.
  StrCmp $CursedKeepData "1" cursed_keep_data 0
    RMDir /r "$APPDATA\Cursed"
    Goto cursed_data_done
  cursed_keep_data:
    DetailPrint "Keeping your presets and custom cursors in $APPDATA\Cursed."
  cursed_data_done:

  ; Removes the parent only when it is already empty, so a sibling app's data is
  ; never touched.
  RMDir "$APPDATA\Cursed"

  cursed_postuninstall_done:
!macroend
