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

/// `genpacks --icon <out.png> [size] [--dev]` renders the brand mark through the
/// same rasteriser the catalog uses, so the icon and the in-app logo cannot
/// drift.
///
/// `--dev` renders it in the development channel's amber. The two channels
/// install side by side, so their icons have to be tellable apart at tray size —
/// see `packs::brand::IconPalette`.
fn render_icon(args: &[String]) -> ! {
    let dev = args.iter().any(|a| a == "--dev");
    let out = args
        .get(2)
        .filter(|a| !a.starts_with("--"))
        .cloned()
        .unwrap_or_else(|| "src-tauri/icons/source.png".to_owned());
    let size: u32 = args
        .get(3)
        .filter(|a| !a.starts_with("--"))
        .and_then(|s| s.parse().ok())
        .unwrap_or(1024);

    let svg = if dev {
        brand::icon_svg_in(&brand::ICON_AMBER)
    } else {
        brand::icon_svg()
    };

    match cursorforge_lib::build::svg::render(&svg, size)
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

/// `genpacks --stress-handles [iterations]` hammers the cursor loader and
/// reports whether the process's GDI and USER handle counts moved.
///
/// The leak this looks for cannot be found by reading the code — every path
/// looks correct — and cannot be found by a short run, because one leaked handle
/// per apply takes hours to matter and then takes the process down all at once.
/// It is found by doing it thousands of times and watching a number.
fn stress_handles(args: &[String]) -> ! {
    let iterations: u32 = args
        .get(2)
        .and_then(|s| s.parse().ok())
        .unwrap_or(2_000)
        .clamp(1, 1_000_000);

    let report = cursorforge_lib::stress::run_handle_stress(iterations);
    if report.files.is_empty() {
        eprintln!("no stock cursors found to test against; nothing was measured");
        std::process::exit(2);
    }

    println!("files:      {}", report.files.len());
    for file in &report.files {
        println!("  {}", file.display());
    }
    println!("iterations: {} x {} loads", report.iterations, report.files.len());
    println!(
        "GDI:        {} -> {}  ({:+})",
        report.gdi_before,
        report.gdi_after,
        report.gdi_growth()
    );
    println!(
        "USER:       {} -> {}  ({:+})",
        report.user_before,
        report.user_after,
        report.user_growth()
    );
    if report.failures > 0 {
        println!("failures:   {}", report.failures);
    }

    if report.is_clean() {
        println!("\nclean — the handle counts did not track the iteration count.");
        std::process::exit(0);
    }
    println!("\nLEAK — the counts grew with the work done. Audit every");
    println!("LoadImageW / CopyIcon / DestroyCursor path in cursor::engine.");
    std::process::exit(1);
}

/// `genpacks --soak [minutes] [csv-path]` runs the work loop for hours and
/// watches every counter that matters.
///
/// The handle harness above answers one question thoroughly. This answers a
/// different one badly, and is worth running anyway: Cursed lives in the tray
/// for weeks, so a leak of one anything per operation is invisible in every test
/// in the suite and fatal on day eleven. The only way to see that is to do the
/// work for a long time and watch the numbers.
///
/// Default is 24 hours, sampled every minute. Rows are written to the CSV as
/// they are taken rather than at the end, so a run that is interrupted — a
/// reboot, a closed laptop, a killed terminal — still leaves everything it
/// measured up to that point. A soak whose results only exist in memory is a
/// soak that produces nothing the first time anything goes wrong, which on a
/// twenty-four hour run is most of the time.
fn soak(args: &[String]) -> ! {
    use std::io::Write as _;

    let minutes: u64 = args
        .get(2)
        .and_then(|s| s.parse().ok())
        .unwrap_or(24 * 60)
        .clamp(1, 7 * 24 * 60);
    let csv_path = args
        .get(3)
        .cloned()
        .unwrap_or_else(|| "soak.csv".to_owned());

    // A sample a minute for a day is 1,440 rows: enough to see a slope, few
    // enough to open in anything.
    let interval = std::time::Duration::from_secs(60);

    println!("soaking for {minutes} minute(s), sampling every 60s -> {csv_path}");
    println!("{}", cursorforge_lib::stress::Sample::header());

    let file = std::fs::File::create(&csv_path);
    let mut sink = match file {
        Ok(f) => Some(std::io::BufWriter::new(f)),
        Err(e) => {
            eprintln!("could not open {csv_path}: {e} — the run will print but not record");
            None
        }
    };
    if let Some(out) = sink.as_mut() {
        let _ = writeln!(out, "{}", cursorforge_lib::stress::Sample::header());
        let _ = out.flush();
    }

    let report = cursorforge_lib::stress::run_soak(
        std::time::Duration::from_secs(minutes * 60),
        interval,
        |sample| {
            let line = sample.as_csv();
            println!("{line}");
            if let Some(out) = sink.as_mut() {
                let _ = writeln!(out, "{line}");
                // Flushed per row. The whole point is surviving an interruption.
                let _ = out.flush();
            }
        },
    );

    println!("\ncycles:            {}", report.cycles);
    println!("cursor loads:      {}", report.work.cursor_loads);
    println!("images decoded:    {}", report.work.image_decodes);
    println!("cursors built:     {}", report.work.cursors_built);
    println!("state round trips: {}", report.work.state_round_trips);
    println!("failures:          {}", report.work.failures);

    match report.growth() {
        Some((gdi, user, threads, handles, working_set, private)) => {
            println!("\ngrowth from first sample to last:");
            println!("  GDI objects   {gdi:+}");
            println!("  USER objects  {user:+}");
            println!("  threads       {threads:+}");
            println!("  handles       {handles:+}");
            println!("  working set   {:+.1} MB", working_set as f64 / 1_048_576.0);
            println!("  private bytes {:+.1} MB", private as f64 / 1_048_576.0);
            println!("\nRead the slope, not the endpoints: memory moves for a dozen reasons");
            println!("that are not leaks, and a number that tracks the cycle count is the");
            println!("only shape that means anything.");
            std::process::exit(0);
        }
        None => {
            eprintln!("the run produced no samples");
            std::process::exit(1);
        }
    }
}

/// `genpacks --matte-sheet [out.png]` renders the background-removal test set
/// before and after, on one sheet.
///
/// The acceptance criteria for a cut-out are visual. There is no assertion that
/// distinguishes "clean edge" from "chewed edge" — a pixel count passes both —
/// so the deliverable is a picture, and this makes it.
///
/// The seven cases are the ones that break different things:
///
///  1. **logo on white** — the ordinary case, and the one a naive global colour
///     match also passes, which is why it cannot be the only case.
///  2. **checkerboard screenshot** — an editor's transparency grid,
///     photographed. Two background colours instead of one; defeats border
///     sampling, spread measurement and tolerance selection all at once.
///  3. **JPEG fringing** — ringing around a hard edge. The reason tolerance is
///     derived from border noise rather than fixed.
///  4. **anti-aliased dark art on black** — subject and background share a
///     value range. The case where too much slack eats the artwork.
///  5. **already clean alpha** — must be left completely alone. Re-cutting art
///     somebody already cut is how a soft edge is lost.
///  6. **grey subject on a flat grey card** — a subject a few levels from its
///     own background. The case a loose tolerance destroys.
///  7. **a photograph** — no background to remove. Must fail *gracefully*:
///     leave the image alone rather than punch a hole in it.
///
/// Each is generated rather than shipped, so the sheet can be reproduced on any
/// machine without carrying test artwork in the repository.
fn matte_sheet(args: &[String]) {
    use cursorforge_lib::build::bitmap::Bitmap;
    use cursorforge_lib::build::matte;

    let out = PathBuf::from(
        args.get(2)
            .cloned()
            .unwrap_or_else(|| "docs/verification/matte-sheet.png".into()),
    );
    if let Some(parent) = out.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    const S: u32 = 128;
    let cases = matte_cases(S);

    // Two rows: before on top, after underneath, one column per case.
    const PAD: u32 = 8;
    let cols = cases.len() as u32;
    let sheet_w = cols * (S + PAD) + PAD;
    let sheet_h = 2 * (S + PAD) + PAD;
    let mut sheet = Bitmap::new(sheet_w, sheet_h);

    // A mid grey ground, so both a white cut-out and a black one are visible
    // against it. On white, case 1's result is invisible; on black, case 4's is.
    for y in 0..sheet_h {
        for x in 0..sheet_w {
            sheet.set_pixel(x, y, [24, 24, 28, 255]);
        }
    }

    println!("{:<28} {:>9}  NOTE", "CASE", "REMOVED");
    for (index, (name, expectation, original)) in cases.iter().enumerate() {
        let mut cut = original.clone();
        let report = matte::remove_background(&mut cut);

        let x0 = PAD + index as u32 * (S + PAD);
        blit_over_checker(&mut sheet, original, x0, PAD);
        blit_over_checker(&mut sheet, &cut, x0, PAD + S + PAD);

        let removed = report.removed * 100.0;
        let verdict = match expectation {
            Expect::Removes if report.removed > 0.20 => "ok".to_owned(),
            Expect::Removes => "TOO LITTLE REMOVED".to_owned(),
            Expect::LeavesAlone if report.removed < 0.02 => "ok, left alone".to_owned(),
            Expect::LeavesAlone => "TOOK SOMETHING IT SHOULD NOT HAVE".to_owned(),
            Expect::KeepsSubject => {
                // The centre of the frame is where a photograph's subject is.
                // Whether the sky went is a matter of taste; whether the subject
                // went is not.
                let survived = subject_survived(original, &cut);
                if survived {
                    "ok, the subject survived".to_owned()
                } else {
                    "SUBJECT WAS DESTROYED".to_owned()
                }
            }
        };
        println!("{name:<28} {removed:>8.1}%  {verdict}");
    }

    match write_png(&sheet, &out) {
        Ok(()) => {
            println!("\nsheet -> {}", out.display());
            println!("Top row: as imported. Bottom row: after removal, over a checkerboard.");
            println!("Judge the edges. A percentage cannot tell a clean cut from a chewed one.");
        }
        Err(e) => eprintln!("could not write {}: {e}", out.display()),
    }
}

enum Expect {
    /// There is a background and it should go.
    Removes,
    /// There is nothing to remove; the image must come out untouched.
    LeavesAlone,
    /// There may or may not be something to remove, and the only thing that
    /// matters is that the subject survives. "Fail gracefully" means the user
    /// gets their picture back, not that nothing happened.
    KeepsSubject,
}

/// Whether the middle of the image is still opaque after a cut.
///
/// A blunt measure, and the right one for "did this fail gracefully": the
/// subject of a photograph is in the middle of the frame, and a cut that
/// hollowed it out has done the unrecoverable thing. What happened to the sky is
/// a judgement call; what happened to the subject is not.
fn subject_survived(
    before: &cursorforge_lib::build::bitmap::Bitmap,
    after: &cursorforge_lib::build::bitmap::Bitmap,
) -> bool {
    let (w, h) = (before.width, before.height);
    let (x0, x1) = (w * 4 / 10, w * 6 / 10);
    let (y0, y1) = (h * 4 / 10, h * 7 / 10);

    let mut was = 0usize;
    let mut still = 0usize;
    for y in y0..y1 {
        for x in x0..x1 {
            if before.alpha(x, y) > 200 {
                was += 1;
                if after.alpha(x, y) > 200 {
                    still += 1;
                }
            }
        }
    }
    was == 0 || (still as f32 / was as f32) > 0.9
}

/// The seven test images, generated.
fn matte_cases(size: u32) -> Vec<(String, Expect, cursorforge_lib::build::bitmap::Bitmap)> {
    use cursorforge_lib::build::bitmap::Bitmap;

    // A filled circle, which is the shape most likely to show a bad edge: every
    // pixel of its boundary is a different subpixel coverage.
    let disc = |b: &mut Bitmap, colour: [u8; 4], soft: bool| {
        let r = (size as f32) * 0.34;
        let (cx, cy) = (size as f32 / 2.0, size as f32 / 2.0);
        for y in 0..size {
            for x in 0..size {
                let d = (((x as f32 + 0.5) - cx).powi(2) + ((y as f32 + 0.5) - cy).powi(2)).sqrt();
                let coverage = if soft {
                    (r + 0.5 - d).clamp(0.0, 1.0)
                } else if d <= r {
                    1.0
                } else {
                    0.0
                };
                if coverage <= 0.0 {
                    continue;
                }
                let under = b.pixel(x, y);
                let blend = |a: u8, c: u8| ((c as f32 * coverage) + (a as f32 * (1.0 - coverage))) as u8;
                b.set_pixel(
                    x,
                    y,
                    [
                        blend(under[0], colour[0]),
                        blend(under[1], colour[1]),
                        blend(under[2], colour[2]),
                        // Anywhere the disc has any coverage becomes opaque:
                        // these are the *inputs*, and an input with a soft
                        // alpha edge would be testing the resampler rather than
                        // the matte. Case 5 gets its transparency from the
                        // pixels the disc never reached.
                        255,
                    ],
                );
            }
        }
    };

    let flat = |rgb: [u8; 3]| {
        let mut b = Bitmap::new(size, size);
        for y in 0..size {
            for x in 0..size {
                b.set_pixel(x, y, [rgb[0], rgb[1], rgb[2], 255]);
            }
        }
        b
    };

    let mut cases = Vec::new();

    // 1. A logo on white.
    let mut one = flat([255, 255, 255]);
    disc(&mut one, [220, 40, 60, 255], true);
    cases.push(("1 logo on white".to_owned(), Expect::Removes, one));

    // 2. A screenshot of an editor's transparency grid.
    let mut two = Bitmap::new(size, size);
    for y in 0..size {
        for x in 0..size {
            let light = ((x / 8) + (y / 8)) % 2 == 0;
            let v = if light { 255 } else { 204 };
            two.set_pixel(x, y, [v, v, v, 255]);
        }
    }
    disc(&mut two, [40, 120, 220, 255], true);
    cases.push(("2 checkerboard screenshot".to_owned(), Expect::Removes, two));

    // 3. JPEG-style ringing around the edge, on an off-white card.
    let mut three = flat([250, 249, 247]);
    disc(&mut three, [30, 30, 30, 255], true);
    for y in 1..size - 1 {
        for x in 1..size - 1 {
            // A cheap ringing model: push each pixel away from its neighbours'
            // mean, which is what a lossy edge looks like.
            let here = three.pixel(x, y);
            let left = three.pixel(x - 1, y);
            let right = three.pixel(x + 1, y);
            let ring = |c: usize| {
                let mean = (left[c] as i32 + right[c] as i32) / 2;
                (here[c] as i32 + (here[c] as i32 - mean) / 3).clamp(0, 255) as u8
            };
            three.set_pixel(x, y, [ring(0), ring(1), ring(2), 255]);
        }
    }
    cases.push(("3 jpeg fringing".to_owned(), Expect::Removes, three));

    // 4. Dark, anti-aliased art on black.
    let mut four = flat([8, 8, 10]);
    disc(&mut four, [56, 56, 64, 255], true);
    cases.push(("4 dark art on black".to_owned(), Expect::Removes, four));

    // 5. Already clean: a transparent surround.
    let mut five = Bitmap::new(size, size);
    disc(&mut five, [90, 200, 140, 255], true);
    cases.push(("5 already clean alpha".to_owned(), Expect::LeavesAlone, five));

    // 6. A grey subject on a flat grey card, a few levels apart.
    let mut six = flat([180, 180, 180]);
    disc(&mut six, [150, 150, 150, 255], false);
    cases.push(("6 grey on grey".to_owned(), Expect::Removes, six));

    // 7. A photograph: a scene, not a swatch.
    //
    // A gradient alone is not this test. A smooth ramp with no structure *is*
    // background by every definition in `matte`, and removing it is arguably
    // correct — so a case made of one only proves the flood fill follows
    // gradients, which is a thing it is supposed to do.
    //
    // A photograph has a subject. Here: a graded sky, a textured foreground with
    // high-frequency detail, and a solid object standing on it. Failing
    // gracefully means the object and the texture are still there afterwards,
    // whatever happens to the sky.
    let mut seven = Bitmap::new(size, size);
    for y in 0..size {
        for x in 0..size {
            let horizon = size * 6 / 10;
            let pixel = if y < horizon {
                // Sky: a smooth vertical grade with a little sensor noise.
                let t = y as f32 / horizon as f32;
                let noise = ((x * 7919 + y * 104_729) % 9) as u8;
                [
                    (90.0 + 90.0 * t) as u8,
                    (130.0 + 80.0 * t) as u8,
                    (200.0 + 40.0 * t) as u8,
                ]
                .map(|c: u8| c.saturating_add(noise))
            } else {
                // Ground: high-frequency texture, which is what the smoothness
                // gate exists to stop the flood walking into.
                let grain = ((x * 31 + y * 17) % 61) as u8;
                let speck = if (x * 13 + y * 29) % 7 == 0 { 40 } else { 0 };
                [
                    70u8.saturating_add(grain).saturating_add(speck),
                    58u8.saturating_add(grain / 2),
                    44u8.saturating_add(grain / 3),
                ]
            };
            seven.set_pixel(x, y, [pixel[0], pixel[1], pixel[2], 255]);
        }
    }
    // The subject: a solid shape standing on the ground, well inside the frame.
    for y in (size * 3 / 10)..(size * 8 / 10) {
        for x in (size * 4 / 10)..(size * 6 / 10) {
            seven.set_pixel(x, y, [190, 40, 40, 255]);
        }
    }
    cases.push(("7 photograph".to_owned(), Expect::KeepsSubject, seven));

    cases
}

/// Draws a bitmap onto the sheet over a small checkerboard, so transparency is
/// visible rather than reading as the sheet's own background.
fn blit_over_checker(
    sheet: &mut cursorforge_lib::build::bitmap::Bitmap,
    source: &cursorforge_lib::build::bitmap::Bitmap,
    x0: u32,
    y0: u32,
) {
    for y in 0..source.height {
        for x in 0..source.width {
            let (tx, ty) = (x0 + x, y0 + y);
            if tx >= sheet.width || ty >= sheet.height {
                continue;
            }
            let under = if ((x / 8) + (y / 8)) % 2 == 0 {
                [64u8, 64, 68]
            } else {
                [48u8, 48, 52]
            };
            let pixel = source.pixel(x, y);
            let a = pixel[3] as f32 / 255.0;
            let mix = |c: usize| ((pixel[c] as f32 * a) + (under[c] as f32 * (1.0 - a))) as u8;
            sheet.set_pixel(tx, ty, [mix(0), mix(1), mix(2), 255]);
        }
    }
}

fn write_png(
    bitmap: &cursorforge_lib::build::bitmap::Bitmap,
    path: &std::path::Path,
) -> Result<(), String> {
    let buffer =
        image::RgbaImage::from_raw(bitmap.width, bitmap.height, bitmap.pixels.clone())
            .ok_or_else(|| "the sheet could not be wrapped as an image".to_owned())?;
    image::DynamicImage::ImageRgba8(buffer)
        .save(path)
        .map_err(|e| e.to_string())
}

/// `genpacks --check-roles` reads all seventeen pointer roles and checks each.
///
/// This is the answer to "the cursor does not change in Firefox". A role that
/// fails to follow is almost never the application refusing to cooperate — it is
/// a malformed or missing entry among these seventeen, and Windows falls back to
/// its own cursor for any of them silently, with nothing written anywhere. Which
/// looks exactly like a browser being difficult.
///
/// So it is checked before anybody starts screenshotting browsers.
fn check_roles() -> ! {
    let audit = cursorforge_lib::cursor::audit_roles();

    println!("{:<12} {:<8} {:<24} VALUE", "ROLE", "FORMAT", "STATUS");
    let mut faults = 0;
    for role in &audit {
        let status = if !role.set {
            "unset (Windows default)"
        } else if !role.exists {
            "FILE DOES NOT EXIST"
        } else if !role.ok {
            "BAD HEADER"
        } else {
            "ok"
        };
        if !role.ok {
            faults += 1;
        }
        println!(
            "{:<12} {:<8} {:<24} {}",
            role.role, role.format, status, role.value
        );
    }

    let set = audit.iter().filter(|r| r.set).count();
    println!("\n{set} of 17 roles set, {faults} fault(s)");
    if faults == 0 {
        println!("Every role that is set resolves to a real cursor file.");
        println!("A role that still does not follow in an application is that");
        println!("application's own decision, not a broken entry.");
        std::process::exit(0);
    }
    println!("Fix these before blaming an application: Windows silently draws its");
    println!("own cursor for any role it cannot load, with no error anywhere.");
    std::process::exit(1);
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

/// `genpacks --roles <pack-id> <out.png>` renders one pack's roles side by side.
///
/// Artwork that is only ever seen 32 px wide, in motion, under the user's hand
/// is very easy to ship broken. This puts the roles on one sheet so they can be
/// looked at.
fn role_sheet(args: &[String]) {
    use cursorforge_lib::build::bitmap::Bitmap;
    use cursorforge_lib::cursor::roles::Role;
    use cursorforge_lib::packs::{art, styles};

    let id = args.get(2).cloned().unwrap_or_else(|| "minimal-arrow".into());
    let out = args.get(3).cloned().unwrap_or_else(|| "logo-sheets/roles.png".into());
    let Some(pack) = styles::find(&id) else {
        eprintln!("no pack called {id}");
        std::process::exit(1);
    };

    const SHOW: [Role; 6] = [Role::Arrow, Role::Hand, Role::IBeam, Role::Help, Role::No, Role::Wait];
    const CELL: u32 = 96;
    const PAD: u32 = 16;
    let width = PAD + SHOW.len() as u32 * (CELL + PAD);
    let height = PAD * 2 + CELL + 40;

    let mut sheet = Bitmap::new(width, height);
    for y in 0..height {
        for x in 0..width {
            sheet.set_pixel(x, y, [0x0b, 0x0d, 0x12, 0xff]);
        }
    }

    for (i, role) in SHOW.iter().enumerate() {
        let markup = art::render_role(&pack.style, *role, 0.0);
        let Ok(m) = cursorforge_lib::build::svg::render(&markup, CELL) else { continue };
        let ox = PAD + i as u32 * (CELL + PAD);
        for y in 0..m.height {
            for x in 0..m.width {
                let s = m.pixel(x, y);
                let a = s[3] as u32;
                if a == 0 { continue; }
                let (tx, ty) = (ox + x, PAD + y);
                if tx >= width || ty >= height { continue; }
                let d = sheet.pixel(tx, ty);
                let mix = |s: u8, d: u8| ((s as u32 * a + d as u32 * (255 - a)) / 255) as u8;
                sheet.set_pixel(tx, ty, [mix(s[0], d[0]), mix(s[1], d[1]), mix(s[2], d[2]), 255]);
            }
        }
    }

    if let Ok(png) = sheet.to_png(image::codecs::png::CompressionType::Default) {
        let _ = std::fs::write(&out, &png);
        println!("{out}  ({})", SHOW.iter().map(|r| r.to_string()).collect::<Vec<_>>().join(", "));
    }
    std::process::exit(0);
}

/// `genpacks --flatten <in> <out.png> <rrggbb>` composites an image onto a
/// solid card, destroying its alpha.
///
/// Exists to manufacture the exact input people complain about: artwork that
/// *had* transparency, saved by something that did not keep it. Cutting that
/// back out is the whole job, and it cannot be tested without one.
fn flatten(args: &[String]) {
    use cursorforge_lib::build::bitmap::Bitmap;

    let (Some(src), Some(dst)) = (args.get(2), args.get(3)) else {
        eprintln!("usage: genpacks --flatten <in> <out.png> [rrggbb]");
        std::process::exit(2);
    };
    let hex = args.get(4).cloned().unwrap_or_else(|| "ffffff".into());
    let card = cursorforge_lib::util::parse_hex_color(&hex).unwrap_or([255, 255, 255]);

    let decoded = match image::open(src) {
        Ok(i) => i.to_rgba8(),
        Err(e) => {
            eprintln!("could not open {src}: {e}");
            std::process::exit(1);
        }
    };
    let (w, h) = (decoded.width(), decoded.height());
    let source = match Bitmap::from_rgba(w, h, decoded.into_raw()) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };

    let mut out = Bitmap::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let [r, g, b, a] = source.pixel(x, y);
            let a = a as u32;
            let mix = |s: u8, d: u8| ((s as u32 * a + d as u32 * (255 - a)) / 255) as u8;
            out.set_pixel(x, y, [mix(r, card[0]), mix(g, card[1]), mix(b, card[2]), 255]);
        }
    }

    match out.to_png(image::codecs::png::CompressionType::Best) {
        Ok(png) => {
            let _ = std::fs::write(dst, &png);
            println!("wrote {dst} ({w}x{h}) on #{hex}");
        }
        Err(e) => eprintln!("{e}"),
    }
    std::process::exit(0);
}

