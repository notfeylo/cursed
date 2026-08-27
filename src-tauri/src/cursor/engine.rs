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
    CopyIcon, DestroyCursor, LoadCursorFromFileW, LoadImageW, SetSystemCursor, HCURSOR, HICON,
    IMAGE_CURSOR, LR_DEFAULTSIZE, LR_LOADFROMFILE, SYSTEM_CURSOR_ID,
};

fn wide(path: &Path) -> Vec<u16> {
    path.as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

/// Whether this file is an animated cursor, which needs a different loader.
pub fn is_animated(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("ani"))
}

/// Installs an animated cursor for one role, preserving its frames.
///
/// Deliberately does three things differently from the static path, and each one
/// matters:
///
///  * `LoadCursorFromFileW`, not `LoadImageW`. It is the function documented for
///    cursor files and it keeps the frame list. `LoadImageW` with a requested
///    size has to rescale, an `.ani` cannot be rescaled, and what comes back is
///    a single frame.
///  * **No size.** There is nothing to choose between — an `.ani` holds one
///    resolution, which is why the build step renders it at the target size
///    rather than shipping a ladder.
///  * **No `CopyIcon`.** The duplicate it returns is a still. The handle goes
///    straight to `SetSystemCursor`, which takes ownership of it.
fn set_animated_role(role: Role, path: &Path) -> AppResult<()> {
    let wide_path = wide(path);

    // SAFETY: `wide_path` is a NUL-terminated UTF-16 buffer that outlives the
    // call, and the function only reads the file.
    let cursor = unsafe { LoadCursorFromFileW(PCWSTR(wide_path.as_ptr())) }.map_err(|e| {
        AppError::Win32(format!(
            "{} could not be loaded as an animated cursor ({})",
            path.file_name().unwrap_or_default().to_string_lossy(),
            e.message()
        ))
    })?;

    // SAFETY: ownership of `cursor` transfers to `SetSystemCursor` when it
    // succeeds, so it must not be destroyed here on the happy path.
    let result = unsafe { SetSystemCursor(cursor, SYSTEM_CURSOR_ID(role.ocr_id())) };

    match result {
        Ok(()) => Ok(()),
        Err(e) => {
            // The call failed, so ownership did not transfer and the handle is
            // still ours to release.
            unsafe { DestroyCursor(cursor).ok() };
            if role.live_layer_is_best_effort() {
                Ok(())
            } else {
                Err(AppError::Win32(format!(
                    "{role} could not be applied: {}",
                    crate::error::describe_win32(&e)
                )))
            }
        }
    }
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
    // Verified through the same loader that will apply it. Checking an `.ani`
    // with `LoadImageW` proves only that a still frame can be read out of it,
    // which is not the question being asked.
    if is_animated(path) {
        let wide_path = wide(path);
        // SAFETY: NUL-terminated buffer that outlives the call; read-only.
        let cursor = unsafe { LoadCursorFromFileW(PCWSTR(wide_path.as_ptr())) }.map_err(|e| {
            AppError::Win32(format!(
                "{} is not a valid animated cursor ({})",
                path.file_name().unwrap_or_default().to_string_lossy(),
                e.message()
            ))
        })?;
        // SAFETY: we own it; nothing else has taken it.
        unsafe { DestroyCursor(cursor).ok() };
        return Ok(());
    }

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
/// The size Windows will load a cursor at when it is not told one.
///
/// `SM_CXCURSOR`. Read every time rather than cached, because it does change
/// between sessions — it simply does not change *within* one, which is the
/// property that matters here.
fn system_cursor_px() -> u32 {
    use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXCURSOR};
    // SAFETY: no pointers, no allocation; reads one integer system metric.
    let px = unsafe { GetSystemMetrics(SM_CXCURSOR) };
    if px > 0 {
        px as u32
    } else {
        32
    }
}

pub fn set_role(role: Role, path: &Path, size: u32) -> AppResult<()> {
    // An animated cursor takes the other route, and has to.
    //
    // Applying an animated pack used to install a frozen first frame *over* the
    // animated cursor Windows had already loaded from the registry moments
    // earlier, because both `LoadImageW`-with-a-size and `CopyIcon` flatten an
    // `.ani`. The animation only came back when the watchdog next reloaded the
    // scheme — which is what the half-minute pause before anything moved
    // actually was. The pack was never static; it was being flattened after the
    // fact.
    if is_animated(path) {
        // ...and that route cannot be given a size, which decides whether it
        // should run at all.
        //
        // `LoadCursorFromFileW` takes no dimensions. It loads at the system
        // cursor metric, and `SM_CXCURSOR` does not follow `CursorBaseSize`
        // inside a process that is already running — measured on Windows 11
        // build 26200 it reports 32 whatever the registry says and however many
        // times `SPI_SETCURSORS` is broadcast. So this path can only ever
        // produce a 32 px cursor.
        //
        // Windows itself has no such trouble: loading the scheme from the
        // registry, it applies `CursorBaseSize` correctly, animation and all. By
        // the time this runs `commit` has already written the scheme, written
        // the size and broadcast the change, so the correctly-sized animated
        // cursor is *already on screen* — and installing a live override here
        // replaces it with a 32 px one.
        //
        // That is the whole reason animated cursors ignored the size control
        // while static ones obeyed it: a static `.cur` goes through `LoadImageW`
        // with an explicit size and is unaffected by any of this.
        //
        // At the default size the override is still worth doing. It is instant,
        // and it is what lets a hover preview animate without touching the
        // registry at all.
        if size != system_cursor_px() {
            log::debug!("{role}: left to the registry, which can size an .ani ({size}px)");
            return Ok(());
        }
        return set_animated_role(role, path);
    }

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

/// Which caller installed a live override.
///
/// Carried into the log so a pointer that is wrong for a while can be traced to
/// the code that made it wrong. "It is blurry while the app has focus and
/// correct once it does not" is a sentence about *which* of these ran last, and
/// without the label the log cannot answer it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// Hovering a tile in the catalog. Reverted by `clear_preview`.
    Preview,
    /// A committed apply: registry written and broadcast first.
    Commit,
    /// The watchdog putting a scheme back that something else changed.
    Watchdog,
}

