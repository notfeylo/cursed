//! Catalog source exporter — `npm run generate:packs`.
//!
//! The runtime renders catalog artwork straight from the parametric definitions
//! in `packs::art`, so the app does not need these files to work. They exist so
//! the artwork is **reviewable**: a contributor can read `assets/packs/<id>/`,
//! see the seventeen SVG masters and the manifest, and open a pull request
//! against something concrete instead of against a wall of Rust.
//!
//! Because both paths run the same code, the exported SVG is byte-identical to
//! what ships — there is no second implementation to drift.

use cursorforge_lib::cursor::roles::ALL_ROLES;
use cursorforge_lib::packs::{brand, catalog, styles};
use std::path::PathBuf;

/// `genpacks --icon <out.png> [size]` renders the brand mark through the same
/// rasteriser the catalog uses, so the icon and the in-app logo cannot drift.
fn render_icon(args: &[String]) -> ! {
    let out = args
        .get(2)
        .cloned()
        .unwrap_or_else(|| "src-tauri/icons/source.png".to_owned());
    let size: u32 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(1024);

    match cursorforge_lib::build::svg::render(&brand::icon_svg(), size)
        .and_then(|b| b.to_png(image::codecs::png::CompressionType::Best))
    {
        Ok(png) => {
            if let Some(parent) = std::path::Path::new(&out).parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            match std::fs::write(&out, &png) {
                Ok(()) => {
                    println!("wrote {out} at {size}x{size} ({} KB)", png.len() / 1024);
                    std::process::exit(0);
                }
                Err(e) => {
                    eprintln!("could not write {out}: {e}");
                    std::process::exit(1);
                }
            }
        }
        Err(e) => {
            eprintln!("could not render the icon: {e}");
            std::process::exit(1);
        }
    }
}

/// `genpacks --import <folder>` runs the folder importer headlessly.
///
/// Exists so the import path can be exercised against a real folder of cursors
/// without driving the GUI, which is how it was proven before shipping.
fn run_import(args: &[String]) -> ! {
    let Some(folder) = args.get(2) else {
        eprintln!("usage: genpacks --import <folder>");
        std::process::exit(2);
    };
    match cursorforge_lib::import::import_folder(std::path::Path::new(folder)) {
        Ok(report) => {
            println!("imported {}, skipped {}", report.imported, report.skipped);
            for name in &report.names {
                println!("  + {name}");
            }
            for problem in &report.problems {
                println!("  ! {problem}");
            }
            std::process::exit(0);
        }
        Err(e) => {
            eprintln!("import failed: {e}");
            std::process::exit(1);
        }
    }
}

/// `genpacks --check-update` runs the real update check through the app's own
/// WinHTTP code.
///
/// Added because the checksum chain was verified with curl while the code that
/// actually performs the request was never run — and it turned out to be broken.
/// Verifying the thing next to the thing is not verifying the thing.
fn check_update() -> ! {
    match cursorforge_lib::updates::check() {
        Ok(status) => {
            println!("current:   {}", status.current);
            println!("latest:    {}", status.latest.as_deref().unwrap_or("(none)"));
            println!("newer:     {}", status.newer_available);
            println!(
                "installer: {}",
                status.installer.as_deref().unwrap_or("(none offered)")
            );
            if let Some(size) = status.size {
                println!("size:      {:.2} MB", size as f64 / 1_048_576.0);
            }
            std::process::exit(0);
        }
        Err(e) => {
            eprintln!("update check failed: {e}");
            std::process::exit(1);
        }
    }
}

/// `genpacks --list-packs` prints exactly what the catalog screen will show.
///
/// Added because "the catalog still shows cursors that are not mine" is not
/// answerable by reading a flag — it needs the same function the UI calls.
fn list_packs() -> ! {
    match cursorforge_lib::packs::catalog::list_summaries() {
        Ok(packs) => {
            println!("{} entries in the catalog", packs.len());
            for pack in &packs {
                println!(
                    "  {:<44} {:<16} {}",
                    pack.name,
                    pack.category,
                    if pack.id.starts_with("user:") {
                        "imported"
                    } else {
                        "GENERATED"
                    }
                );
            }
            let generated = packs.iter().filter(|p| !p.id.starts_with("user:")).count();
            println!("\ngenerated: {generated}   imported: {}", packs.len() - generated);
            std::process::exit(0);
        }
        Err(e) => {
            eprintln!("could not list the catalog: {e}");
            std::process::exit(1);
        }
    }
}

