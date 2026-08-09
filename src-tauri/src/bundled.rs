//! Cursor packs that ship inside the installer.
//!
//! The 205 generated packs are code, so every install has them. These are
//! something else: complete, hand-made, animated sets that arrive as real
//! `.ani` files and cannot be expressed parametrically. They are embedded in the
//! binary and installed on first run, so a machine that has never seen the app
//! still gets them without a download.
//!
//! **Only packs whose licence permits redistribution appear here.** Both of the
//! current ones are GPL-3.0 with the author stating the assets are their own
//! work, and each archive carries its own `LICENSE.txt` and `COPYRIGHT.txt`
//! which are extracted alongside the cursors. Cursed itself stays MIT; these sit
//! beside it as separately-licensed data, which is what the GPL calls mere
//! aggregation. `docs/LICENSES.md` names them.
//!
//! A pack whose licence is unstated is not a pack that can be shipped, however
//! good it is. That is the whole test applied here — not whether the artwork is
//! wanted, but whether the person who made it said it could be passed on.

use crate::error::AppResult;
use crate::paths;
use std::path::Path;

/// One embedded archive, and the name the importer will give it.
struct Bundled {
    /// Matches the pack name the importer derives, so an already-installed pack
    /// is recognised rather than reinstalled every launch.
    slug: &'static str,
    label: &'static str,
    archive: &'static [u8],
}

const PACKS: [Bundled; 2] = [
    Bundled {
        slug: "geared-brass",
        label: "Geared Brass",
        archive: include_bytes!("../../assets/bundled/geared-brass.zip"),
    },
    Bundled {
        slug: "geared-steel",
        label: "Geared Steel",
        archive: include_bytes!("../../assets/bundled/geared-steel.zip"),
    },
];

/// Installs any bundled pack that is not already present.
///
/// Runs on every launch, and is a no-op once the packs exist — the check is a
/// directory test, not an unpack. Re-running matters: a user who clears their
/// imports should get the shipped packs back, because those are part of the
/// product rather than something they added.
pub fn install_missing() {
    let Ok(root) = paths::root() else {
        log::warn!("no storage directory, so bundled packs were skipped");
        return;
    };
    let imported = root.join("imported");

    for pack in &PACKS {
        if imported.join(pack.slug).join("pack.json").is_file() {
            continue;
        }
        match install(pack, &root) {
            Ok(()) => log::info!("installed the bundled pack {}", pack.label),
            Err(e) => log::warn!("could not install {}: {e}", pack.label),
        }
    }
}

fn install(pack: &Bundled, root: &Path) -> AppResult<()> {
    // Unpacked into a scratch directory and then handed to the ordinary folder
    // importer, so a bundled pack goes through exactly the same role mapping,
    // `.inf` parsing and verification as one the user imports themselves. A
    // second code path here would be a second set of bugs.
    let scratch = root.join("bundled-scratch").join(pack.slug);
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch)?;

    let cursor = std::io::Cursor::new(pack.archive);
    let mut archive = zip::ZipArchive::new(cursor)
        .map_err(|e| crate::error::AppError::msg(format!("bundled archive unreadable: {e}")))?;

    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|e| crate::error::AppError::msg(format!("bundled entry unreadable: {e}")))?;
        if entry.is_dir() {
            continue;
        }
        // Only the final component, flattened — the same rule the folder
        // importer applies to any zip.
        let Some(name) = Path::new(entry.name())
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
        else {
            continue;
        };
        if paths::validate_relative(&name).is_err() {
            continue;
        }
        let mut bytes = Vec::new();
        std::io::Read::read_to_end(&mut entry, &mut bytes)?;
        std::fs::write(scratch.join(&name), &bytes)?;
    }

    // The importer names a pack from its directory, so the scratch directory is
    // named for the pack rather than for the archive.
    let staging = root.join("bundled-scratch").join(pack.label);
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::rename(&scratch, &staging)?;

    let parent = root.join("bundled-scratch");
    crate::import::import_folder(&parent)?;
    let _ = std::fs::remove_dir_all(&parent);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_bundled_archive_is_present_and_looks_like_a_zip() {
        for pack in &PACKS {
            assert!(
                pack.archive.len() > 1024,
                "{} is {} bytes, which is not an archive",
                pack.label,
                pack.archive.len()
            );
            assert_eq!(&pack.archive[..2], b"PK", "{} is not a zip", pack.label);
        }
    }

    /// Shipping someone else's artwork without their licence text is the thing
    /// that makes it infringement rather than redistribution.
    #[test]
    fn each_bundled_pack_carries_its_licence() {
        for pack in &PACKS {
            let cursor = std::io::Cursor::new(pack.archive);
            let mut archive = zip::ZipArchive::new(cursor).expect("readable");
            let names: Vec<String> = (0..archive.len())
                .filter_map(|i| archive.by_index(i).ok().map(|e| e.name().to_owned()))
                .collect();
            assert!(
                names.iter().any(|n| n.to_ascii_uppercase().contains("LICENSE")),
                "{} ships without its LICENSE",
                pack.label
            );
            assert!(
                names.iter().any(|n| n.to_ascii_uppercase().contains("COPYRIGHT")),
                "{} ships without its COPYRIGHT",
                pack.label
            );
        }
    }
}