impl std::fmt::Display for Source {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Source::Preview => "preview",
            Source::Commit => "commit",
            Source::Watchdog => "watchdog",
        })
    }
}

/// Applies a whole set to the live session.
///
/// A single failing role does not abort the run — a half-applied scheme with one
/// stock cursor is strictly better than a half-applied scheme that stopped
/// mid-way. Failures are collected and reported once.
pub fn apply_live(set: &CursorSet, size: u32, source: Source) -> AppResult<()> {
    let mut failures = Vec::new();
    log::debug!("{source}: installing {} roles at {size}px", set.files.len());
    for (role, path) in &set.files {
        // Per role, not one size for the set. A static `.cur` carries the whole
        // resolution ladder, so which image Windows draws is decided by the size
        // asked for here — passing the pointer's size would hand back a 128 px
        // I-beam out of a file that also contains a 32 px one.
        let rung = crate::packs::catalog::glyph_size(*role, size);
        log::debug!(
            "{source}: {role} <- {} at {rung}px",
            path.file_name().unwrap_or_default().to_string_lossy()
        );
        if let Err(e) = set_role(*role, path, rung) {
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
    let asked = configured
        .or_else(|| crate::cursor::scheme::read_base_size().ok())
        .unwrap_or(32)
        .clamp(crate::state::settings::MIN_CURSOR_PX, crate::state::settings::MAX_CURSOR_PX);
    // Snapped to a rung, for the same reason `Settings::sanitised` snaps: a
    // `.cur` holds a fixed set of resolutions, and a size between two of them
    // is one the shell has to scale the nearest entry into.
    //
    // Both places, not one. `sanitised` catches the size this app was told to
    // use; this catches the one it inherits when it was told nothing and reads
    // `CursorBaseSize` — which is where Windows' own accessibility slider puts
    // its answer, and that slider does not know our ladder exists.
    crate::build::pipeline::nearest_size(asked)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The bug this snapping exists for: the size control moves in twos and the
    /// ladder has rungs, so most positions asked Windows to scale a cursor that
    /// could have been drawn exactly.
    #[test]
    fn every_size_that_can_be_asked_for_lands_on_a_rung() {
        use crate::build::cur_writer::TARGET_SIZES;
        use crate::state::settings::{MAX_CURSOR_PX, MIN_CURSOR_PX};
        for asked in MIN_CURSOR_PX..=MAX_CURSOR_PX {
            let got = effective_size(Some(asked));
            assert!(
                TARGET_SIZES.contains(&got),
                "{asked} px became {got} px, which no cursor file carries"
            );
        }
    }

    /// 38 is what the machine that reported the blurry pointer was sitting on.
    #[test]
    fn the_size_that_was_reported_blurry_snaps_to_one_that_exists() {
        assert_eq!(effective_size(Some(38)), 32);
        assert_eq!(effective_size(Some(78)), 64);
        assert_eq!(effective_size(Some(120)), 128);
    }

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

#[cfg(test)]
mod animated_tests {
    use super::*;

    #[test]
    fn only_ani_files_take_the_animated_path() {
        assert!(is_animated(Path::new(r"C:\x\arrow.ani")));
        assert!(is_animated(Path::new(r"C:\x\ARROW.ANI")), "extension is case-insensitive");
        assert!(!is_animated(Path::new(r"C:\x\arrow.cur")));
        assert!(!is_animated(Path::new(r"C:\x\arrow")));
        // A name that merely contains "ani" is not an animated cursor.
        assert!(!is_animated(Path::new(r"C:\x\animated-looking.cur")));
    }

    /// The regression this guards: an animated cursor applied through the
    /// static path is flattened to its first frame, and the pack only starts
    /// moving when the watchdog next reloads the scheme from the registry.
    #[test]
    fn a_real_animated_cursor_loads_with_its_frames_intact() {
        let stock = Path::new(r"C:\Windows\Cursors\aero_busy.ani");
        if !stock.exists() {
            return; // not every Windows install ships the Aero set
        }
        assert!(is_animated(stock));
        assert!(verify_loadable(stock).is_ok(), "Windows must accept its own busy cursor");
    }
}