/// The sizes that decide a mark. 16 is the one that kills directions.
const SHEET_SIZES: [u32; 5] = [256, 128, 48, 32, 16];

/// Contact sheets for the candidate marks, on dark and on light.
///
/// Every size is rasterised **at its own pixel size** and then blitted, rather
/// than drawing one big sheet and scaling it down. Those are different
/// operations: scaling a 256 px render to 16 px shows what the shape looks like,
/// not what the rasteriser will actually do with it in a tray icon, which is the
/// only question that matters here.
fn logo_sheets(args: &[String]) {
    use cursorforge_lib::build::bitmap::Bitmap;
    use cursorforge_lib::packs::logo;

    let out = PathBuf::from(args.get(2).cloned().unwrap_or_else(|| "logo-sheets".into()));
    if let Err(e) = std::fs::create_dir_all(&out) {
        eprintln!("could not create {}: {e}", out.display());
        std::process::exit(1);
    }

    // Panels are the app's own background and a plain light one, because a
    // light Windows taskbar is the case everyone forgets to check.
    let panels: [([u8; 4], [u8; 4], &str); 2] = [
        ([0x05, 0x05, 0x07, 0xff], [0xed, 0xf1, 0xf7, 0xff], "dark"),
        ([0xf4, 0xf5, 0xf7, 0xff], [0x0b, 0x0d, 0x12, 0xff], "light"),
    ];

    const PAD: u32 = 24;
    const GAP: u32 = 24;
    let width = PAD * 2 + 256 + GAP + 128;
    let panel_h = PAD * 2 + 256;
    let height = panel_h * 2;

    for direction in logo::DIRECTIONS {
        let mut sheet = Bitmap::new(width, height);

        for (index, (background, ink, _label)) in panels.iter().enumerate() {
            let top = index as u32 * panel_h;
            for y in top..top + panel_h {
                for x in 0..width {
                    sheet.set_pixel(x, y, *background);
                }
            }

            let colour = format!("#{:02x}{:02x}{:02x}", ink[0], ink[1], ink[2]);

            // 256 on the left; the smaller sizes stack down the right column,
            // baseline-aligned so they can be compared against each other.
            let mut cursor_y = top + PAD;
            for size in SHEET_SIZES {
                // Each size gets the form it would actually ship at, so the
                // sheet shows the icon set rather than one drawing scaled.
                let markup = logo::svg_for_size(direction, &colour, size);
                let Ok(mark) = cursorforge_lib::build::svg::render(&markup, size) else {
                    eprintln!("{direction}: could not render at {size}px");
                    continue;
                };
                let (ox, oy) = if size == 256 {
                    (PAD, top + PAD)
                } else {
                    let x = PAD + 256 + GAP;
                    let y = cursor_y;
                    cursor_y += size + 16;
                    (x, y)
                };
                blit(&mut sheet, &mark, ox, oy);
            }
        }

        let path = out.join(format!("{direction}.png"));
        match sheet.to_png(image::codecs::png::CompressionType::Default) {
            Ok(bytes) => match std::fs::write(&path, &bytes) {
                Ok(()) => println!("{}  ({} bytes)", path.display(), bytes.len()),
                Err(e) => eprintln!("could not write {}: {e}", path.display()),
            },
            Err(e) => eprintln!("{direction}: {e}"),
        }
    }
    std::process::exit(0);
}

