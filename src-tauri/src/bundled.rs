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
//! **Two of these carry a licence. Eighty-one do not.**
//!
//! `geared-brass` and `geared-steel` are GPL-3.0, with the author stating the
//! assets are their own work, and their archives carry the `LICENSE.txt` and
//! `COPYRIGHT.txt` that say so — extracted alongside the cursors. Cursed itself
//! stays MIT; those sit beside it as separately-licensed data, which is what the
//! GPL calls mere aggregation.
//!
//! Thirty-four were imported from cursor sites and state no licence at all.
//! Forty-seven were given to the project's owner directly by the person who
//! drew them, to use here, with no credit asked for and none given — which is
//! why those archives hold their two cursor files and nothing else, and why the
//! `LIST INFO` chunk naming the artist was stripped out of the animations
//! themselves. Deleting a readme does not empty a RIFF chunk.
//!
//! Several across both groups depict characters owned by somebody else — Batman,
//! Spider-Man, Hello Kitty, Minecraft, Skyrim, Jujutsu Kaisen, One Piece,
//! Naruto, Roblox, Shrek. Permission from the person who drew a cursor is not
//! permission from whoever owns what it depicts.
//!
//! Shipping them in an installer is a decision the project's owner took
//! knowingly, with that position put to them first. It is written here and in
//! `docs/LICENSES.md` so it is never something anyone has to discover.
//!
//! Undoing it is one list: delete the entries below and their archives. Nothing
//! else depends on which packs are here.
//!
//! ## Why the files inside an archive are named `arrow-role` and `hand-role`
//!
//! The importer reads a role out of the *whole* filename stem, and it checks
//! "hand" before it checks "cursor". A pack shipped under its download name —
//! `Minecraft Steve Raising Hands--cursor--....cur` — therefore maps its
//! **arrow** to the hand, collides with the real hand file, and installs as a
//! pack with no pointer at all. Two of the forty-seven hit that exactly, and
//! nothing would have reported it: the pack installs, it just has one role.
//!
//! So each archive holds exactly one `arrow-role.{cur,ani}` and one
//! `hand-role.{cur,ani}`, named for the role rather than for the artwork, and
//! nothing else at all. The PNG previews, readme and shortcut files that come in
//! a download are dropped — a `.png` sharing a stem with a `.cur` is a second
//! candidate for the same role, and which one wins is directory order.

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

