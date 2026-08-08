//! Two narrow shell affordances, kept in Rust on purpose.
//!
//! The Tauri `shell` plugin is denied outright (PRD §13.2). These two functions
//! are what remains: opening our own storage folder, and opening one of three
//! allow-listed project URLs. Neither takes an argument the webview controls —
//! callers pass a path we computed or a URL we matched against a fixed list.

use crate::error::{AppError, AppResult};
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use windows::core::PCWSTR;
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

fn execute(target: &[u16]) -> AppResult<()> {
    let verb = wide("open");
    // SAFETY: both buffers are NUL-terminated and live across the call.
    // `ShellExecuteW` returns a pseudo-HINSTANCE; values above 32 mean success.
    let result = unsafe {
        ShellExecuteW(
            None,
            PCWSTR(verb.as_ptr()),
            PCWSTR(target.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    if result.0 as isize > 32 {
        Ok(())
    } else {
        Err(AppError::Win32("Windows would not open that".into()))
    }
}

pub fn open_path(path: &Path) -> AppResult<()> {
    let mut buffer: Vec<u16> = path.as_os_str().encode_wide().collect();
    buffer.push(0);
    execute(&buffer)
}

pub fn open_url(url: &str) -> AppResult<()> {
    // Belt and braces: even though every caller matches against an allow-list,
    // refuse anything that is not plainly an https URL.
    if !url.starts_with("https://") {
        return Err(AppError::invalid("only https links are opened"));
    }
    execute(&wide(url))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_https_schemes_are_refused() {
        assert!(open_url("file:///C:/Windows/System32/cmd.exe").is_err());
        assert!(open_url("http://example.com").is_err());
        assert!(open_url("javascript:alert(1)").is_err());
    }
}
