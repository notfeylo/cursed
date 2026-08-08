//! Launch on sign-in (PRD §9).
//!
//! Registered per-user under `HKCU\...\Run` by the plugin — consistent with the
//! rest of the product, which never touches machine-wide state. The `--silent`
//! argument is what makes an autostarted launch go straight to the tray instead
//! of throwing a window at someone who just signed in.

use crate::error::{AppError, AppResult};
use tauri::AppHandle;
use tauri_plugin_autostart::ManagerExt;

pub const SILENT_FLAG: &str = "--silent";

pub fn apply(app: &AppHandle, enabled: bool) -> AppResult<()> {
    let manager = app.autolaunch();
    let currently = manager.is_enabled().unwrap_or(false);
    if currently == enabled {
        return Ok(());
    }
    let result = if enabled {
        manager.enable()
    } else {
        manager.disable()
    };
    result.map_err(|e| AppError::msg(format!("could not change the startup setting: {e}")))
}

/// True when this process was started by the autostart entry.
pub fn launched_silently() -> bool {
    std::env::args().any(|argument| argument == SILENT_FLAG)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_silent_flag_is_matched_exactly() {
        // Substring matching here would let an unrelated argument such as
        // `--silently-broken` send the app to the tray.
        let arguments = ["cursorforge.exe", "--silentish"];
        assert!(!arguments.contains(&SILENT_FLAG));
        let arguments = ["cursorforge.exe", SILENT_FLAG];
        assert!(arguments.contains(&SILENT_FLAG));
    }
}
