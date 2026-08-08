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