/// `genpacks --cutout <in> <out.png>` runs the real background remover.
///
/// The matte is covered by synthetic tests, but a synthetic test cannot tell you
/// whether a photograph comes out clean. This writes the actual result so it can
/// be looked at, and reports what survived: any pixel that is still opaque and
/// still the background colour is a failure you can count.
fn cutout(args: &[String]) {
    use cursorforge_lib::build::{bitmap::Bitmap, matte};

    let (Some(src), Some(dst)) = (args.get(2), args.get(3)) else {
        eprintln!("usage: genpacks --cutout <in> <out.png>");
        std::process::exit(2);
    };
    let decoded = match image::open(src) {
        Ok(i) => i.to_rgba8(),
        Err(e) => {
            eprintln!("could not open {src}: {e}");
            std::process::exit(1);
        }
    };
    let (w, h) = (decoded.width(), decoded.height());
    let mut bitmap = match Bitmap::from_rgba(w, h, decoded.into_raw()) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };

    let corner = bitmap.pixel(0, 0);
    let report = matte::remove_background(&mut bitmap);

    // Count what is still opaque and still looks like the original corner.
    let mut residue = 0usize;
    let mut kept = 0usize;
    for y in 0..h {
        for x in 0..w {
            let p = bitmap.pixel(x, y);
            if p[3] > 128 {
                kept += 1;
                let d = |a: u8, b: u8| (a as i32 - b as i32).abs();
                if d(p[0], corner[0]).max(d(p[1], corner[1])).max(d(p[2], corner[2])) <= 40 {
                    residue += 1;
                }
            }
        }
    }

    println!("removed        {:.1}%", report.removed * 100.0);
    println!("already alpha  {}", report.already_had_alpha);
    println!("kept opaque    {kept} px");
    println!("residue        {residue} px still the background colour and opaque");

    match bitmap.to_png(image::codecs::png::CompressionType::Best) {
        Ok(png) => match std::fs::write(dst, &png) {
            Ok(()) => println!("wrote {dst}"),
            Err(e) => eprintln!("could not write {dst}: {e}"),
        },
        Err(e) => eprintln!("{e}"),
    }
    std::process::exit(0);
}