/// `genpacks --trace <png>` turns a supplied logo image into an SVG path.
///
/// Transcribing a shape by eye from a picture produces a shape that is nearly
/// right, which is the worst outcome for a logo: every angle slightly off, and
/// no way to tell which. This reads the pixels instead. It masks the artwork,
/// separates the connected shapes, walks the boundary of the one asked for,
/// simplifies the staircase a raster edge is made of, and prints a path
/// normalised into a 64-unit box.
fn trace_logo(args: &[String]) {
    let Some(path) = args.get(2) else {
        eprintln!("usage: genpacks --trace <png> [shape-index]");
        std::process::exit(2);
    };
    let want: usize = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(0);

    let img = match image::open(path) {
        Ok(i) => i.to_rgba8(),
        Err(e) => {
            eprintln!("could not open {path}: {e}");
            std::process::exit(1);
        }
    };
    let (w, h) = (img.width() as i32, img.height() as i32);

    // Ink is anything both opaque and dark. The file is a cut-out, so the
    // background may be transparent or white and both have to be excluded.
    let ink: Vec<bool> = img
        .pixels()
        .map(|p| {
            let [r, g, b, a] = p.0;
            let luma = (r as u32 * 299 + g as u32 * 587 + b as u32 * 114) / 1000;
            a > 128 && luma < 160
        })
        .collect();
    let at = |x: i32, y: i32| -> bool { x >= 0 && y >= 0 && x < w && y < h && ink[(y * w + x) as usize] };

    // Flood fill, so the mark and each letter are separate shapes.
    let mut label = vec![0usize; (w * h) as usize];
    let mut shapes: Vec<(usize, i32, i32, i32, i32, usize)> = Vec::new();
    let mut next = 0usize;
    for sy in 0..h {
        for sx in 0..w {
            if !at(sx, sy) || label[(sy * w + sx) as usize] != 0 {
                continue;
            }
            next += 1;
            let (mut count, mut x0, mut y0, mut x1, mut y1) = (0usize, sx, sy, sx, sy);
            let mut stack = vec![(sx, sy)];
            label[(sy * w + sx) as usize] = next;
            while let Some((x, y)) = stack.pop() {
                count += 1;
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x);
                y1 = y1.max(y);
                for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                    let (nx, ny) = (x + dx, y + dy);
                    if at(nx, ny) && label[(ny * w + nx) as usize] == 0 {
                        label[(ny * w + nx) as usize] = next;
                        stack.push((nx, ny));
                    }
                }
            }
            if count > 200 {
                shapes.push((next, x0, y0, x1, y1, count));
            }
        }
    }
    shapes.sort_by_key(|s| s.2);

    println!("{} shapes, top to bottom:", shapes.len());
    for (i, (_, x0, y0, x1, y1, n)) in shapes.iter().enumerate() {
        println!("  [{i}] x {x0}..{x1}  y {y0}..{y1}  {}x{}  {n} px", x1 - x0 + 1, y1 - y0 + 1);
    }
    let Some(&(id, x0, y0, x1, y1, _)) = shapes.get(want) else {
        eprintln!("no shape at index {want}");
        std::process::exit(1);
    };

    let member = |x: i32, y: i32| -> bool {
        x >= 0 && y >= 0 && x < w && y < h && label[(y * w + x) as usize] == id
    };
    let mut start = (x0, y0);
    'find: for y in y0..=y1 {
        for x in x0..=x1 {
            if member(x, y) {
                start = (x, y);
                break 'find;
            }
        }
    }

    // Moore-neighbourhood boundary walk.
    const DIRS: [(i32, i32); 8] =
        [(1, 0), (1, 1), (0, 1), (-1, 1), (-1, 0), (-1, -1), (0, -1), (1, -1)];
    let mut outline: Vec<(f64, f64)> = Vec::new();
    let (mut cx, mut cy) = start;
    let mut dir = 0usize;
    for step in 0..200_000usize {
        outline.push((cx as f64, cy as f64));
        let mut moved = false;
        for k in 0..8 {
            let d = (dir + 6 + k) % 8;
            let (nx, ny) = (cx + DIRS[d].0, cy + DIRS[d].1);
            if member(nx, ny) {
                cx = nx;
                cy = ny;
                dir = d;
                moved = true;
                break;
            }
        }
        if !moved || ((cx, cy) == start && step > 8) {
            break;
        }
    }

    let simplified = rdp(&outline, 1.5);
    let (bw, bh) = ((x1 - x0 + 1) as f64, (y1 - y0 + 1) as f64);
    let scale = 60.0 / bw.max(bh);
    let ox = (64.0 - bw * scale) / 2.0 - x0 as f64 * scale;
    let oy = (64.0 - bh * scale) / 2.0 - y0 as f64 * scale;

    let mut d = String::new();
    for (i, (px, py)) in simplified.iter().enumerate() {
        d.push_str(&format!(
            "{}{:.2} {:.2} ",
            if i == 0 { "M" } else { "L" },
            px * scale + ox,
            py * scale + oy
        ));
    }
    d.push('Z');

    println!("\nboundary {} points -> {} after simplification", outline.len(), simplified.len());
    println!("\n{d}\n");
    std::process::exit(0);
}

