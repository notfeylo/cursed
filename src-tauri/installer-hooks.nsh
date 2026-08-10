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
  ; Rendered cursors are disposable and can be large, so they go.
  RMDir /r "$APPDATA\Cursed\cache"
  ; Presets, custom cursors and settings are the user's own work and are left
  ; in place. `%APPDATA%\Cursed` can be deleted by hand if they want it
  ; gone — the Privacy Policy says so in as many words.
!macroend
