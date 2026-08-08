//! Reading an existing `.cur` or `.ani` back into pixels.
//!
//! The writers in this module tree only ever produce cursor files; importing
//! somebody else's needs the opposite direction. Rather than parse the format a
//! second time — and inherit every quirk of every tool that has ever written one
//! — this asks Windows to draw the cursor into a bitmap we own.
//!
//! Letting the OS decode has two real advantages: `.ani` frames, palette files,
//! monochrome masks and 256-colour oddities all come back correctly, and a file
//! Windows cannot draw is exactly the file we must refuse to import anyway.

use crate::build::bitmap::Bitmap;
use crate::error::{AppError, AppResult};
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use windows::core::PCWSTR;
use windows::Win32::Foundation::HANDLE;
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, SelectObject, BITMAPINFO,
    BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HBITMAP, HGDIOBJ,
};
use windows::Win32::UI::WindowsAndMessaging::{
    DestroyCursor, DrawIconEx, GetIconInfo, ICONINFO, IMAGE_CURSOR, LR_LOADFROMFILE, LoadImageW,
    DI_NORMAL, HCURSOR, HICON,
};

fn wide(path: &Path) -> Vec<u16> {
    path.as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

/// Renders a cursor file into an RGBA bitmap of `size` pixels.
///
/// For an `.ani` this is the first frame, which is what a catalog tile wants.
pub fn read(path: &Path, size: u32) -> AppResult<Bitmap> {
    let size = size.clamp(16, 256);
    let wide_path = wide(path);

    // SAFETY: `wide_path` is NUL-terminated and outlives the call; the cursor
    // handle is destroyed on every path out of this function.
    let handle: HANDLE = unsafe {
        LoadImageW(
            None,
            PCWSTR(wide_path.as_ptr()),
            IMAGE_CURSOR,
            size as i32,
            size as i32,
            LR_LOADFROMFILE,
        )
    }
    .map_err(|e| {
        AppError::invalid(format!(
            "{} is not a cursor Windows can read ({})",
            path.file_name().unwrap_or_default().to_string_lossy(),
            e.message()
        ))
    })?;

    if handle.is_invalid() {
        return Err(AppError::invalid("that file is not a usable cursor"));
    }

    let result = draw_to_bitmap(HCURSOR(handle.0), size);
    // SAFETY: we own this handle and nothing else took it.
    unsafe { DestroyCursor(HCURSOR(handle.0)).ok() };
    result
}

/// Draws a cursor into a top-down 32-bit DIB and copies the pixels out.
fn draw_to_bitmap(cursor: HCURSOR, size: u32) -> AppResult<Bitmap> {
    let mut header = BITMAPINFO::default();
    header.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
    header.bmiHeader.biWidth = size as i32;
    // Negative height requests a top-down DIB, which matches our row order and
    // saves flipping every row back afterwards.
    header.bmiHeader.biHeight = -(size as i32);
    header.bmiHeader.biPlanes = 1;
    header.bmiHeader.biBitCount = 32;
    header.bmiHeader.biCompression = BI_RGB.0;

    // SAFETY: the DC and DIB are created, selected, read and destroyed in order
    // on this thread. `bits` points into the DIB section, which stays alive
    // until `DeleteObject`, and is only read while the DIB is still selected.
    unsafe {
        let dc = CreateCompatibleDC(None);
        if dc.is_invalid() {
            return Err(AppError::Win32("could not create a drawing context".into()));
        }

        let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
        let dib: HBITMAP = match CreateDIBSection(
            Some(dc),
            &header,
            DIB_RGB_COLORS,
            &mut bits,
            None,
            0,
        ) {
            Ok(dib) if !dib.is_invalid() && !bits.is_null() => dib,
            _ => {
                let _ = DeleteDC(dc);
                return Err(AppError::Win32("could not allocate a bitmap".into()));
            }
        };

        let previous: HGDIOBJ = SelectObject(dc, HGDIOBJ(dib.0));

        // The DIB starts zeroed, so anything the cursor does not cover stays
        // fully transparent — exactly what an alpha-blended cursor wants.
        let drawn = DrawIconEx(
            dc,
            0,
            0,
            HICON(cursor.0),
            size as i32,
            size as i32,
            0,
            None,
            DI_NORMAL,
        );

        let pixels = if drawn.is_ok() {
            let count = (size as usize) * (size as usize);
            let raw = std::slice::from_raw_parts(bits.cast::<u8>(), count * 4);
            let mut rgba = Vec::with_capacity(count * 4);
            for chunk in raw.chunks_exact(4) {
                // The DIB is BGRA; the rest of the pipeline is RGBA.
                rgba.extend_from_slice(&[chunk[2], chunk[1], chunk[0], chunk[3]]);
            }
            Some(rgba)
        } else {
            None
        };

        SelectObject(dc, previous);
        let _ = DeleteObject(HGDIOBJ(dib.0));
        let _ = DeleteDC(dc);

        let mut rgba = pixels.ok_or_else(|| AppError::Win32("the cursor could not be drawn".into()))?;

        // Some cursors are pure AND/XOR masks with no alpha channel at all, and
        // come back fully transparent. Windows still draws them correctly on
        // screen, so treat an all-zero alpha as "opaque where anything was
        // painted" rather than discarding the import.
        if rgba.iter().skip(3).step_by(4).all(|&a| a == 0) {
            for pixel in rgba.chunks_exact_mut(4) {
                if pixel[0] != 0 || pixel[1] != 0 || pixel[2] != 0 {
                    pixel[3] = 255;
                }
            }
        }

        Bitmap::from_rgba(size, size, rgba)
    }
}

/// The hotspot Windows reports for a cursor file, normalised to 0.0-1.0.
///
/// Imported cursors carry their author's intended click point; re-deriving it
/// from the artwork would move it, which is the one thing an import must not do.
pub fn hotspot(path: &Path, size: u32) -> AppResult<(f32, f32)> {
    let size = size.clamp(16, 256);
    let wide_path = wide(path);

    // SAFETY: as `read` above — NUL-terminated path, handle destroyed after use.
    unsafe {
        let handle = LoadImageW(
            None,
            PCWSTR(wide_path.as_ptr()),
            IMAGE_CURSOR,
            size as i32,
            size as i32,
            LR_LOADFROMFILE,
        )
        .map_err(|_| AppError::invalid("that cursor could not be opened"))?;

        let mut info = ICONINFO::default();
        let ok = GetIconInfo(HICON(handle.0), &mut info).is_ok();

        // GetIconInfo hands back two bitmaps that belong to the caller.
        if ok {
            if !info.hbmMask.is_invalid() {
                let _ = DeleteObject(HGDIOBJ(info.hbmMask.0));
            }
            if !info.hbmColor.is_invalid() {
                let _ = DeleteObject(HGDIOBJ(info.hbmColor.0));
            }
        }
        DestroyCursor(HCURSOR(handle.0)).ok();

        if !ok {
            return Err(AppError::invalid("that cursor has no readable hotspot"));
        }
        let max = (size.saturating_sub(1)).max(1) as f32;
        Ok((
            (info.xHotspot as f32 / max).clamp(0.0, 1.0),
            (info.yHotspot as f32 / max).clamp(0.0, 1.0),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build::cur_writer::{build_multi_resolution, write_cur};

    /// Each caller names its own file. Tests run in parallel, and two of these
    /// sharing one path would race — the flake would look like a reader bug
    /// rather than the test-isolation mistake it actually is.
    fn a_written_cursor(dir: &Path, name: &str, hotspot: (f32, f32)) -> std::path::PathBuf {
        let mut art = Bitmap::new(64, 64);
        for y in 10..54 {
            for x in 10..54 {
                art.set_pixel(x, y, [255, 128, 0, 255]);
            }
        }
        let images = build_multi_resolution(&art, hotspot, &[32, 64], false).unwrap();
        let bytes = write_cur(&images).unwrap();
        let path = dir.join(name);
        std::fs::write(&path, bytes).unwrap();
        path
    }

    /// Write a cursor with our own writer, read it back through Windows, and
    /// check the pixels survive. This closes the loop on both directions.
    #[test]
    fn a_cursor_we_wrote_reads_back_with_its_pixels() {
        let dir = std::env::temp_dir().join("cursorforge-tests");
        std::fs::create_dir_all(&dir).unwrap();
        let path = a_written_cursor(&dir, "reader-pixels.cur", (0.0, 0.0));

        let bitmap = read(&path, 64).unwrap();
        assert_eq!((bitmap.width, bitmap.height), (64, 64));
        assert!(!bitmap.is_empty(), "the cursor read back blank");

        let centre = bitmap.pixel(32, 32);
        assert!(centre[3] > 200, "centre should be opaque, got {centre:?}");
        assert!(centre[0] > 180 && centre[1] > 80, "colour was lost: {centre:?}");

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn the_authors_hotspot_is_preserved_rather_than_re_derived() {
        let dir = std::env::temp_dir().join("cursorforge-tests");
        std::fs::create_dir_all(&dir).unwrap();
        let path = a_written_cursor(&dir, "reader-hotspot.cur", (0.5, 0.25));

        let (hx, hy) = hotspot(&path, 64).unwrap();
        assert!((hx - 0.5).abs() < 0.03, "x drifted to {hx}");
        assert!((hy - 0.25).abs() < 0.03, "y drifted to {hy}");

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_file_that_is_not_a_cursor_is_refused() {
        let dir = std::env::temp_dir().join("cursorforge-tests");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("not-a-cursor.cur");
        std::fs::write(&path, b"plainly not a cursor at all").unwrap();

        assert!(read(&path, 32).is_err());
        assert!(hotspot(&path, 32).is_err());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_missing_file_is_an_error_not_a_panic() {
        assert!(read(Path::new(r"C:\nope\missing.cur"), 32).is_err());
    }
}