/// `genpacks --shrink <in> <out> <max-dim>` prepares a bundled raster asset.
///
/// A photographic backdrop arrives from a screenshot tool at full resolution
/// with a full colour palette, and every byte of that ends up inside the
/// installer. A blurred greyscale image carries almost no high-frequency detail,
/// so it survives being halved and reduced to a single channel with nothing
/// visible lost — and the app scales it to fill the window anyway.
fn shrink_image(args: &[String]) {
    let (Some(src), Some(dst)) = (args.get(2), args.get(3)) else {
        eprintln!("usage: genpacks --shrink <in> <out> [max-dim]");
        std::process::exit(2);
    };
    let max: u32 = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(512);

    let img = match image::open(src) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("could not open {src}: {e}");
            std::process::exit(1);
        }
    };
    let before = std::fs::metadata(src).map(|m| m.len()).unwrap_or(0);

    let scaled = img.resize(max, max, image::imageops::FilterType::Lanczos3);
    let grey = image::DynamicImage::ImageLuma8(scaled.to_luma8());

    if let Err(e) = grey.save(dst) {
        eprintln!("could not write {dst}: {e}");
        std::process::exit(1);
    }
    let after = std::fs::metadata(dst).map(|m| m.len()).unwrap_or(0);
    println!(
        "{} -> {}  {}x{}  {:.1} KB -> {:.1} KB",
        src,
        dst,
        grey.width(),
        grey.height(),
        before as f64 / 1024.0,
        after as f64 / 1024.0
    );
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

        // Also dump the tiled app icon at taskbar sizes, since that is the form
    // the .ico ships and the one Windows actually draws down there.
    {
        let mut s2 = Bitmap::new(760, 200);
        fill(&mut s2, 0, 0, 760, 200, [0x20, 0x20, 0x20, 0xff]);
        let mut x = 20;
        for size in [16u32, 24, 32, 48] {
            if let Ok(m) = cursorforge_lib::build::svg::render(&brand::icon_svg(), size) {
                zoom(&mut s2, &m, x, 20, 8);
            }
            x += size * 8 + 24;
        }
        write_sheet(&out, "accept-5-tile-small.png", &s2);
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

    // The tiled icon at every size, including 16 and 24.
    //
    // Those two used to be the bare flat mark on transparency while everything
    // 32 and up was the mark on its tile. Windows picks whichever size fits the
    // surface it is drawing, so the taskbar got the bare wedge while the Start
    // menu and Explorer got the tile — one app with two icons, which is exactly
    // what it looked like. The tile survives 16 px (verified by rendering it and
    // reading the pixels), so there is no reason to have a second treatment.
    for size in SIZES {
        let markup = brand::icon_svg();
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
        let markup = brand::icon_svg();
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

/// Renders the exact ladder a cursor ships, magnified above and 1:1 below.
///
/// The point is to look at what Windows will actually draw. A master that looks
/// perfect at 500 px says nothing about 24 px, and "the cursor is blurry" is a
/// complaint about sizes that no preview shows large enough to judge.
/// Nearest-neighbour for the magnification, deliberately — a smooth upscale
/// would hide the very pixels being inspected.
fn ladder(args: &[String]) {
    use cursorforge_lib::build::bitmap::Bitmap;
    use cursorforge_lib::build::{cur_writer, pipeline};

    let (Some(src), Some(dst)) = (args.get(2), args.get(3)) else {
        eprintln!("usage: genpacks --ladder <in> <out.png>");
        std::process::exit(2);
    };

    let decoded = match image::open(src) {
        Ok(i) => i.to_rgba8(),
        Err(e) => {
            eprintln!("could not open {src}: {e}");
            std::process::exit(1);
        }
    };
    let (w, h) = (decoded.width(), decoded.height());
    let bitmap = match Bitmap::from_rgba(w, h, decoded.into_raw()) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };
    let master = match pipeline::prepare_master_with(&bitmap, pipeline::Cut::Auto) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };
    let images =
        match cur_writer::build_multi_resolution(&master, (0.5, 0.5), &cur_writer::TARGET_SIZES, false) {
            Ok(i) => i,
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1);
            }
        };

    const ZOOM: u32 = 4;
    const PAD: u32 = 10;
    let width: u32 = PAD + cur_writer::TARGET_SIZES.iter().map(|s| s * ZOOM + PAD).sum::<u32>();
    let height = PAD + 128 * ZOOM + PAD + 128 + PAD;

    let mut sheet = Bitmap::new(width, height);
    for y in 0..height {
        for x in 0..width {
            // Mid grey: light and dark artwork are both legible on it, and
            // neither is flattered.
            sheet.set_pixel(x, y, [44, 46, 52, 255]);
        }
    }

    let mut x = PAD;
    for cursor in &images {
        let side = cursor.bitmap.width;
        let mut big = Bitmap::new(side * ZOOM, side * ZOOM);
        for yy in 0..side * ZOOM {
            for xx in 0..side * ZOOM {
                big.set_pixel(xx, yy, cursor.bitmap.pixel(xx / ZOOM, yy / ZOOM));
            }
        }
        blit(&mut sheet, &big, x, PAD);
        // 1:1 beneath, which is the size that actually matters.
        blit(&mut sheet, &cursor.bitmap, x, PAD + 128 * ZOOM + PAD);
        x += side * ZOOM + PAD;
    }

    match sheet.to_png(image::codecs::png::CompressionType::Default) {
        Ok(bytes) => match std::fs::write(dst, &bytes) {
            Ok(()) => println!("wrote {dst} ({} sizes, master {}px)", images.len(), master.width),
            Err(e) => eprintln!("could not write {dst}: {e}"),
        },
        Err(e) => eprintln!("{e}"),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--ladder") {
        ladder(&args);
        return;
    }
    if args.get(1).map(String::as_str) == Some("--icon") {
        render_icon(&args);
    }
    if args.get(1).map(String::as_str) == Some("--list-packs") {
        list_packs();
    }
    if args.get(1).map(String::as_str) == Some("--check-update") {
        check_update();
    }
    if args.get(1).map(String::as_str) == Some("--stress-handles") {
        stress_handles(&args);
    }
    if args.get(1).map(String::as_str) == Some("--soak") {
        soak(&args);
    }
    if args.get(1).map(String::as_str) == Some("--check-roles") {
        check_roles();
    }
    if args.get(1).map(String::as_str) == Some("--matte-sheet") {
        matte_sheet(&args);
        return;
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
    if args.get(1).map(String::as_str) == Some("--shrink") {
        shrink_image(&args);
    }
    if args.get(1).map(String::as_str) == Some("--roles") {
        role_sheet(&args);
    }
    if args.get(1).map(String::as_str) == Some("--cutout") {
        cutout(&args);
    }
    if args.get(1).map(String::as_str) == Some("--flatten") {
        flatten(&args);
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

    // An unrecognised flag is an error, not an output directory.
    //
    // Everything above matches an exact string and falls through otherwise, so
    // a mistyped or not-yet-built subcommand reached the pack exporter below and
    // was treated as a path — which created a directory literally called
    // `--check-roles` and filled it with 17 cursors. Silently doing something
    // unrelated to what was asked is the worst answer available.
    if let Some(flag) = args.get(1).filter(|a| a.starts_with("--")) {
        eprintln!("genpacks: unknown option {flag}");
        eprintln!("If this is a new subcommand, the binary may predate it — rebuild.");
        std::process::exit(2);
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
