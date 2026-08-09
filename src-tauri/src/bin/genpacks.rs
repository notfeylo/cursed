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
