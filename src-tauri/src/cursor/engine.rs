//! Layer A — the live layer (PRD §4.1).
//!
//! `SetSystemCursor` swaps the pointer for the current session with no registry
//! write and no flicker, which is what makes catalog hover-preview feel instant.
//!
//! Cursed never draws a cursor (PRD §3). Everything below hands a real
//! `.cur` / `.ani` file to Windows and lets the GPU's hardware cursor plane do
//! the drawing — which is why added input latency is zero by construction.

use crate::cursor::roles::Role;
use crate::cursor::scheme::CursorSet;
use crate::error::{AppError, AppResult};
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use windows::core::PCWSTR;
use windows::Win32::Foundation::HANDLE;
use windows::Win32::UI::WindowsAndMessaging::{
    CopyIcon, DestroyCursor, LoadImageW, SetSystemCursor, HCURSOR, HICON, IMAGE_CURSOR,
    LR_DEFAULTSIZE, LR_LOADFROMFILE, SYSTEM_CURSOR_ID,
};

fn wide(path: &Path) -> Vec<u16> {
    path.as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

/// Loads a cursor file at a specific pixel size.
///
/// A multi-resolution `.cur` holds 8 images; passing an explicit `size` lets
/// Windows pick the closest one instead of falling back to the 32 px system
/// metric. That is the whole reason cursors stay crisp at 200% DPI.
fn load(path: &Path, size: u32) -> AppResult<HANDLE> {
    let wide_path = wide(path);
    let (cx, cy, flags) = if size == 0 {
        (0, 0, LR_LOADFROMFILE | LR_DEFAULTSIZE)
    } else {
        (size as i32, size as i32, LR_LOADFROMFILE)
    };

    // SAFETY: `wide_path` is a NUL-terminated UTF-16 buffer that outlives the
    // call. `LoadImageW` with LR_LOADFROMFILE only reads the file; on failure it
    // returns Err and allocates nothing.
    let handle = unsafe { LoadImageW(None, PCWSTR(wide_path.as_ptr()), IMAGE_CURSOR, cx, cy, flags) }
        .map_err(|e| {
            AppError::Win32(format!(
                "{} could not be loaded as a cursor ({})",
                path.file_name().unwrap_or_default().to_string_lossy(),
                e.message()
            ))
        })?;

    if handle.is_invalid() {
        return Err(AppError::Win32(format!(
            "{} is not a valid cursor file",
            path.file_name().unwrap_or_default().to_string_lossy()
        )));
    }
    Ok(handle)
}

/// Round-trips a generated file through Windows' own loader.
///
/// PRD §6.1 step 6: never install a cursor we have not proved Windows will
/// accept. This is the difference between a clear error message and a machine
/// left with an invisible pointer.
pub fn verify_loadable(path: &Path) -> AppResult<()> {
    let handle = load(path, 0)?;
    // SAFETY: we own `handle` — nothing else has taken it — so destroying it here
    // is correct and required to avoid leaking a GDI object per verification.
    unsafe { DestroyCursor(HCURSOR(handle.0))? };
    Ok(())
}

/// Applies one role to the live session.
///
/// The trap this function exists to contain: **`SetSystemCursor` takes ownership
/// of the handle and destroys it.** Handing it the handle we loaded would leave
/// us holding a dangling handle, and reusing that handle across roles corrupts
/// the session's cursor table. So we give it a `CopyIcon` duplicate and destroy
/// our own original — every role, every apply, no exceptions.
pub fn set_role(role: Role, path: &Path, size: u32) -> AppResult<()> {
    let original = load(path, size)?;

    // SAFETY: `original` is a live cursor handle we own. `CopyIcon` returns an
    // independent handle; ownership of that copy transfers to `SetSystemCursor`,
    // and we destroy the original ourselves regardless of how the set went.
    //
    // `CopyIcon` gets the same treatment as `SetSystemCursor` rather than a `?`.
    // Using `?` here returned early with a bare Win32 error, skipping the
    // best-effort handling below — so a cursor Windows would not duplicate (some
    // `.ani` files, and hand-made cursors from elsewhere) failed the whole apply
    // with "Windows refused it without saying why", even for the three roles
    // that are allowed to fail.
    let result = unsafe {
        match CopyIcon(HICON(original.0)) {
            Ok(copy) => SetSystemCursor(HCURSOR(copy.0), SYSTEM_CURSOR_ID(role.ocr_id())),
            Err(e) => Err(e),
        }
    };

    // SAFETY: still ours, still valid — `SetSystemCursor` only consumed the copy.
    unsafe { DestroyCursor(HCURSOR(original.0)).ok() };

    match result {
        Ok(()) => Ok(()),
        // Pin and Person are not documented SetSystemCursor targets. They are
        // still written to the registry, so they take effect on the next reload;
        // refusing the whole apply over them would be wrong.
        Err(_) if role.live_layer_is_best_effort() => Ok(()),
        Err(e) => Err(AppError::Win32(format!(
            "{role} could not be applied: {}",
            crate::error::describe_win32(&e)
        ))),
    }
}

/// Applies a whole set to the live session.
///
/// A single failing role does not abort the run — a half-applied scheme with one
/// stock cursor is strictly better than a half-applied scheme that stopped
/// mid-way. Failures are collected and reported once.
pub fn apply_live(set: &CursorSet, size: u32) -> AppResult<()> {
    let mut failures = Vec::new();
    for (role, path) in &set.files {
        if let Err(e) = set_role(*role, path, size) {
            failures.push(format!("{role}: {e}"));
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(AppError::Win32(failures.join("; ")))
    }
}

/// Drops every live override and reloads from the registry.
pub fn revert_live() -> AppResult<()> {
    crate::cursor::scheme::reload_from_registry()
}

/// The system cursor size Windows is currently drawing at, in pixels.
pub fn effective_size(configured: Option<u32>) -> u32 {
    configured
        .or_else(|| crate::cursor::scheme::read_base_size().ok())
        .unwrap_or(32)
        .clamp(32, 256)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_file_is_an_error_not_a_panic() {
        let missing = Path::new(r"C:\this\does\not\exist\nope.cur");
        assert!(load(missing, 32).is_err());
        assert!(verify_loadable(missing).is_err());
    }

    #[test]
    fn a_non_cursor_file_is_rejected() {
        let dir = std::env::temp_dir().join("cursorforge-tests");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("engine-not-a-cursor.cur");
        std::fs::write(&path, b"this is plainly not a cursor").unwrap();
        assert!(verify_loadable(&path).is_err());
        let _ = std::fs::remove_file(&path);
    }
}
