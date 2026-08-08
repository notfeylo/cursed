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
  ; Nothing to do. Cursed installs per-user and touches no shared state.
!macroend

!macro NSIS_HOOK_POSTINSTALL
  ; Nothing to do. The first launch captures the original scheme before it
  ; changes anything, so there is nothing to prepare here.
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  DetailPrint "Restoring your original Windows pointer scheme..."
  ; Runs synchronously and exits immediately; a failure here must not block the
  ; uninstall, so the exit code is popped and ignored.
  nsExec::ExecToLog '"$INSTDIR\Cursed.exe" --restore-defaults'
  Pop $0
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ; Rendered cursors are disposable and can be large, so they go.
  RMDir /r "$APPDATA\Cursed\cache"
  ; Presets, custom cursors and settings are the user's own work and are left
  ; in place. `%APPDATA%\Cursed` can be deleted by hand if they want it
  ; gone — the Privacy Policy says so in as many words.
!macroend