/// Ramer-Douglas-Peucker: keeps the corners, drops the staircase.
fn rdp(points: &[(f64, f64)], epsilon: f64) -> Vec<(f64, f64)> {
    if points.len() < 3 {
        return points.to_vec();
    }
    let (first, last) = (points[0], points[points.len() - 1]);
    let (mut worst, mut index) = (0.0f64, 0usize);
    for (i, p) in points.iter().enumerate().take(points.len() - 1).skip(1) {
        let d = perpendicular(*p, first, last);
        if d > worst {
            worst = d;
            index = i;
        }
    }
    if worst > epsilon {
        let mut left = rdp(&points[..=index], epsilon);
        let right = rdp(&points[index..], epsilon);
        left.pop();
        left.extend(right);
        left
    } else {
        vec![first, last]
    }
}

fn perpendicular(p: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len = (dx * dx + dy * dy).sqrt();
    if len < f64::EPSILON {
        return ((p.0 - a.0).powi(2) + (p.1 - a.1).powi(2)).sqrt();
    }
    ((p.0 - a.0) * dy - (p.1 - a.1) * dx).abs() / len
}

/// The four acceptance checks for the mark, rendered so they can be judged
/// rather than asserted.
///
/// Each one is a failure that survives a casual eyeball review: a tray icon that
/// vanishes on a light taskbar, a mark indistinguishable from the system arrow
/// it sits next to, a size ramp that reads as two different logos, and artwork
/// that only holds together in the brand colour.
fn logo_accept(args: &[String]) {
    use cursorforge_lib::build::bitmap::Bitmap;
    use cursorforge_lib::packs::brand;

    let out = PathBuf::from(args.get(2).cloned().unwrap_or_else(|| "logo-sheets".into()));
    let _ = std::fs::create_dir_all(&out);

    let fill = |b: &mut Bitmap, x0: u32, y0: u32, w: u32, h: u32, c: [u8; 4]| {
        for y in y0..(y0 + h).min(b.height) {
            for x in x0..(x0 + w).min(b.width) {
                b.set_pixel(x, y, c);
            }
        }
    };
    /// Nearest-neighbour magnification, so the pixel grid stays visible.
    fn zoom(sheet: &mut Bitmap, src: &Bitmap, ox: u32, oy: u32, z: u32) {
        for y in 0..src.height * z {
            for x in 0..src.width * z {
                let s = src.pixel(x / z, y / z);
                let a = s[3] as u32;
                if a == 0 {
                    continue;
                }
                let (tx, ty) = (ox + x, oy + y);
                if tx >= sheet.width || ty >= sheet.height {
                    continue;
                }
                let d = sheet.pixel(tx, ty);
                let m = |s: u8, d: u8| ((s as u32 * a + d as u32 * (255 - a)) / 255) as u8;
                sheet.set_pixel(tx, ty, [m(s[0], d[0]), m(s[1], d[1]), m(s[2], d[2]), 255]);
            }
        }
    }

    const Z: u32 = 10;
    const LIGHT: [u8; 4] = [0xf3, 0xf3, 0xf3, 0xff];
    const TASKBAR: [u8; 4] = [0x20, 0x20, 0x20, 0xff];

    // 1 — the tray form on a light taskbar and a dark one, actual size and
    //     magnified. The accent blue is what actually ships in the tray.
    {
        let mut s = Bitmap::new(760, 400);
        fill(&mut s, 0, 0, 760, 200, LIGHT);
        fill(&mut s, 0, 200, 760, 200, TASKBAR);
        for (i, bg) in [LIGHT, TASKBAR].iter().enumerate() {
            let top = i as u32 * 200;
            let _ = bg;
            for (j, colour) in ["#2e8bff", "#000000", "#ffffff"].iter().enumerate() {
                if let Ok(m) = cursorforge_lib::build::svg::render(&brand::small_mark_svg(colour), 16)
                {
                    let x = 40 + j as u32 * 240;
                    zoom(&mut s, &m, x, top + 20, Z);
                    // Actual size beside it, which is the only honest view.
                    zoom(&mut s, &m, x + 180, top + 52, 1);
                }
            }
        }
        write_sheet(&out, "accept-1-light-taskbar.png", &s);
    }

    // 2 — beside the system arrow. This is the constraint that killed Horns.
    {
        let mut s = Bitmap::new(560, 400);
        fill(&mut s, 0, 0, 560, 200, LIGHT);
        fill(&mut s, 0, 200, 560, 200, TASKBAR);
        let stock = std::path::Path::new(r"C:\Windows\Cursors\aero_arrow.cur");
        for (i, ink) in ["#2e8bff", "#2e8bff"].iter().enumerate() {
            let top = i as u32 * 200;
            if let Ok(m) = cursorforge_lib::build::svg::render(&brand::small_mark_svg(ink), 16) {
                zoom(&mut s, &m, 40, top + 20, Z);
            }
            if let Ok(w) = cursorforge_lib::build::cur_reader::read(stock, 16) {
                zoom(&mut s, &w, 300, top + 20, Z);
            }
        }
        write_sheet(&out, "accept-2-vs-windows-arrow.png", &s);
    }

    // 3 — the size ramp, actual size, to judge whether the two forms read as
    //     one logo. Anything but actual size would beg the question.
    {
        let mut s = Bitmap::new(420, 180);
        fill(&mut s, 0, 0, 420, 180, [0x0b, 0x0d, 0x12, 0xff]);
        let mut x = 24;
        for size in [16u32, 24, 32, 128] {
            let markup = if size < 32 {
                brand::small_mark_svg("#5cb8ff")
            } else {
                brand::mark_svg("#2e8bff", "#5cb8ff", false)
            };
            if let Ok(m) = cursorforge_lib::build::svg::render(&markup, size) {
                zoom(&mut s, &m, x, 24 + (128 - size), 1);
            }
            x += size + 32;
        }
        write_sheet(&out, "accept-3-size-ramp.png", &s);
    }

    // 4 — flat white on black. If it only works in the accent, it is not a logo.
    {
        let mut s = Bitmap::new(460, 200);
        fill(&mut s, 0, 0, 460, 200, [0x00, 0x00, 0x00, 0xff]);
        if let Ok(m) = cursorforge_lib::build::svg::render(&brand::mark_svg("#ffffff", "#ffffff", false), 128) {
            zoom(&mut s, &m, 24, 36, 1);
        }
        if let Ok(m) = cursorforge_lib::build::svg::render(&brand::mark_svg("#ffffff", "#ffffff", false), 32) {
            zoom(&mut s, &m, 190, 36, 1);
        }
        if let Ok(m) = cursorforge_lib::build::svg::render(&brand::small_mark_svg("#ffffff"), 16) {
            zoom(&mut s, &m, 250, 40, 6);
        }
        write_sheet(&out, "accept-4-silhouette.png", &s);
    }

    std::process::exit(0);
}

