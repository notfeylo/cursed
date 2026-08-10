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
    MessageBox MB_ICONSTOP|MB_OK "Cursed needs Windows 10 version 1803 (build 17134) or newer.$\r$\n$\r$\nThis PC reports build $0. Cursed's window is built on Microsoft Edge WebView2, which Microsoft no longer supports on older versions of Windows, so it cannot run here."
    Abort

  cursed_windows_ok:
!macroend

!macro NSIS_HOOK_POSTINSTALL
  ; Nothing to do. The first launch captures the original scheme before it
  ; changes anything, so there is nothing to prepare here.
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ; Asked here, before anything is removed, and acted on in POSTUNINSTALL once
  ; the files are gone. `Var /GLOBAL` is how a variable gets declared from inside
  ; a section, which is where these macros are expanded.
  ;
  ; The default is to remove everything: someone uninstalling has said they want
  ; the app gone, and a folder of leftovers they never hear about again is not a
  ; kindness. Keeping is a deliberate choice, so it is the one that takes a
  ; click. MB_DEFBUTTON2 puts the focus on No, so Enter erases.
  Var /GLOBAL CursedKeepData
  StrCpy $CursedKeepData "0"

  ; A silent uninstall has nobody to answer, and must never sit on a dialog
  ; waiting. It takes the default: a complete removal.
  IfSilent cursed_keep_decided
  MessageBox MB_YESNO|MB_ICONQUESTION|MB_DEFBUTTON2 \
    "Keep your presets and custom cursors?$\r$\n$\r$\nChoose No to remove everything Cursed created and leave this PC exactly as it was before it was installed." \
    IDNO cursed_keep_decided
  StrCpy $CursedKeepData "1"
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
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
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
  ; cursors they built from their own images, their settings. $CursedKeepData is
  ; set by the prompt in PREUNINSTALL, which defaults to removing it.
  StrCmp $CursedKeepData "1" cursed_keep_data 0
    RMDir /r "$APPDATA\Cursed"
    Goto cursed_data_done
  cursed_keep_data:
    DetailPrint "Keeping your presets and custom cursors in $APPDATA\Cursed."
  cursed_data_done:

  ; Removes the parent only when it is already empty, so a sibling app's data is
  ; never touched.
  RMDir "$APPDATA\Cursed"
!macroend
