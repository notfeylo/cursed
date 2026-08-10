//! Cursor packs that ship inside the installer.
//!
//! These are hand-made sets that arrive as real `.cur` and `.ani` files and
//! cannot be expressed parametrically. They are embedded in the binary and
//! installed on first run, so a machine that has never seen the app still gets
//! them without a download. Since the generated catalog was cut back to the
//! single pack that fills unmapped roles, **these are the catalog.**
//!
//! ## Licensing, stated plainly
//!
//! **Two of these carry a licence. Thirty-four do not.**
//!
//! `geared-brass` and `geared-steel` are GPL-3.0, with the author stating the
//! assets are their own work, and their archives carry the `LICENSE.txt` and
//! `COPYRIGHT.txt` that say so — extracted alongside the cursors. Cursed itself
//! stays MIT; those sit beside it as separately-licensed data, which is what the
//! GPL calls mere aggregation.
//!
//! The other thirty-four were imported from cursor sites and state no licence at
//! all. Several depict characters owned by somebody else — Batman, Spider-Man,
//! Hello Kitty, Minecraft, Skyrim, Jujutsu Kaisen. Shipping them in an installer
//! is a decision the project's owner took knowingly, with that position put to
//! them first. It is written here and in `docs/LICENSES.md` so it is never
//! something anyone has to discover.
//!
//! Undoing it is one list: delete the entries below and their archives. Nothing
//! else depends on which packs are here.

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

impl Bundled {
    /// The directory name the importer will give this pack once installed.
    ///
    /// It derives that from the folder it is handed, and the folder is named for
    /// the label — so this, not `slug`, is what tells you whether the pack is
    /// already there. `slug` names the archive and nothing else.
    fn installed_as(&self) -> String {
        paths::slugify(self.label)
    }
}