const PACKS: [Bundled; 83] = [
    Bundled { slug: "9892a", label: "9892a", archive: include_bytes!("../../assets/bundled/9892a.zip") },
    Bundled { slug: "awkward-look-monkey-puppet-meme-3d", label: "Awkward Look Monkey Puppet Meme 3D", archive: include_bytes!("../../assets/bundled/awkward-look-monkey-puppet-meme-3d.zip") },
    Bundled { slug: "banana-cat-meme", label: "Banana Cat Meme", archive: include_bytes!("../../assets/bundled/banana-cat-meme.zip") },
    Bundled { slug: "batman-batarang", label: "Batman & Batarang", archive: include_bytes!("../../assets/bundled/batman-batarang.zip") },
    Bundled { slug: "batman-logo-face", label: "Batman Logo & Face", archive: include_bytes!("../../assets/bundled/batman-logo-face.zip") },
    Bundled { slug: "cells-at-work-white-blood-cell-knife", label: "Cells at Work! White Blood Cell & Knife", archive: include_bytes!("../../assets/bundled/cells-at-work-white-blood-cell-knife.zip") },
    Bundled { slug: "chrome-hearts-sneaker-cross-arrow", label: "Chrome Hearts Sneaker & Cross Arrow", archive: include_bytes!("../../assets/bundled/chrome-hearts-sneaker-cross-arrow.zip") },
    Bundled { slug: "cristiano-ronaldo-siuuuu-meme-animated", label: "Cristiano Ronaldo Siuuuu Meme Animated", archive: include_bytes!("../../assets/bundled/cristiano-ronaldo-siuuuu-meme-animated.zip") },
    Bundled { slug: "cur1020", label: "Cur1020", archive: include_bytes!("../../assets/bundled/cur1020.zip") },
    Bundled { slug: "cur736", label: "Cur736", archive: include_bytes!("../../assets/bundled/cur736.zip") },
    Bundled { slug: "dc-joker-card", label: "DC Joker & Card", archive: include_bytes!("../../assets/bundled/dc-joker-card.zip") },
    Bundled { slug: "demon-slayer-water-breathing-sword", label: "Demon Slayer Water Breathing Sword", archive: include_bytes!("../../assets/bundled/demon-slayer-water-breathing-sword.zip") },
    Bundled { slug: "doraemon-pancake", label: "Doraemon & Pancake", archive: include_bytes!("../../assets/bundled/doraemon-pancake.zip") },
    Bundled { slug: "dragon-ball-black-rose-scythe", label: "Dragon Ball Black Rose & Scythe", archive: include_bytes!("../../assets/bundled/dragon-ball-black-rose-scythe.zip") },
    Bundled { slug: "dragon-ball-goku-arrow-animated", label: "Dragon Ball Goku & Arrow Animated", archive: include_bytes!("../../assets/bundled/dragon-ball-goku-arrow-animated.zip") },
    Bundled { slug: "dragon-ball-goku-face-energy-arrow", label: "Dragon Ball Goku Face & Energy Arrow", archive: include_bytes!("../../assets/bundled/dragon-ball-goku-face-energy-arrow.zip") },
    Bundled { slug: "geared-brass", label: "Geared Brass", archive: include_bytes!("../../assets/bundled/geared-brass.zip") },
    Bundled { slug: "geared-steel", label: "Geared Steel", archive: include_bytes!("../../assets/bundled/geared-steel.zip") },
    Bundled { slug: "ghost", label: "Ghost", archive: include_bytes!("../../assets/bundled/ghost.zip") },
    Bundled { slug: "glowing-futuristic-arrow", label: "Glowing Futuristic Arrow", archive: include_bytes!("../../assets/bundled/glowing-futuristic-arrow.zip") },
    Bundled { slug: "gothic-aesthetic-dagger-skull", label: "Gothic Aesthetic Dagger & Skull", archive: include_bytes!("../../assets/bundled/gothic-aesthetic-dagger-skull.zip") },
    Bundled { slug: "green-water-gun-toy-animated", label: "Green Water Gun Toy Animated", archive: include_bytes!("../../assets/bundled/green-water-gun-toy-animated.zip") },
    Bundled { slug: "grey-bmw-m5", label: "Grey BMW M5", archive: include_bytes!("../../assets/bundled/grey-bmw-m5.zip") },
    Bundled { slug: "grey-electric-animated", label: "Grey Electric Animated", archive: include_bytes!("../../assets/bundled/grey-electric-animated.zip") },
    Bundled { slug: "haaland-onion-meme", label: "Haaland Onion Meme", archive: include_bytes!("../../assets/bundled/haaland-onion-meme.zip") },
    Bundled { slug: "halo-energy-sword", label: "Halo Energy Sword", archive: include_bytes!("../../assets/bundled/halo-energy-sword.zip") },
    Bundled { slug: "hello-kitty-hearts-pixel-animated", label: "Hello Kitty & Hearts Pixel Animated", archive: include_bytes!("../../assets/bundled/hello-kitty-hearts-pixel-animated.zip") },
    Bundled { slug: "hello-kitty-pusheen", label: "Hello Kitty & Pusheen", archive: include_bytes!("../../assets/bundled/hello-kitty-pusheen.zip") },
    Bundled { slug: "hollow-knight-game-arrow", label: "Hollow Knight & Game Arrow", archive: include_bytes!("../../assets/bundled/hollow-knight-game-arrow.zip") },
    Bundled { slug: "jujutsu-kaisen-choso-blood-manipulation", label: "Jujutsu Kaisen Choso Blood Manipulation", archive: include_bytes!("../../assets/bundled/jujutsu-kaisen-choso-blood-manipulation.zip") },
    Bundled { slug: "jujutsu-kaisen-gojo-cat-sword", label: "Jujutsu Kaisen Gojo Cat Sword", archive: include_bytes!("../../assets/bundled/jujutsu-kaisen-gojo-cat-sword.zip") },
    Bundled { slug: "jujutsu-kaisen-sukuna-flame-arrow-hand", label: "Jujutsu Kaisen Sukuna Flame Arrow & Hand", archive: include_bytes!("../../assets/bundled/jujutsu-kaisen-sukuna-flame-arrow-hand.zip") },
    Bundled { slug: "just-a-chill-guy-3d-meme-animated", label: "Just a Chill Guy 3D Meme Animated", archive: include_bytes!("../../assets/bundled/just-a-chill-guy-3d-meme-animated.zip") },
    Bundled { slug: "kfc-fried-chicken-bucket", label: "KFC Fried Chicken Bucket", archive: include_bytes!("../../assets/bundled/kfc-fried-chicken-bucket.zip") },
    Bundled { slug: "kingdom-hearts-riku-soul-eater", label: "Kingdom Hearts Riku & Soul Eater", archive: include_bytes!("../../assets/bundled/kingdom-hearts-riku-soul-eater.zip") },
    Bundled { slug: "kuromi-naruto-crossover-akatsuki-cloak-kunai-dagger", label: "Kuromi & Naruto Crossover Akatsuki Cloak", archive: include_bytes!("../../assets/bundled/kuromi-naruto-crossover-akatsuki-cloak-kunai-dagger.zip") },
    Bundled { slug: "kuromi-notebook-pen", label: "Kuromi Notebook Pen", archive: include_bytes!("../../assets/bundled/kuromi-notebook-pen.zip") },
    Bundled { slug: "lamine-yamal-gold-cup-trophy-animated", label: "Lamine Yamal & Gold Cup Trophy Animated", archive: include_bytes!("../../assets/bundled/lamine-yamal-gold-cup-trophy-animated.zip") },
    Bundled { slug: "marvel-spider-man-venom", label: "Marvel Spider-Man & Venom", archive: include_bytes!("../../assets/bundled/marvel-spider-man-venom.zip") },
    Bundled { slug: "matrix-pixel-animated", label: "Matrix Pixel Animated", archive: include_bytes!("../../assets/bundled/matrix-pixel-animated.zip") },
    Bundled { slug: "mec424", label: "Mec424", archive: include_bytes!("../../assets/bundled/mec424.zip") },
    Bundled { slug: "messi-world-cup-animated", label: "Messi & World Cup Animated", archive: include_bytes!("../../assets/bundled/messi-world-cup-animated.zip") },
    Bundled { slug: "miffy-listen-to-music", label: "Miffy Listen To Music", archive: include_bytes!("../../assets/bundled/miffy-listen-to-music.zip") },
    Bundled { slug: "mine-craft-items", label: "Mine Craft Items", archive: include_bytes!("../../assets/bundled/mine-craft-items.zip") },
    Bundled { slug: "minecraft-enchanted-diamond-pickaxe-animated", label: "Minecraft Enchanted Diamond Pickaxe Anim", archive: include_bytes!("../../assets/bundled/minecraft-enchanted-diamond-pickaxe-animated.zip") },
    Bundled { slug: "minecraft-enchanted-diamond-sword-animated", label: "Minecraft Enchanted Diamond Sword Animat", archive: include_bytes!("../../assets/bundled/minecraft-enchanted-diamond-sword-animated.zip") },
    Bundled { slug: "minecraft-fat-chicken", label: "Minecraft Fat Chicken", archive: include_bytes!("../../assets/bundled/minecraft-fat-chicken.zip") },
    Bundled { slug: "minecraft-steve-raising-hands", label: "Minecraft Steve Raising Hands", archive: include_bytes!("../../assets/bundled/minecraft-steve-raising-hands.zip") },
    Bundled { slug: "moyai-emoji-meme", label: "Moyai Emoji Meme", archive: include_bytes!("../../assets/bundled/moyai-emoji-meme.zip") },
    Bundled { slug: "naruto-arrow-animated", label: "Naruto & Arrow Animated", archive: include_bytes!("../../assets/bundled/naruto-arrow-animated.zip") },
    Bundled { slug: "one-piece-luffy-arrow-animated", label: "One Piece Luffy & Arrow Animated", archive: include_bytes!("../../assets/bundled/one-piece-luffy-arrow-animated.zip") },
    Bundled { slug: "one-piece-roronoa-zoro-swords-animated", label: "One Piece Roronoa Zoro & Swords Animated", archive: include_bytes!("../../assets/bundled/one-piece-roronoa-zoro-swords-animated.zip") },
    Bundled { slug: "one-piece-zoro-demon-form-shusui", label: "One Piece Zoro Demon Form & Shusui", archive: include_bytes!("../../assets/bundled/one-piece-zoro-demon-form-shusui.zip") },
    Bundled { slug: "oo-ee-a-e-a-cat-meme-animated", label: "Oo Ee A E A Cat Meme Animated", archive: include_bytes!("../../assets/bundled/oo-ee-a-e-a-cat-meme-animated.zip") },
    Bundled { slug: "paper-airplane", label: "Paper Airplane", archive: include_bytes!("../../assets/bundled/paper-airplane.zip") },
    Bundled { slug: "pixel-bmw-m4-racing-car", label: "Pixel BMW M4 Racing Car", archive: include_bytes!("../../assets/bundled/pixel-bmw-m4-racing-car.zip") },
    Bundled { slug: "please-speed-i-need-this-meme-animated", label: "Please Speed I Need This Meme Animated", archive: include_bytes!("../../assets/bundled/please-speed-i-need-this-meme-animated.zip") },
    Bundled { slug: "roblox-2013", label: "Roblox 2013", archive: include_bytes!("../../assets/bundled/roblox-2013.zip") },
    Bundled { slug: "roblox-baby", label: "Roblox Baby", archive: include_bytes!("../../assets/bundled/roblox-baby.zip") },
    Bundled { slug: "roblox-black-white-cat", label: "Roblox Black & White Cat", archive: include_bytes!("../../assets/bundled/roblox-black-white-cat.zip") },
    Bundled { slug: "roblox-mega-noob", label: "Roblox Mega Noob", archive: include_bytes!("../../assets/bundled/roblox-mega-noob.zip") },
    Bundled { slug: "roblox-steal-brainrot-67-animated", label: "Roblox Steal Brainrot 67 Animated", archive: include_bytes!("../../assets/bundled/roblox-steal-brainrot-67-animated.zip") },
    Bundled { slug: "roblox-sussy-smirk-arrow-hand", label: "Roblox Sussy Smirk Arrow & Hand", archive: include_bytes!("../../assets/bundled/roblox-sussy-smirk-arrow-hand.zip") },
    Bundled { slug: "sad-hamster-meme", label: "Sad Hamster Meme", archive: include_bytes!("../../assets/bundled/sad-hamster-meme.zip") },
    Bundled { slug: "sanrio-cinnamoroll-ice-cream-pixel", label: "Sanrio Cinnamoroll & Ice Cream Pixel", archive: include_bytes!("../../assets/bundled/sanrio-cinnamoroll-ice-cream-pixel.zip") },
    Bundled { slug: "sanrio-hello-kitty-bow-arrow", label: "Sanrio Hello Kitty & Bow Arrow", archive: include_bytes!("../../assets/bundled/sanrio-hello-kitty-bow-arrow.zip") },
    Bundled { slug: "sanrio-pochacco-carrot", label: "Sanrio Pochacco & Carrot", archive: include_bytes!("../../assets/bundled/sanrio-pochacco-carrot.zip") },
    Bundled { slug: "shrek-funny-face-meme", label: "Shrek Funny Face Meme", archive: include_bytes!("../../assets/bundled/shrek-funny-face-meme.zip") },
    Bundled { slug: "silksong-hornet-in-game-arrow", label: "Silksong Hornet & In-Game Arrow", archive: include_bytes!("../../assets/bundled/silksong-hornet-in-game-arrow.zip") },
    Bundled { slug: "sizenwse", label: "SizeNWSE", archive: include_bytes!("../../assets/bundled/sizenwse.zip") },
    Bundled { slug: "skyrim-set-2", label: "Skyrim Set 2", archive: include_bytes!("../../assets/bundled/skyrim-set-2.zip") },
    Bundled { slug: "solo-leveling-sung-jin-woo-dark-flames", label: "Solo Leveling Sung Jin-Woo Dark Flames", archive: include_bytes!("../../assets/bundled/solo-leveling-sung-jin-woo-dark-flames.zip") },
    Bundled { slug: "spider-man", label: "Spider-Man", archive: include_bytes!("../../assets/bundled/spider-man.zip") },
    Bundled { slug: "spider-man-hand-mask", label: "Spider-Man Hand & Mask", archive: include_bytes!("../../assets/bundled/spider-man-hand-mask.zip") },
    Bundled { slug: "spongebob-patrick-star", label: "SpongeBob & Patrick Star", archive: include_bytes!("../../assets/bundled/spongebob-patrick-star.zip") },
    Bundled { slug: "steroid-strong-goose-meme", label: "Steroid Strong Goose Meme", archive: include_bytes!("../../assets/bundled/steroid-strong-goose-meme.zip") },
    Bundled { slug: "sukuna-human-finger", label: "Sukuna Human Finger", archive: include_bytes!("../../assets/bundled/sukuna-human-finger.zip") },
    Bundled { slug: "supreme-gun-money", label: "Supreme Gun & Money", archive: include_bytes!("../../assets/bundled/supreme-gun-money.zip") },
    Bundled { slug: "swole-doge-vs-cheems-meme", label: "Swole Doge vs. Cheems Meme", archive: include_bytes!("../../assets/bundled/swole-doge-vs-cheems-meme.zip") },
    Bundled { slug: "toyota-gr-supra-a90-red-black-sports-car", label: "Toyota GR Supra A90 Red & Black Sports C", archive: include_bytes!("../../assets/bundled/toyota-gr-supra-a90-red-black-sports-car.zip") },
    Bundled { slug: "tung-tung-tung-sahur-brainrot-meme", label: "Tung Tung Tung Sahur Brainrot Meme", archive: include_bytes!("../../assets/bundled/tung-tung-tung-sahur-brainrot-meme.zip") },
    Bundled { slug: "vintage-cartoon-raccoon-glove-hand", label: "Vintage Cartoon Raccoon & Glove Hand", archive: include_bytes!("../../assets/bundled/vintage-cartoon-raccoon-glove-hand.zip") },
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

    /// A pack must install to the directory this module goes looking for.
    ///
    /// `install_missing` skips a pack when `imported/<installed_as()>/pack.json`
    /// is there, and `installed_as()` is the slugified **label**. The importer
    /// does not use the label directly: it names the directory after the pack
    /// name it *derives*, which is `pretty_folder_name` of the folder it was
    /// handed. Those agree for every label until one ends in a word the importer
    /// treats as download-site noise and strips — "Cursor", "Pointer", "Link",
    /// "Normal", "Set".
    ///
    /// Label a pack `Banana Cat Meme Cursor` and it installs to
    /// `banana-cat-meme` while the check looks for `banana-cat-meme-cursor`.
    /// Nothing breaks and nothing is logged; the pack is simply unpacked,
    /// re-imported and rewritten on every launch for the life of the install.
    /// The comment in `install_missing` has said so since two packs did it. It
    /// is an assertion now because forty-seven of these came from a site that
    /// names every download "... Cursor", so the mistake is one paste away and
    /// invisible on the machine that makes it.
    #[test]
    fn every_label_names_the_directory_the_importer_will_create() {
        for pack in &PACKS {
            let derived = paths::slugify(&crate::import::pretty_folder_name(pack.label));
            assert_eq!(
                derived,
                pack.installed_as(),
                "{} installs as \"{}\" but is looked for at \"{}\", so it would                  reinstall on every launch",
                pack.label,
                derived,
                pack.installed_as()
            );
        }
    }

    /// A two-cursor pack must fill two different roles.
    ///
    /// These arrive as a pointer and a hand, and the importer works out which is
    /// which by reading a role out of the *whole* filename stem — checking
    /// "hand" before it checks "cursor". Left under their download names,
    /// `Minecraft Steve Raising Hands--cursor--...` and
    /// `Spider-Man Hand & Mask--cursor--...` both claim the hand, collide with
    /// the file that really is the hand, and install as a pack with no pointer.
    ///
    /// That is why the entries are renamed `arrow-role` and `hand-role` on the
    /// way in. This is the assertion that they still are: a pack that loses its
    /// pointer still installs, still appears in the catalog, and says nothing.
    ///
    /// Only two-cursor archives are checked. A full scheme legitimately ships
    /// several files for one role — `Move.ani` beside `Move_1.ani` — and the
    /// claim system is there to pick between them.
    #[test]
    fn a_two_cursor_pack_fills_two_roles() {
        for pack in &PACKS {
            let cursor = std::io::Cursor::new(pack.archive);
            let mut archive = zip::ZipArchive::new(cursor).expect("readable");
            let stems: Vec<String> = (0..archive.len())
                .filter_map(|i| archive.by_index(i).ok().map(|e| e.name().to_owned()))
                .filter(|n| {
                    let lower = n.to_ascii_lowercase();
                    lower.ends_with(".cur") || lower.ends_with(".ani")
                })
                .map(|n| Path::new(&n).file_stem().unwrap_or_default().to_string_lossy().into_owned())
                .collect();
            if stems.len() != 2 {
                continue;
            }
            let roles: Vec<_> = stems
                .iter()
                .map(|s| crate::import::role_from_filename(s).map(|(role, _)| role))
                .collect();
            assert!(
                roles.iter().all(Option::is_some),
                "{}: {:?} does not name a role, so it would be guessed at",
                pack.label,
                stems
            );
            assert_ne!(
                roles[0], roles[1],
                "{}: {:?} both read as {:?}, so one would overwrite the other and                  the pack would install with a single role",
                pack.label, stems, roles[0]
            );
        }
    }

    /// No two files in an archive may share a stem.
    ///
    /// The download these come from ships a `.png` preview beside every cursor,
    /// under the same name. The importer accepts images as sources too, so both
    /// files claim the same role with the same confidence — and which one wins
    /// is whatever order the directory happens to be read in. A pack would then
    /// be the real cursor or a flat picture of it, decided by the filesystem.
    ///
    /// The previews are dropped when an archive is built. This is what stops
    /// somebody dropping a raw download in later and never finding out.
    #[test]
    fn no_two_files_in_an_archive_compete_for_the_same_role() {
        for pack in &PACKS {
            let cursor = std::io::Cursor::new(pack.archive);
            let mut archive = zip::ZipArchive::new(cursor).expect("readable");
            let mut stems: Vec<String> = (0..archive.len())
                .filter_map(|i| archive.by_index(i).ok().map(|e| e.name().to_owned()))
                .filter_map(|n| {
                    Path::new(&n)
                        .file_stem()
                        .map(|s| s.to_string_lossy().to_ascii_lowercase())
                })
                .collect();
            stems.sort();
            let before = stems.len();
            stems.dedup();
            assert_eq!(before, stems.len(), "{} ships two files with one stem", pack.label);
        }
    }

    /// Windows must accept every cursor file that ships in the installer.
    ///
    /// The importer verifies each file as it installs it and, on refusal, skips
    /// that one role and keeps the rest — "one bad file in a 47-file scheme must
    /// not cost the other 46". That is the right behaviour at import and it is
    /// exactly what makes a bad file invisible here: the pack still installs,
    /// still shows in the catalog, and is simply missing its hand. The log line
    /// lands on the user's machine on first run, where nobody is reading it.
    ///
    /// So the loader is asked here instead, against the real files, on the
    /// machine building the release. `verify_loadable` is the same call the
    /// importer makes — `LoadCursorFromFileW` for an `.ani`, because loading one
    /// with an explicit size returns a still frame and proves nothing about the
    /// animation.
    #[test]
    fn windows_accepts_every_bundled_cursor() {
        let dir = std::env::temp_dir().join("cursorforge-tests").join("bundled");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        let mut checked = 0usize;

        for pack in &PACKS {
            let cursor = std::io::Cursor::new(pack.archive);
            let mut archive = zip::ZipArchive::new(cursor).expect("readable");
            for index in 0..archive.len() {
                let mut entry = archive.by_index(index).expect("entry");
                let name = entry.name().to_ascii_lowercase();
                if !(name.ends_with(".cur") || name.ends_with(".ani")) {
                    continue;
                }
                // Flattened to one component, and prefixed with the pack, so two
                // packs' `arrow-role.cur` cannot overwrite each other mid-test.
                let Some(leaf) = Path::new(&name).file_name().map(|n| n.to_string_lossy().into_owned())
                else {
                    continue;
                };
                let path = dir.join(format!("{}-{leaf}", pack.slug));
                let mut bytes = Vec::new();
                std::io::Read::read_to_end(&mut entry, &mut bytes).expect("read");
                std::fs::write(&path, &bytes).expect("write");

                let loaded = crate::cursor::engine::verify_loadable(&path);
                let _ = std::fs::remove_file(&path);
                assert!(
                    loaded.is_ok(),
                    "{} ships {leaf}, which Windows refuses ({loaded:?}) — it would                      install as a pack with that role missing and say nothing",
                    pack.label
                );
                checked += 1;
            }
        }

        let _ = std::fs::remove_dir_all(&dir);
        assert!(checked >= PACKS.len(), "only {checked} cursor files were checked");
    }

    /// **The end-to-end check, and the first-run cost. Run it deliberately:**
    ///
    /// ```text
    /// cargo test --release --lib a_clean_machine_installs_every_pack -- --ignored --nocapture --test-threads=1
    /// ```
    ///
    /// `#[ignore]` because it points `APPDATA` at a temporary directory for the
    /// whole **process**, and any test running beside it would then read and
    /// write the wrong storage root. Same reason as the memory measurement in
    /// `photo.rs`: a thing that is only true when nothing else is running
    /// belongs behind a flag rather than in the gate.
    ///
    /// Every other test in this module reads the archives. This one runs the
    /// code that turns them into packs, and it is the only place three failures
    /// are visible at all:
    ///
    /// - a cursor Windows refuses is **skipped**, so the pack installs with a
    ///   role missing and nothing says so;
    /// - a label the importer rewrites installs to a directory the launch check
    ///   never looks at, so the pack is rebuilt on **every** launch — which the
    ///   second `install_missing()` below is here to catch;
    /// - `install_missing` runs synchronously in Tauri's `setup`, before the
    ///   window is shown, so its cost is the user's first-run wait.
    #[test]
    #[ignore]
    fn a_clean_machine_installs_every_pack() {
        use crate::cursor::roles::Role;

        let root = std::env::temp_dir().join("cursorforge-tests").join("clean-machine");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("temporary APPDATA");
        std::env::set_var("APPDATA", &root);

        let started = std::time::Instant::now();
        install_missing();
        let cold = started.elapsed();

        let installed = crate::import::list().expect("the catalog reads back");
        println!("first run: {} packs in {:?}", installed.len(), cold);
        assert_eq!(
            installed.len(),
            PACKS.len(),
            "{} archives went in and {} packs came out",
            PACKS.len(),
            installed.len()
        );

        for pack in &PACKS {
            let id = format!("user:{}", pack.installed_as());
            let found = installed
                .iter()
                .find(|p| p.id == id)
                .unwrap_or_else(|| panic!("{} did not install as {id}", pack.label));

            // Two cursors in, two roles out. A file Windows refused would show
            // up here as one, and only here.
            let cursors = std::io::Cursor::new(pack.archive);
            let mut archive = zip::ZipArchive::new(cursors).expect("readable");
            let count = (0..archive.len())
                .filter_map(|i| archive.by_index(i).ok().map(|e| e.name().to_ascii_lowercase()))
                .filter(|n| n.ends_with(".cur") || n.ends_with(".ani"))
                .count();
            if count == 2 {
                assert!(
                    found.roles.contains_key(&Role::Arrow) && found.roles.contains_key(&Role::Hand),
                    "{} shipped two cursors and installed {:?} — a role was dropped",
                    pack.label,
                    found.roles.keys().collect::<Vec<_>>()
                );
            }
            assert!(!found.roles.is_empty(), "{} installed with no roles", pack.label);

            // No name reaches the user that the pack did not ship on purpose.
            // The importer reads an author out of any `.txt` in the archive and
            // shows it as the pack's source, so an archive carrying no text
            // must install with that field empty. This is the assertion behind
            // "no credit" — dropping a readme is easy to undo by accident when
            // the next pack is added from a download that includes one.
            let has_text = (0..archive.len())
                .filter_map(|i| archive.by_index(i).ok().map(|e| e.name().to_ascii_lowercase()))
                .any(|n| n.ends_with(".txt"));
            if !has_text {
                assert!(
                    found.source.is_empty(),
                    "{} carries no readme yet installed with the source {:?}",
                    pack.label,
                    found.source
                );
            }
        }

        // Nothing may be done twice. `install_missing` runs on every launch by
        // design, and its whole claim to being free is that the second call is a
        // directory test. If a label does not name the directory the importer
        // creates, every pack is silently unpacked and rewritten here — the
        // timestamps are what prove it did not happen.
        let before: Vec<String> = installed.iter().map(|p| p.imported.clone()).collect();
        let started = std::time::Instant::now();
        install_missing();
        let warm = started.elapsed();
        let after: Vec<String> = crate::import::list()
            .expect("the catalog reads back")
            .iter()
            .map(|p| p.imported.clone())
            .collect();

        println!("second run: {warm:?}");
        assert_eq!(before, after, "a second launch reinstalled packs that were already there");
        assert!(
            warm < cold / 10,
            "a second launch cost {warm:?} against a first run of {cold:?}, so it is doing real work"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    /// A pack that ships nothing but cursors must not carry a name inside one.
    ///
    /// Stripping the readme out of an archive is not enough. An `.ani` is a RIFF
    /// file, and every one of these arrived with a `LIST INFO` chunk holding
    /// `INAM` (the download's title) and `IART` (whoever drew it) — inside the
    /// cursor, invisible to a directory listing, and shown by Windows in the
    /// file's own properties. Forty-seven archives were rebuilt to hold only
    /// their two cursor files and eighteen of them still had the name in the
    /// bytes.
    ///
    /// The rule is keyed to the shape rather than to a list: an archive whose
    /// entries are *only* cursors is one built to that convention, and must be
    /// clean. Packs that ship a `pack.json`, a readme or a licence are the older
    /// ones and are left alone — `ghost` uses `INAM` to name the role, and a
    /// pack that credits its author should go on doing so.
    #[test]
    fn a_cursors_only_pack_carries_no_name_inside_its_animation() {
        for pack in &PACKS {
            let cursor = std::io::Cursor::new(pack.archive);
            let mut archive = zip::ZipArchive::new(cursor).expect("readable");
            let names: Vec<String> =
                (0..archive.len())
                    .filter_map(|i| archive.by_index(i).ok().map(|e| e.name().to_ascii_lowercase()))
                    .collect();
            if !names.iter().all(|n| n.ends_with(".cur") || n.ends_with(".ani")) {
                continue;
            }

            for name in names.iter().filter(|n| n.ends_with(".ani")) {
                let mut entry = archive.by_name(name).expect("entry");
                let mut bytes = Vec::new();
                std::io::Read::read_to_end(&mut entry, &mut bytes).expect("read");
                assert!(
                    !riff_has_info(&bytes),
                    "{} ships {name} with a RIFF INFO chunk, which is where the                      title and the artist live",
                    pack.label
                );
            }
        }
    }

    /// Whether a RIFF file carries a top-level `LIST INFO`. Walks the chunk list
    /// rather than searching for the tag, so pixel data that happens to spell
    /// `IART` cannot fail the test above.
    fn riff_has_info(bytes: &[u8]) -> bool {
        if bytes.len() < 12 || &bytes[..4] != b"RIFF" {
            return false;
        }
        let mut at = 12usize;
        while at + 8 <= bytes.len() {
            let size = u32::from_le_bytes([bytes[at + 4], bytes[at + 5], bytes[at + 6], bytes[at + 7]]) as usize;
            if &bytes[at..at + 4] == b"LIST"
                && at + 12 <= bytes.len()
                && &bytes[at + 8..at + 12] == b"INFO"
            {
                return true;
            }
            // Chunks are word-aligned: an odd size is followed by a pad byte.
            let Some(next) = size.checked_add(8 + (size & 1)).and_then(|s| at.checked_add(s)) else {
                return false;
            };
            at = next;
        }
        false
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
