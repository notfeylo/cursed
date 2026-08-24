//! What Windows actually produces from a `.cur`, at a given size.
//!
//! Every diagnosis of "the pointer looks bad" so far has been reasoning about
//! the file. This asks the only authority that matters — `LoadImageW`, the same
//! call the app applies with — and reads the bitmap back out, so the pixels on
//! screen can be looked at instead of predicted.
//!
//!     cargo run --bin cursorprobe -- <file.cur> <size> [<size>...]
//!
//! Writes `<stem>-<size>.png` beside the output directory and prints the size
//! Windows chose, which is the number that says whether it blitted an entry or
//! stretched one.
use std::path::{Path, PathBuf};
use windows::core::PCWSTR;
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, DeleteDC, GetDIBits, GetObjectW, BITMAP, BITMAPINFO, BITMAPINFOHEADER,
    BI_RGB, DIB_RGB_COLORS, HBITMAP,
};
use windows::Win32::UI::WindowsAndMessaging::{
    DestroyCursor, GetIconInfo, LoadImageW, HCURSOR, HICON, ICONINFO, IMAGE_CURSOR, LR_LOADFROMFILE,
};

fn wide(p: &Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    p.as_os_str().encode_wide().chain(std::iter::once(0)).collect()
}

fn probe(path: &Path, size: u32, out_dir: &Path) {
    let w = wide(path);
    let handle = unsafe {
        LoadImageW(None, PCWSTR(w.as_ptr()), IMAGE_CURSOR, size as i32, size as i32, LR_LOADFROMFILE)
    };
    let handle = match handle {
        Ok(h) if !h.is_invalid() => h,
        _ => {
            println!("{size:>4}: LoadImageW refused the file");
            return;
        }
    };

    let mut info = ICONINFO::default();
    if unsafe { GetIconInfo(HICON(handle.0), &mut info) }.is_err() {
        println!("{size:>4}: GetIconInfo failed");
        unsafe { let _ = DestroyCursor(HCURSOR(handle.0)); }
        return;
    }

    let mut bm = BITMAP::default();
    let colour: HBITMAP = info.hbmColor;
    unsafe {
        GetObjectW(colour.into(), std::mem::size_of::<BITMAP>() as i32, Some(&mut bm as *mut _ as *mut _));
    }
    let (bw, bh) = (bm.bmWidth.max(0) as u32, bm.bmHeight.max(0) as u32);

    // Pull the pixels out as top-down BGRA.
    let mut pixels = vec![0u8; (bw * bh * 4) as usize];
    let mut header = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: bw as i32,
            biHeight: -(bh as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let dc = unsafe { CreateCompatibleDC(None) };
    let got = unsafe {
        GetDIBits(dc, colour, 0, bh, Some(pixels.as_mut_ptr() as *mut _), &mut header, DIB_RGB_COLORS)
    };
    unsafe { let _ = DeleteDC(dc); }

    let mut distinct = std::collections::HashSet::new();
    let mut opaque = 0u32;
    for px in pixels.chunks_exact(4) {
        if px[3] > 0 {
            opaque += 1;
            distinct.insert([px[0], px[1], px[2]]);
        }
    }

    println!(
        "{size:>4}: Windows returned {bw}x{bh}  rows={got}  opaque={opaque}  distinct colours={}",
        distinct.len()
    );

    if bw > 0 && bh > 0 {
        let mut rgba = Vec::with_capacity(pixels.len());
        for px in pixels.chunks_exact(4) {
            rgba.extend_from_slice(&[px[2], px[1], px[0], px[3]]);
        }
        let stem = path.file_stem().unwrap_or_default().to_string_lossy().to_string();
        let out = out_dir.join(format!("{stem}-req{size}-got{bw}.png"));
        if let Some(img) = image::RgbaImage::from_raw(bw, bh, rgba) {
            let _ = img.save(&out);
            println!("      wrote {}", out.display());
        }
    }

    unsafe {
        let _ = windows::Win32::Graphics::Gdi::DeleteObject(info.hbmColor.into());
        if !info.hbmMask.is_invalid() {
            let _ = windows::Win32::Graphics::Gdi::DeleteObject(info.hbmMask.into());
        }
        let _ = DestroyCursor(HCURSOR(handle.0));
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        eprintln!("usage: cursorprobe <file.cur> <size> [<size>...]");
        std::process::exit(2);
    }
    let path = PathBuf::from(&args[0]);
    let out_dir = std::env::temp_dir().join("cursorprobe");
    let _ = std::fs::create_dir_all(&out_dir);
    println!("probing {}", path.display());
    for a in &args[1..] {
        if let Ok(size) = a.parse::<u32>() {
            probe(&path, size, &out_dir);
        }
    }
}
