//! Records what the live pointer actually is, moment to moment.
//!
//!     cargo run --bin livewatch -- [seconds]
//!
//! Polls the cursor Windows is currently drawing and prints a line every time it
//! changes — its bitmap size and a hash of its pixels, with the time since the
//! watch started. Run it, then apply a cursor in the app.
//!
//! This exists because "it is blurry for a few seconds and then corrects itself"
//! cannot be diagnosed by reading code: every candidate mechanism reads
//! plausibly, and four of them were eliminated only by measurement. A transient
//! has to be caught while it is happening, and this is what catches it — the
//! sequence of states, and how long each one lasted.
//!
//! Saves a PNG per distinct state so the blurry one can be looked at beside the
//! sharp one rather than described.
use std::time::Instant;
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, DeleteDC, DeleteObject, GetDIBits, GetObjectW, BITMAP, BITMAPINFO,
    BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetCursorInfo, GetIconInfo, CURSORINFO, HICON, ICONINFO,
};

/// Size and pixel hash of whatever the pointer is right now.
fn sample() -> Option<(u32, u32, u64, Vec<u8>)> {
    let mut info = CURSORINFO {
        cbSize: std::mem::size_of::<CURSORINFO>() as u32,
        ..Default::default()
    };
    unsafe { GetCursorInfo(&mut info) }.ok()?;
    if info.hCursor.is_invalid() {
        return None;
    }

    let mut icon = ICONINFO::default();
    unsafe { GetIconInfo(HICON(info.hCursor.0), &mut icon) }.ok()?;

    let bitmap = if icon.hbmColor.is_invalid() { icon.hbmMask } else { icon.hbmColor };
    let mut bm = BITMAP::default();
    unsafe {
        GetObjectW(bitmap.into(), std::mem::size_of::<BITMAP>() as i32, Some(&mut bm as *mut _ as *mut _));
    }
    let (w, h) = (bm.bmWidth.max(0) as u32, bm.bmHeight.max(0) as u32);
    if w == 0 || h == 0 {
        return None;
    }

    let mut pixels = vec![0u8; (w * h * 4) as usize];
    let mut header = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w as i32,
            biHeight: -(h as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let dc = unsafe { CreateCompatibleDC(None) };
    unsafe {
        GetDIBits(dc, bitmap, 0, h, Some(pixels.as_mut_ptr() as *mut _), &mut header, DIB_RGB_COLORS);
        let _ = DeleteDC(dc);
        if !icon.hbmColor.is_invalid() {
            let _ = DeleteObject(icon.hbmColor.into());
        }
        if !icon.hbmMask.is_invalid() {
            let _ = DeleteObject(icon.hbmMask.into());
        }
    }

    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in &pixels {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    Some((w, h, hash, pixels))
}

fn main() {
    let seconds: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(20);
    let out = std::env::temp_dir().join("livewatch");
    let _ = std::fs::create_dir_all(&out);

    println!("watching the live pointer for {seconds}s — apply a cursor now");
    println!("  {:>8}  {:>9}  {:>16}", "at", "size", "pixels");

    let start = Instant::now();
    let mut last: Option<(u32, u32, u64)> = None;
    let mut changes = 0usize;

    while start.elapsed().as_secs() < seconds {
        if let Some((w, h, hash, pixels)) = sample() {
            if last != Some((w, h, hash)) {
                let at = start.elapsed().as_millis();
                println!("  {at:>6}ms  {:>4}x{:<4} {hash:016x}", w, h);
                let mut rgba = Vec::with_capacity(pixels.len());
                for px in pixels.chunks_exact(4) {
                    rgba.extend_from_slice(&[px[2], px[1], px[0], px[3]]);
                }
                if let Some(img) = image::RgbaImage::from_raw(w, h, rgba) {
                    let _ = img.save(out.join(format!("state-{changes:02}-{at}ms-{w}px.png")));
                }
                last = Some((w, h, hash));
                changes += 1;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    println!("\n{changes} distinct pointer states. PNGs in {}", out.display());
    if changes > 2 {
        println!("More than two means the pointer settled through intermediate states —");
        println!("compare the PNGs: the early ones are what you see during the bad window.");
    }
}