fn write_sheet(dir: &std::path::Path, name: &str, sheet: &cursorforge_lib::build::bitmap::Bitmap) {
    match sheet.to_png(image::codecs::png::CompressionType::Default) {
        Ok(bytes) => {
            let path = dir.join(name);
            match std::fs::write(&path, &bytes) {
                Ok(()) => println!("{}", path.display()),
                Err(e) => eprintln!("could not write {}: {e}", path.display()),
            }
        }
        Err(e) => eprintln!("{name}: {e}"),
    }
}

/// Writes the multi-resolution `.ico` and the PNG icon set.
///
/// Built here rather than by scaling one master, because the mark has two forms
/// and the whole point of the small one is that it is *not* the big one
/// resampled. A generator that takes a single PNG cannot express that.
///
/// Entries are PNG-compressed, which Windows has understood since Vista and
/// which keeps a 256 px entry from costing 256 KB of raw BGRA.
fn icon_set(args: &[String]) {
    use cursorforge_lib::packs::brand;

    let out = PathBuf::from(args.get(2).cloned().unwrap_or_else(|| "src-tauri/icons".into()));
    let _ = std::fs::create_dir_all(&out);

    // Below 32 the tile's rounded corners and the ring both disappear, so the
    // small form is drawn as the whole icon rather than sitting on a tile.
    const SIZES: [u32; 7] = [256, 128, 64, 48, 32, 24, 16];
    let mut entries: Vec<(u32, Vec<u8>)> = Vec::new();

    for size in SIZES {
        let markup = if size < 32 {
            brand::small_mark_svg("#2e8bff")
        } else {
            brand::icon_svg()
        };
        match cursorforge_lib::build::svg::render(&markup, size)
            .and_then(|b| b.to_png(image::codecs::png::CompressionType::Best))
        {
            Ok(png) => {
                let _ = std::fs::write(out.join(format!("{size}x{size}.png")), &png);
                entries.push((size, png));
            }
            Err(e) => eprintln!("icon at {size}px failed: {e}"),
        }
    }

    // The rest of the names Tauri's bundler and the Store manifest expect. Left
    // stale, these keep the previous mark alive in the installer and on a store
    // listing long after the logo has changed — which is exactly how an app ends
    // up with two logos.
    let extras: [(&str, u32); 12] = [
        ("icon.png", 512),
        ("128x128@2x.png", 256),
        ("StoreLogo.png", 50),
        ("Square30x30Logo.png", 30),
        ("Square44x44Logo.png", 44),
        ("Square71x71Logo.png", 71),
        ("Square89x89Logo.png", 89),
        ("Square107x107Logo.png", 107),
        ("Square142x142Logo.png", 142),
        ("Square150x150Logo.png", 150),
        ("Square284x284Logo.png", 284),
        ("Square310x310Logo.png", 310),
    ];
    for (name, size) in extras {
        let markup = if size < 32 {
            brand::small_mark_svg("#2e8bff")
        } else {
            brand::icon_svg()
        };
        match cursorforge_lib::build::svg::render(&markup, size)
            .and_then(|b| b.to_png(image::codecs::png::CompressionType::Best))
        {
            Ok(png) => {
                let _ = std::fs::write(out.join(name), &png);
            }
            Err(e) => eprintln!("{name} failed: {e}"),
        }
    }
    println!("wrote {} PNG sizes + {} named assets", SIZES.len(), extras.len());

    match write_ico(&entries) {
        Ok(bytes) => {
            let path = out.join("icon.ico");
            match std::fs::write(&path, &bytes) {
                Ok(()) => println!(
                    "{} — {} entries, {} KB",
                    path.display(),
                    entries.len(),
                    bytes.len() / 1024
                ),
                Err(e) => eprintln!("could not write {}: {e}", path.display()),
            }
        }
        Err(e) => eprintln!("could not build the .ico: {e}"),
    }
    std::process::exit(0);
}

