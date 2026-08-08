// A GUI app must not attach a console: `windows_subsystem = "windows"` is what
// makes the installed .exe a double-clickable desktop app rather than something
// that flashes a terminal (PRD §18).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

/// Run by the uninstaller before it deletes anything (see `installer-hooks.nsh`).
/// Same code path as Settings → Restore Windows Default, so there is only ever
/// one restore to get right.
const RESTORE_FLAG: &str = "--restore-defaults";

fn main() {
    if std::env::args().any(|argument| argument == RESTORE_FLAG) {
        // Deliberately silent and headless: no window, no webview, no tray. It
        // must finish quickly, because an uninstaller is waiting on it.
        let restored = cursorforge_lib::cursor::restore_default().is_ok();
        let _ = cursorforge_lib::cursor::restore::deregister_our_schemes();
        std::process::exit(if restored { 0 } else { 1 });
    }

    cursorforge_lib::run();
}
