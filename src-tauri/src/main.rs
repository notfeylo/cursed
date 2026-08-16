// A GUI app must not attach a console: `windows_subsystem = "windows"` is what
// makes the installed .exe a double-clickable desktop app rather than something
// that flashes a terminal (PRD §18).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

/// Run by the uninstaller before it deletes anything (see `installer-hooks.nsh`).
/// Same code path as Settings → Restore Windows Default, so there is only ever
/// one restore to get right.
const RESTORE_FLAG: &str = "--restore-defaults";

/// Writes a fingerprint of the data directory to the path given after it.
///
/// Run either side of an update by `scripts/verify-release.ps1` to prove that
/// nothing of the user's was touched — the check that no unit test can be,
/// because an update replaces the process that would be doing the asserting.
/// See `dataprint`.
///
/// A file rather than stdout: this binary is built with
/// `windows_subsystem = "windows"` and has no console to print to.
const DATA_PRINT_FLAG: &str = "--data-print";

fn main() {
    let arguments: Vec<String> = std::env::args().collect();

    if let Some(index) = arguments.iter().position(|a| a == DATA_PRINT_FLAG) {
        // Headless, like the restore path: no window, no tray, no watchdog.
        // Nothing here writes to the directory it is measuring.
        let Some(dest) = arguments.get(index + 1) else {
            std::process::exit(2);
        };
        let wrote = cursorforge_lib::dataprint::write_to(std::path::Path::new(dest)).is_ok();
        std::process::exit(if wrote { 0 } else { 1 });
    }

    if arguments.iter().any(|argument| argument == RESTORE_FLAG) {
        // Deliberately silent and headless: no window, no webview, no tray. It
        // must finish quickly, because an uninstaller is waiting on it.
        let restored = cursorforge_lib::cursor::restore_default().is_ok();
        let _ = cursorforge_lib::cursor::restore::deregister_our_schemes();
        // Only this channel's half of the shared record. The other channel may
        // still be installed, and it needs its own claim on the machine's
        // original scheme to survive this uninstall.
        let _ = cursorforge_lib::cursor::crosschannel::forget_this_channel();
        std::process::exit(if restored { 0 } else { 1 });
    }

    cursorforge_lib::run();
}