/// An `ICONDIR` followed by one `ICONDIRENTRY` per size, then the PNG payloads.
///
/// Same container as a `.cur` with `idType` 1 instead of 2, and no hotspot —
/// where a cursor stores its hotspot, an icon stores colour planes and bit
/// depth.
fn write_ico(entries: &[(u32, Vec<u8>)]) -> Result<Vec<u8>, String> {
    if entries.is_empty() {
        return Err("nothing to write".into());
    }
    let mut out = Vec::new();
    out.extend_from_slice(&0u16.to_le_bytes()); // reserved
    out.extend_from_slice(&1u16.to_le_bytes()); // 1 = icon
    out.extend_from_slice(&(entries.len() as u16).to_le_bytes());

    // Payloads start after the directory.
    let mut offset = 6 + entries.len() * 16;
    for (size, png) in entries {
        // 256 is stored as 0: the field is one byte and 256 does not fit.
        let dim = if *size >= 256 { 0u8 } else { *size as u8 };
        out.push(dim);
        out.push(dim);
        out.push(0); // palette size — 0 for truecolour
        out.push(0); // reserved
        out.extend_from_slice(&1u16.to_le_bytes()); // colour planes
        out.extend_from_slice(&32u16.to_le_bytes()); // bits per pixel
        out.extend_from_slice(&(png.len() as u32).to_le_bytes());
        out.extend_from_slice(&(offset as u32).to_le_bytes());
        offset += png.len();
    }
    for (_, png) in entries {
        out.extend_from_slice(png);
    }
    Ok(out)
}

