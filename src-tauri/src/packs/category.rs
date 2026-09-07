//! What shelf a cursor belongs on.
//!
//! The catalog used to have two categories — `OPTIMAL CURSED` and `MINIMAL
//! CURSED` — and the second was deliberately empty. With a hundred and
//! thirty-three packs shipping, a single undifferentiated grid is not something
//! anyone can find anything in, so a pack is now filed by what it *is*.
//!
//! ## Why this is derived from the name rather than stored
//!
//! A pack's category lives in its `pack.json`, written at import time. Every
//! pack installed before this module existed therefore has `OPTIMAL CURSED`
//! written into it, and those files are on users' disks — not in this repo.
//! Trusting the stored value would mean the entire existing catalog stays in one
//! bucket until each pack is re-imported, which nothing prompts a user to do.
//!
//! So the category is worked out from the name every time the catalog is
//! listed. It costs a few hundred string comparisons on a screen that already
//! decodes a hundred and thirty PNGs, it needs no migration, and it fixes
//! itself the moment these rules improve. `pack.json` still carries the value
//! for anything that wants it; nothing reads it back.
//!
//! ## Word boundaries, not substrings
//!
//! Matching is on whole words. This is not fastidiousness: `Cute Fluffy Cat Paw
//! Arrow` filed itself under ANIME because "f-**luffy**" contains the One Piece
//! key, and that is the entire class of bug this shape of matching removes. The
//! name is reduced to its alphanumeric words, joined by single spaces and padded
//! at both ends, so a key only matches on whole-word boundaries.
//!
//! Order matters and is deliberate: a franchise beats a generic noun. `Halo
//! Energy Sword` is a game, not a weapon; `Demon Slayer Water Breathing Sword`
//! is anime, not a weapon; `Blue Lock Rin Itoshi & Soccer Ball` is anime, not
//! sport. The generic shelves — WEAPONS, TECH, OTHER — catch what is left.

/// Every shelf, in the order the catalog shows them.
///
/// `OTHER` is last because it is the fallback, and the UI puts ALL, ANIMATED and
/// STATIC in front of these — those three are filters over the whole set rather
/// than categories a pack is filed under, so they are not in this list.
pub const CATEGORIES: [&str; 10] = [
    "ANIME",
    "GAMING",
    "MEMES",
    "CUTE",
    "MOVIES & TV",
    "CARS",
    "WEAPONS",
    "SPORTS",
    "TECH",
    "OTHER",
];

/// The shelves in match order, each with the words that put a pack on it.
///
/// This is not the display order: a franchise has to be tested before the
/// generic noun it contains, or every anime pack holding a sword becomes a
/// weapon.
const RULES: [(&str, &[&str]); 9] = [
    (
        "CARS",
        &[
            "bmw", "audi", "lamborghini", "huracan", "toyota", "supra", "mclaren", "mcqueen",
            "porsche", "ferrari", "nissan", "gtr", "m4", "m5", "racing car", "sports car", "cars",
        ],
    ),
    (
        "ANIME",
        &[
            "naruto", "one piece", "luffy", "zoro", "sanji", "roronoa", "trafalgar", "shusui",
            "jujutsu", "gojo", "sukuna", "choso", "demon slayer", "kny", "muichiro", "tokito",
            "dragon ball", "goku", "bleach", "rukia", "kuchiki", "blue lock", "itoshi",
            "solo leveling", "sung jin woo", "himouto", "umaru", "rascal does not dream",
            "sakurajima", "cells at work", "doraemon", "akatsuki", "chibi", "anime", "manga",
            "tybw",
        ],
    ),
    (
        "GAMING",
        &[
            "minecraft", "mine craft", "roblox", "hollow knight", "silksong", "halo",
            "kingdom hearts", "skyrim", "gta", "among us", "pokemon", "pokeball", "forsaken",
            "twixxel", "steve", "noob", "in game", "game arrow", "brainrot 67",
        ],
    ),
    (
        "MOVIES & TV",
        &[
            "batman", "batarang", "spider man", "spider verse", "miles morales", "marvel",
            "iron man", "captain america", "venom", "joker", "dc", "spongebob", "patrick star",
            "krusty krab", "squarepants", "infinity gauntlet", "john wick",
            "battle for dream island",
        ],
    ),
    (
        "SPORTS",
        &[
            "messi", "ronaldo", "haaland", "lamine yamal", "world cup", "soccer", "football",
            "trophy", "air jordan", "nike",
        ],
    ),
    (
        "MEMES",
        &[
            "meme", "brainrot", "soyjak", "doge", "cheems", "moyai", "sahur", "chill guy",
            "monkey puppet", "sussy", "twerk", "siuuuu", "goose", "hamster", "shrek", "toothless",
            "du bist", "please speed",
        ],
    ),
    (
        "CUTE",
        &[
            "hello kitty", "sanrio", "pusheen", "kuromi", "cinnamoroll", "pochacco", "miffy",
            "kawaii", "cute", "cat", "paw", "hearts pixel", "bunny", "sheep", "raccoon",
        ],
    ),
    (
        "WEAPONS",
        &[
            "dagger", "sword", "knife", "gun", "pistol", "katana", "tanto", "needle", "scythe",
            "crosshair", "blade", "ninja", "weapon", "axe", "pickaxe",
        ],
    ),
    (
        "TECH",
        &[
            "matrix", "blue screen", "spacecraft", "artemis", "orion", "electric", "futuristic",
            "pixel", "laptop", "phone", "notebook",
        ],
    ),
];