const PACKS: [Bundled; 36] = [
    Bundled { slug: "9892a", label: "9892a", archive: include_bytes!("../../assets/bundled/9892a.zip") },
    Bundled { slug: "batman-batarang", label: "Batman & Batarang", archive: include_bytes!("../../assets/bundled/batman-batarang.zip") },
    Bundled { slug: "batman-logo-face", label: "Batman Logo & Face", archive: include_bytes!("../../assets/bundled/batman-logo-face.zip") },
    Bundled { slug: "cristiano-ronaldo-siuuuu-meme-animated", label: "Cristiano Ronaldo Siuuuu Meme Animated", archive: include_bytes!("../../assets/bundled/cristiano-ronaldo-siuuuu-meme-animated.zip") },
    Bundled { slug: "cur1020", label: "Cur1020", archive: include_bytes!("../../assets/bundled/cur1020.zip") },
    Bundled { slug: "cur736", label: "Cur736", archive: include_bytes!("../../assets/bundled/cur736.zip") },
    Bundled { slug: "geared-brass", label: "Geared Brass", archive: include_bytes!("../../assets/bundled/geared-brass.zip") },
    Bundled { slug: "geared-steel", label: "Geared Steel", archive: include_bytes!("../../assets/bundled/geared-steel.zip") },
    Bundled { slug: "ghost", label: "Ghost", archive: include_bytes!("../../assets/bundled/ghost.zip") },
    Bundled { slug: "glowing-futuristic-arrow", label: "Glowing Futuristic Arrow", archive: include_bytes!("../../assets/bundled/glowing-futuristic-arrow.zip") },
    Bundled { slug: "gothic-aesthetic-dagger-skull", label: "Gothic Aesthetic Dagger & Skull", archive: include_bytes!("../../assets/bundled/gothic-aesthetic-dagger-skull.zip") },
    Bundled { slug: "green-water-gun-toy-animated", label: "Green Water Gun Toy Animated", archive: include_bytes!("../../assets/bundled/green-water-gun-toy-animated.zip") },
    Bundled { slug: "grey-bmw-m5", label: "Grey BMW M5", archive: include_bytes!("../../assets/bundled/grey-bmw-m5.zip") },
    Bundled { slug: "grey-electric-animated", label: "Grey Electric Animated", archive: include_bytes!("../../assets/bundled/grey-electric-animated.zip") },
    Bundled { slug: "hollow-knight-game-arrow", label: "Hollow Knight & Game Arrow", archive: include_bytes!("../../assets/bundled/hollow-knight-game-arrow.zip") },
    Bundled { slug: "jujutsu-kaisen-sukuna-flame-arrow-hand", label: "Jujutsu Kaisen Sukuna Flame Arrow & Hand", archive: include_bytes!("../../assets/bundled/jujutsu-kaisen-sukuna-flame-arrow-hand.zip") },
    Bundled { slug: "kuromi-naruto-crossover-akatsuki-cloak-kunai-dagger", label: "Kuromi & Naruto Crossover Akatsuki Cloak", archive: include_bytes!("../../assets/bundled/kuromi-naruto-crossover-akatsuki-cloak-kunai-dagger.zip") },
    Bundled { slug: "marvel-spider-man-venom", label: "Marvel Spider-Man & Venom", archive: include_bytes!("../../assets/bundled/marvel-spider-man-venom.zip") },
    Bundled { slug: "matrix-pixel-animated", label: "Matrix Pixel Animated", archive: include_bytes!("../../assets/bundled/matrix-pixel-animated.zip") },
    Bundled { slug: "mec424", label: "Mec424", archive: include_bytes!("../../assets/bundled/mec424.zip") },
    Bundled { slug: "minecraft-enchanted-diamond-pickaxe-animated", label: "Minecraft Enchanted Diamond Pickaxe Anim", archive: include_bytes!("../../assets/bundled/minecraft-enchanted-diamond-pickaxe-animated.zip") },
    Bundled { slug: "minecraft-enchanted-diamond-sword-animated", label: "Minecraft Enchanted Diamond Sword Animat", archive: include_bytes!("../../assets/bundled/minecraft-enchanted-diamond-sword-animated.zip") },
    Bundled { slug: "mine-craft-items", label: "Mine Craft Items", archive: include_bytes!("../../assets/bundled/mine-craft-items.zip") },
    Bundled { slug: "paper-airplane", label: "Paper Airplane", archive: include_bytes!("../../assets/bundled/paper-airplane.zip") },
    Bundled { slug: "pixel-bmw-m4-racing-car", label: "Pixel BMW M4 Racing Car", archive: include_bytes!("../../assets/bundled/pixel-bmw-m4-racing-car.zip") },
    Bundled { slug: "please-speed-i-need-this-meme-animated", label: "Please Speed I Need This Meme Animated", archive: include_bytes!("../../assets/bundled/please-speed-i-need-this-meme-animated.zip") },
    Bundled { slug: "roblox-2013", label: "Roblox 2013", archive: include_bytes!("../../assets/bundled/roblox-2013.zip") },
    Bundled { slug: "roblox-sussy-smirk-arrow-hand", label: "Roblox Sussy Smirk Arrow & Hand", archive: include_bytes!("../../assets/bundled/roblox-sussy-smirk-arrow-hand.zip") },
    Bundled { slug: "sanrio-hello-kitty-bow-arrow", label: "Sanrio Hello Kitty & Bow Arrow", archive: include_bytes!("../../assets/bundled/sanrio-hello-kitty-bow-arrow.zip") },
    Bundled { slug: "silksong-hornet-in-game-arrow", label: "Silksong Hornet & In-Game Arrow", archive: include_bytes!("../../assets/bundled/silksong-hornet-in-game-arrow.zip") },
    Bundled { slug: "sizenwse", label: "SizeNWSE", archive: include_bytes!("../../assets/bundled/sizenwse.zip") },
    Bundled { slug: "skyrim-set-2", label: "Skyrim Set 2", archive: include_bytes!("../../assets/bundled/skyrim-set-2.zip") },
    Bundled { slug: "sukuna-human-finger", label: "Sukuna Human Finger", archive: include_bytes!("../../assets/bundled/sukuna-human-finger.zip") },
    Bundled { slug: "supreme-gun-money", label: "Supreme Gun & Money", archive: include_bytes!("../../assets/bundled/supreme-gun-money.zip") },
    Bundled { slug: "toyota-gr-supra-a90-red-black-sports-car", label: "Toyota GR Supra A90 Red & Black Sports C", archive: include_bytes!("../../assets/bundled/toyota-gr-supra-a90-red-black-sports-car.zip") },
    Bundled { slug: "wpppzuou", label: "WpppZUoU", archive: include_bytes!("../../assets/bundled/wpppzuou.zip") },
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
        // Ask for the directory the importer will actually create, which is the
        // slugified *label* — not the archive's filename. Those differ for
        // several packs ("Batman & Batarang" against `batman-batarang`), and
        // checking the wrong one means the test never matches, so every pack
        // with a mismatched name is unpacked and re-imported on every single
        // launch. Nothing breaks; it is simply work done forever.
        if imported.join(pack.installed_as()).join("pack.json").is_file() {
            continue;
        }
        match install(pack, &root) {
            Ok(()) => log::info!("installed the bundled pack {}", pack.label),
            // Names the archive as well as the pack: the label is what a user
            // sees, but the archive is what someone has to go and look at.
            Err(e) => log::warn!("could not install {} ({}.zip): {e}", pack.label, pack.slug),
        }
    }
}