/// Renders the small sizes and magnifies them with nearest-neighbour, so the
/// actual pixel grid is visible.
///
/// A smooth resample would hide exactly what is being judged. The question at
/// 16 px is whether a feature survives rasterisation at all, and that is only
/// answerable by looking at the pixels the rasteriser produced.
fn logo_zoom(args: &[String]) {
    use cursorforge_lib::build::bitmap::Bitmap;
    use cursorforge_lib::packs::logo;

    let out = PathBuf::from(args.get(2).cloned().unwrap_or_else(|| "logo-sheets".into()));
    let _ = std::fs::create_dir_all(&out);

    const SIZES: [u32; 4] = [16, 24, 32, 48];
    const ZOOM: u32 = 9;
    const PAD: u32 = 12;

    for direction in logo::DIRECTIONS {
        let cell = 48 * ZOOM;
        let width = PAD + SIZES.len() as u32 * (cell + PAD);
        let height = PAD * 2 + cell;
        let mut sheet = Bitmap::new(width, height);
        for y in 0..height {
            for x in 0..width {
                sheet.set_pixel(x, y, [0x18, 0x1c, 0x24, 0xff]);
            }
        }

        for (i, size) in SIZES.iter().enumerate() {
            let markup = logo::svg_for_size(direction, "#edf1f7", *size);
            let Ok(mark) = cursorforge_lib::build::svg::render(&markup, *size) else {
                continue;
            };
            let ox = PAD + i as u32 * (cell + PAD);
            let oy = PAD;
            for y in 0..mark.height * ZOOM {
                for x in 0..mark.width * ZOOM {
                    let src = mark.pixel(x / ZOOM, y / ZOOM);
                    let a = src[3] as u32;
                    let (tx, ty) = (ox + x, oy + y);
                    if tx >= width || ty >= height {
                        continue;
                    }
                    let dst = sheet.pixel(tx, ty);
                    let mix =
                        |s: u8, d: u8| -> u8 { ((s as u32 * a + d as u32 * (255 - a)) / 255) as u8 };
                    sheet.set_pixel(
                        tx,
                        ty,
                        [mix(src[0], dst[0]), mix(src[1], dst[1]), mix(src[2], dst[2]), 255],
                    );
                }
            }
        }

        let path = out.join(format!("{direction}-zoom.png"));
        if let Ok(bytes) = sheet.to_png(image::codecs::png::CompressionType::Default) {
            let _ = std::fs::write(&path, &bytes);
            println!("{}", path.display());
        }
    }
    std::process::exit(0);
}