/// The pack name reduced to lowercase words, space-joined and space-padded.
///
/// The padding is what makes a key match on a word boundary: searching for
/// `" cat "` in `" pixel tuxedo cat "` hits, and in `" concatenate "` does not.
fn normalized(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 2);
    out.push(' ');
    let mut pending_space = false;
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            if pending_space {
                out.push(' ');
                pending_space = false;
            }
            out.push(ch.to_ascii_lowercase());
        } else if !out.ends_with(' ') {
            pending_space = true;
        }
    }
    out.push(' ');
    out
}

/// Which shelf this pack belongs on.
pub fn classify(name: &str) -> &'static str {
    let haystack = normalized(name);
    for (category, keys) in RULES {
        for key in keys {
            if haystack.contains(&format!(" {key} ")) {
                return category;
            }
        }
    }
    "OTHER"
}

/// Maps a stored category onto one of ours, for anything that still reads
/// `pack.json`. Unknown values — including the two this replaced — are
/// re-derived rather than kept.
pub fn known(category: &str) -> Option<&'static str> {
    CATEGORIES.into_iter().find(|known| *known == category)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_match_only_on_boundaries() {
        // The bug this shape of matching exists to prevent: "f-luffy".
        assert_eq!(classify("Cute Fluffy Cat Paw Arrow & Pointing Paw"), "CUTE");
        assert_eq!(classify("One Piece Luffy & Arrow Animated"), "ANIME");
        // Punctuation is a boundary, so a hyphenated name still matches.
        assert_eq!(classify("Marvel Spider-Man & Venom"), "MOVIES & TV");
        assert_eq!(classify("Solo Leveling Sung Jin-Woo Dark Flames"), "ANIME");
    }

    #[test]
    fn a_franchise_beats_the_generic_noun_inside_it() {
        assert_eq!(classify("Halo Energy Sword"), "GAMING");
        assert_eq!(classify("Demon Slayer Water Breathing Sword"), "ANIME");
        assert_eq!(classify("Minecraft Enchanted Diamond Sword Animat"), "GAMING");
        assert_eq!(classify("Blue Lock Rin Itoshi & Soccer Ball"), "ANIME");
        assert_eq!(classify("Roblox Forsaken John Doe Crosshair"), "GAMING");
        // And with no franchise in the name, the generic noun is the answer.
        assert_eq!(classify("Japanese Tanto Dagger & Black Ninja"), "WEAPONS");
        assert_eq!(classify("Supreme Gun & Money"), "WEAPONS");
    }

    #[test]
    fn every_rule_names_a_category_the_ui_offers() {
        for (category, _) in RULES {
            assert!(
                CATEGORIES.contains(&category),
                "{category} is matched but is not on the shelf list, so nothing filed \
                 there would ever be reachable"
            );
        }
        assert!(CATEGORIES.contains(&"OTHER"), "the fallback must be reachable");
    }

    /// A name with nothing to go on must still land somewhere.
    #[test]
    fn anything_unrecognised_falls_through_to_other() {
        assert_eq!(classify("9892a"), "OTHER");
        assert_eq!(classify("WpppZUoU"), "OTHER");
        assert_eq!(classify(""), "OTHER");
        assert_eq!(classify("   "), "OTHER");
    }

    #[test]
    fn stored_categories_are_recognised_or_rejected() {
        assert_eq!(known("ANIME"), Some("ANIME"));
        // The two this replaced are not carried forward.
        assert_eq!(known("OPTIMAL CURSED"), None);
        assert_eq!(known("MINIMAL CURSED"), None);
    }

    /// The shipped catalog must actually spread across the shelves.
    ///
    /// A classifier that compiles and files everything under OTHER is worse than
    /// no categories at all — the filter row would be there, and using it would
    /// empty the grid. This reads the real labels out of `bundled.rs` so it fails
    /// when a pack is added that nothing knows what to do with.
    #[test]
    fn the_shipped_catalog_is_spread_across_the_shelves() {
        let source = include_str!("../bundled.rs");
        let labels: Vec<&str> = source
            .match_indices("label: \"")
            .filter_map(|(at, _)| {
                let rest = &source[at + "label: \"".len()..];
                rest.find('"').map(|end| &rest[..end])
            })
            .collect();
        assert!(labels.len() > 100, "only found {} labels", labels.len());

        let mut other = 0usize;
        let mut used: Vec<&'static str> = Vec::new();
        for label in &labels {
            let category = classify(label);
            if category == "OTHER" {
                other += 1;
            }
            if !used.contains(&category) {
                used.push(category);
            }
        }
        // Every named shelf has something on it, so no chip is a dead end.
        for category in CATEGORIES {
            assert!(used.contains(&category), "{category} has no packs on it");
        }
        // And the fallback is not where most of the catalog ended up.
        assert!(
            other * 4 < labels.len(),
            "{other} of {} packs are uncategorised",
            labels.len()
        );
    }
}