fn install(pack: &Bundled, root: &Path) -> AppResult<()> {
    // Unpacked into a scratch directory and then handed to the ordinary folder
    // importer, so a bundled pack goes through exactly the same role mapping,
    // `.inf` parsing and verification as one the user imports themselves. A
    // second code path here would be a second set of bugs.
    // Extracted straight into a directory named for the *label*, because that is
    // the name the importer derives the pack from — no staging copy, no rename.
    //
    // It used to unpack into a directory named for the slug and then rename that
    // to the label, and on two packs the rename destroyed the extraction. The
    // slug and the label differ only in case for `sizenwse`/`SizeNWSE` and
    // `wpppzuou`/`WpppZUoU`, Windows filesystems are case-insensitive, so
    // clearing the destination first cleared the source: same directory, two
    // spellings. Both failed with "the system cannot find the file specified",
    // which reads like a missing archive and is nothing of the kind. One name is
    // one name.
    let scratch = root.join("bundled-scratch").join(pack.label);
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

    let parent = root.join("bundled-scratch");
    crate::import::import_folder(&parent)?;
    let _ = std::fs::remove_dir_all(&parent);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh machine must not arrive at an empty catalog.
    ///
    /// This is the assertion that used to live in `catalog.rs` as "at least a
    /// hundred built-in packs", and it moved here because this is now where the
    /// catalog actually comes from. The generated packs are down to the single
    /// blend base, so if these archives ever stop being embedded, someone who
    /// installs Cursed opens it to one grey arrow — and nothing else in the
    /// build would say a word about it.
    #[test]
    fn a_fresh_install_receives_a_real_catalog() {
        assert!(
            PACKS.len() >= 30,
            "only {} packs are bundled; a machine with no imports would look bare",
            PACKS.len()
        );

        let mut slugs: Vec<&str> = PACKS.iter().map(|p| p.slug).collect();
        slugs.sort_unstable();
        let before = slugs.len();
        slugs.dedup();
        assert_eq!(before, slugs.len(), "two bundled packs share a slug");
    }

    /// No two packs may land in the same directory.
    ///
    /// The directory comes from the label, so two labels that slugify the same
    /// would have one pack silently overwrite the other — 36 archives, 35 packs,
    /// and nothing to say which one lost.
    #[test]
    fn no_two_bundled_packs_install_over_each_other() {
        let mut seen: Vec<String> = PACKS.iter().map(|p| p.installed_as()).collect();
        seen.sort();
        let before = seen.len();
        seen.dedup();
        assert_eq!(before, seen.len(), "two bundled packs install to the same directory");

        for pack in &PACKS {
            assert!(
                !pack.installed_as().is_empty(),
                "{} slugifies to nothing, so it would install to the imports root",
                pack.label
            );
        }
    }

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

    /// The packs that *do* have a licence must still ship the text of it.
    ///
    /// This used to require it of every pack, which is the right rule and is no
    /// longer the situation: thirty-four of these state no licence at all and
    /// are shipped anyway, on the owner's decision, as the module header
    /// records. Deleting the test along with the rule would have quietly removed
    /// the only thing checking that Geared Brass and Geared Steel still carry
    /// the GPL text that permits them to be here — the two packs where dropping
    /// a file genuinely does turn redistribution into infringement.
    ///
    /// So it now names them. A licensed pack that loses its licence text fails;
    /// the rest are counted, not asserted, so the ratio is visible in the test
    /// output rather than buried.
    #[test]
    fn licensed_packs_still_carry_their_licence_text() {
        const LICENSED: [&str; 2] = ["geared-brass", "geared-steel"];
        let mut unlicensed = 0;

        for pack in &PACKS {
            let cursor = std::io::Cursor::new(pack.archive);
            let mut archive = zip::ZipArchive::new(cursor).expect("readable");
            let names: Vec<String> = (0..archive.len())
                .filter_map(|i| archive.by_index(i).ok().map(|e| e.name().to_ascii_uppercase()))
                .collect();
            let has = |needle: &str| names.iter().any(|n| n.contains(needle));

            if LICENSED.contains(&pack.slug) {
                assert!(has("LICENSE"), "{} ships without its LICENSE", pack.label);
                assert!(has("COPYRIGHT"), "{} ships without its COPYRIGHT", pack.label);
            } else if !has("LICENSE") {
                unlicensed += 1;
            }
        }

        assert_eq!(
            unlicensed,
            PACKS.len() - LICENSED.len(),
            "the licence status of a bundled pack changed; update LICENSED and docs/LICENSES.md"
        );
    }
}
