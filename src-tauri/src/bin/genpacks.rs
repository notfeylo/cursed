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
use cursorforge_lib::packs::{catalog, styles};
use std::path::PathBuf;

fn main() {
    // Relative to the caller's working directory, which for `cargo run` is
    // wherever the command was invoked — the repo root, via `npm run`. Passing
    // `../assets/packs` here would silently write a sibling of the repo.
    let target = std::env::args()
        .nth(1)
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