/// Straight source-over composite of `over` onto `sheet` at (ox, oy).
fn blit(sheet: &mut cursorforge_lib::build::bitmap::Bitmap, over: &cursorforge_lib::build::bitmap::Bitmap, ox: u32, oy: u32) {
    for y in 0..over.height {
        for x in 0..over.width {
            let src = over.pixel(x, y);
            let a = src[3] as u32;
            if a == 0 {
                continue;
            }
            let (tx, ty) = (ox + x, oy + y);
            if tx >= sheet.width || ty >= sheet.height {
                continue;
            }
            let dst = sheet.pixel(tx, ty);
            let mix = |s: u8, d: u8| -> u8 { ((s as u32 * a + d as u32 * (255 - a)) / 255) as u8 };
            sheet.set_pixel(tx, ty, [mix(src[0], dst[0]), mix(src[1], dst[1]), mix(src[2], dst[2]), 255]);
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--icon") {
        render_icon(&args);
    }
    if args.get(1).map(String::as_str) == Some("--list-packs") {
        list_packs();
    }
    if args.get(1).map(String::as_str) == Some("--check-update") {
        check_update();
    }
    if args.get(1).map(String::as_str) == Some("--import") {
        run_import(&args);
    }
    if args.get(1).map(String::as_str) == Some("--logo-sheet") {
        logo_sheets(&args);
    }
    if args.get(1).map(String::as_str) == Some("--logo-zoom") {
        logo_zoom(&args);
    }
    if args.get(1).map(String::as_str) == Some("--icon-set") {
        icon_set(&args);
    }
    if args.get(1).map(String::as_str) == Some("--logo-accept") {
        logo_accept(&args);
    }
    if args.get(1).map(String::as_str) == Some("--trace") {
        trace_logo(&args);
    }
    if args.get(1).map(String::as_str) == Some("--fetch-update") {
        // Downloads whatever the release feed is offering and checks it against
        // the checksum published with that release, then stops. Exercises the
        // exact path the in-app updater runs, minus launching the installer --
        // which is the half that had never been proven end to end.
        // An explicit target can be given, because the interesting case is
        // "what would an older install do" and this binary is never older than
        // the release it is testing against.
        let explicit = match (args.get(2), args.get(3)) {
            (Some(tag), Some(asset)) => Some((tag.clone(), asset.clone())),
            _ => None,
        };
        match explicit.map(Ok).unwrap_or_else(|| {
            cursorforge_lib::updates::check().map(|s| {
                (
                    s.latest.clone().unwrap_or_default(),
                    s.installer.clone().unwrap_or_default(),
                )
            })
        }) {
            Ok((tag, asset)) => {
                if tag.is_empty() || asset.is_empty() {
                    println!("nothing offered; pass a tag and asset explicitly to force one");
                    std::process::exit(0);
                }
                println!("downloading {asset} from v{tag}");
                match cursorforge_lib::updates::download(&tag, &asset) {
                    Ok(file) => {
                        println!("downloaded  {}", file.display());
                        match cursorforge_lib::updates::verify_only(&tag, &asset) {
                            Ok(hash) => {
                                println!("checksum    {hash}");
                                println!("VERIFIED against the published checksum");
                            }
                            Err(e) => {
                                eprintln!("VERIFY FAILED: {e}");
                                std::process::exit(1);
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("DOWNLOAD FAILED: {e}");
                        std::process::exit(1);
                    }
                }
                std::process::exit(0);
            }
            Err(e) => {
                eprintln!("check failed: {e}");
                std::process::exit(1);
            }
        }
    }
    if args.get(1).map(String::as_str) == Some("--diagnostics") {
        // The same report the Diagnostics panel produces, reachable without a
        // window — which is what you want when the window is the problem.
        match cursorforge_lib::commands::get_diagnostics() {
            Ok(report) => {
                println!("{report}");
                std::process::exit(0);
            }
            Err(e) => {
                eprintln!("diagnostics failed: {e}");
                std::process::exit(1);
            }
        }
    }

    // Relative to the caller's working directory, which for `cargo run` is
    // wherever the command was invoked — the repo root, via `npm run`. Passing
    // `../assets/packs` here would silently write a sibling of the repo.
    let target = args
        .get(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("assets/packs"));

    if let Err(e) = std::fs::create_dir_all(&target) {
        eprintln!("could not create {}: {e}", target.display());
        std::process::exit(1);
    }

    // A wrong path is easy to make and hard to notice, so say where the files
    // actually went rather than echoing back what was asked for. Windows'
    // canonical form carries a `\\?\` prefix that helps nobody reading output.
    let resolved = target
        .canonicalize()
        .map(|path| {
            let text = path.to_string_lossy().into_owned();
            PathBuf::from(text.strip_prefix(r"\\?\").unwrap_or(&text).to_owned())
        })
        .unwrap_or_else(|_| target.clone());

    let packs = styles::all();
    let mut failures = 0usize;

    for pack in &packs {
        // Every pack must define all seventeen roles. A pack that does not is a
        // build failure, not a runtime surprise (PRD §19 rule 7).
        for role in ALL_ROLES {
            let markup = cursorforge_lib::packs::art::render_role(&pack.style, role, 0.0);
            if !markup.starts_with("<svg") {
                eprintln!("{}: {role} produced no artwork", pack.id);
                failures += 1;
            }
        }

        if let Err(e) = catalog::export_sources(pack, &target) {
            eprintln!("{}: {e}", pack.id);
            failures += 1;
        }
    }

    if failures > 0 {
        eprintln!("\n{failures} problem(s) found — catalog not written cleanly");
        std::process::exit(1);
    }

    println!(
        "exported {} packs x {} roles to {}",
        packs.len(),
        ALL_ROLES.len(),
        resolved.display()
    );
}
