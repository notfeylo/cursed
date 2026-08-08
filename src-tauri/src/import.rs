//! Importing a folder of cursors the user already has.
//!
//! CursorForge ships original artwork only, but people collect cursors from all
//! over — downloaded packs, things they drew, files rescued from an old machine.
//! This turns any folder of them into first-class entries in the catalog.
//!
//! Nothing imported here is redistributed. The files land in the user's own
//! `%APPDATA%\CursorForge\imported` and are never bundled into the installer or
//! the repository, which keeps someone else's artwork exactly where it belongs:
//! on the machine of the person who obtained it.
//!
//! The naming convention most download sites use — `Name--cursor--Source.png`
//! and `Name--pointer--Source.png` — is recognised and paired into one entry, so
//! a folder of forty files becomes twenty usable cursors rather than forty
//! half-cursors.

use crate::build::cur_reader;
use crate::build::hotspot;
use crate::build::pipeline::{self, Finish, Source};
use crate::cursor::roles::Role;
use crate::error::{AppError, AppResult};
use crate::paths;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Extensions we know how to turn into a cursor.
const CURSOR_FILES: [&str; 2] = ["cur", "ani"];
const IMAGE_FILES: [&str; 5] = ["png", "jpg", "jpeg", "webp", "bmp"];

/// Guards against pointing the importer at something enormous by accident —
/// a home directory, say, rather than a folder of cursors.
const MAX_FILES_SCANNED: usize = 5_000;
const MAX_PACKS_PER_IMPORT: usize = 500;
const MAX_ZIP_ENTRIES: usize = 400;
const MAX_ZIP_UNCOMPRESSED: u64 = 128 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportedPack {
    pub id: String,
    pub name: String,
    pub category: String,
    /// Where the file for each role lives, relative to this pack's directory.
    pub roles: BTreeMap<Role, String>,
    pub animated: bool,
    /// Whatever the source named itself, kept for attribution in the UI.
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub imported: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    pub imported: usize,
    pub skipped: usize,
    /// Human-readable reasons, capped — a wall of errors helps nobody.
    pub problems: Vec<String>,
    pub names: Vec<String>,
}

fn imported_dir() -> AppResult<PathBuf> {
    let dir = paths::root()?.join("imported");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Splits `Name--cursor--Source.png` into its parts.
///
/// Returns `(display name, role hint, source)`. Files that do not follow the
/// convention keep their whole stem as the name and get no role hint, which is
/// the right outcome for a folder of loose `arrow.cur` / `hand.cur` files too.
fn parse_name(stem: &str) -> (String, Option<Role>, String) {
    let parts: Vec<&str> = stem.split("--").collect();
    if parts.len() >= 2 {
        let name = parts[0].trim().to_owned();
        let source = parts.get(2).map(|s| s.trim().to_owned()).unwrap_or_default();
        let role = role_from_hint(parts[1]);
        return (name, role, source);
    }
    (stem.trim().to_owned(), role_from_hint(stem), String::new())
}

/// Maps the words download sites and hand-made packs use onto real roles.
fn role_from_hint(text: &str) -> Option<Role> {
    let t = text.to_ascii_lowercase();
    // Order matters: "pointer" is checked before "point" would match anything.
    if t.contains("pointer") || t.contains("link") || t.contains("hand") {
        Some(Role::Hand)
    } else if t.contains("cursor") || t.contains("arrow") || t.contains("normal") {
        Some(Role::Arrow)
    } else if t.contains("text") || t.contains("beam") || t.contains("ibeam") {
        Some(Role::IBeam)
    } else if t.contains("busy") || t.contains("wait") || t.contains("load") {
        Some(Role::Wait)
    } else if t.contains("working") || t.contains("progress") || t.contains("appstart") {
        Some(Role::AppStarting)
    } else if t.contains("precision") || t.contains("cross") {
        Some(Role::Crosshair)
    } else if t.contains("help") {
        Some(Role::Help)
    } else if t.contains("unavail") || t.contains("no") && t.len() <= 4 {
        Some(Role::No)
    } else if t.contains("move") || t.contains("sizeall") {
        Some(Role::SizeAll)
    } else {
        None
    }
}

/// Best-effort category from the name and whether the file really animates.
///
/// Deliberately coarse. Getting this exactly right is impossible and not worth
/// trying; getting it roughly right means a folder of forty imports is
/// browsable instead of being one undifferentiated heap.
///
/// `animated` comes from the file, not the name. Download sites label a pack
/// "Animated" and then ship a static PNG preview alongside the `.ani`, so
/// trusting the name puts still images in the animated category and makes the
/// filter useless.
fn categorise(name: &str, animated: bool) -> &'static str {
    let n = name.to_ascii_lowercase();
    const GAMING: [&str; 13] = [
        "game", "knight", "sword", "pickaxe", "craft", "blox", "valorant", "csgo", "fortnite",
        "hornet", "silksong", "kunai", "gun",
    ];
    const ANIME: [&str; 10] = [
        "anime", "naruto", "kaisen", "sukuna", "akatsuki", "kuromi", "sanrio", "kitty", "manga",
        "chibi",
    ];
    const VEHICLES: [&str; 8] = ["bmw", "toyota", "supra", "car", "racing", "m4", "m5", "moto"];
    const CHARACTERS: [&str; 9] = [
        "batman", "marvel", "spider", "venom", "hero", "meme", "ronaldo", "skull", "dagger",
    ];
    const NEON: [&str; 6] = ["neon", "glow", "electric", "rgb", "laser", "plasma"];
    const RETRO: [&str; 6] = ["pixel", "8-bit", "8bit", "retro", "matrix", "arcade"];

    let has = |set: &[&str]| set.iter().any(|k| n.contains(k));

    if animated {
        "ANIMATED"
    } else if has(&ANIME) {
        "ANIME"
    } else if has(&VEHICLES) {
        "VEHICLES"
    } else if has(&GAMING) {
        "GAMING"
    } else if has(&CHARACTERS) {
        "CHARACTERS"
    } else if has(&NEON) {
        "NEON"
    } else if has(&RETRO) {
        "RETRO"
    } else {
        "IMPORTED"
    }
}

fn extension_of(path: &Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default()
}

/// One candidate file found during the scan.
struct Candidate {
    path: PathBuf,
    name: String,
    role: Option<Role>,
    source: String,
}

/// Walks a folder, one level of subdirectories deep, collecting candidates and
/// expanding any zips into a scratch directory.
fn collect(folder: &Path, scratch: &Path, problems: &mut Vec<String>) -> AppResult<Vec<Candidate>> {
    let mut out = Vec::new();
    let mut seen = 0usize;
    let mut stack = vec![folder.to_path_buf()];
    let mut depth_guard = 0usize;

    while let Some(dir) = stack.pop() {
        depth_guard += 1;
        if depth_guard > 64 {
            break;
        }
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            if seen >= MAX_FILES_SCANNED {
                problems.push(format!("stopped after {MAX_FILES_SCANNED} files"));
                return Ok(out);
            }
            seen += 1;
            let path = entry.path();

            if path.is_dir() {
                stack.push(path);
                continue;
            }

            let ext = extension_of(&path);
            if ext == "zip" {
                match expand_zip(&path, scratch) {
                    Ok(dir) => stack.push(dir),
                    Err(e) => problems.push(format!(
                        "{}: {e}",
                        path.file_name().unwrap_or_default().to_string_lossy()
                    )),
                }
                continue;
            }

            if !CURSOR_FILES.contains(&ext.as_str()) && !IMAGE_FILES.contains(&ext.as_str()) {
                continue;
            }

            let stem = path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            let (name, role, source) = parse_name(&stem);
            if name.is_empty() {
                continue;
            }
            out.push(Candidate {
                path,
                name,
                role,
                source,
            });
        }
    }
    Ok(out)
}

/// Extracts a zip into a fresh scratch directory.
///
/// Same rules as `.cfpack` import: no traversal, bounded entries and size, and
/// only file types we would have accepted loose.
fn expand_zip(archive: &Path, scratch: &Path) -> AppResult<PathBuf> {
    let file = std::fs::File::open(archive).map_err(|_| AppError::invalid("could not open"))?;
    let mut zip = zip::ZipArchive::new(file).map_err(|_| AppError::invalid("not a readable zip"))?;
    if zip.len() > MAX_ZIP_ENTRIES {
        return Err(AppError::invalid("too many files in the archive"));
    }

    let stem = archive
        .file_stem()
        .map(|s| paths::slugify(&s.to_string_lossy()))
        .unwrap_or_else(|| "archive".into());
    let target = scratch.join(stem);
    std::fs::create_dir_all(&target)?;

    let mut written = 0u64;
    for index in 0..zip.len() {
        let mut entry = zip
            .by_index(index)
            .map_err(|_| AppError::invalid("the archive is damaged"))?;
        if entry.is_dir() {
            continue;
        }
        let raw = entry.name().to_owned();
        // The zip decides its own entry names, so it does not get to decide
        // where they land. Only the final component is used, flattened.
        let Some(base) = Path::new(&raw).file_name().map(|n| n.to_string_lossy().into_owned())
        else {
            continue;
        };
        if paths::validate_relative(&base).is_err() {
            continue;
        }
        let ext = extension_of(Path::new(&base));
        if !CURSOR_FILES.contains(&ext.as_str()) && !IMAGE_FILES.contains(&ext.as_str()) {
            continue;
        }

        written = written.saturating_add(entry.size());
        if written > MAX_ZIP_UNCOMPRESSED {
            return Err(AppError::invalid("the archive expands to more than we will unpack"));
        }

        let mut bytes = Vec::new();
        std::io::Read::read_to_end(&mut entry, &mut bytes)?;
        std::fs::write(target.join(&base), &bytes)?;
    }
    Ok(target)
}

/// Turns one candidate file into a cursor inside `dir`, returning its filename.
fn install_file(candidate: &Candidate, dir: &Path) -> AppResult<(String, bool)> {
    let ext = extension_of(&candidate.path);

    if CURSOR_FILES.contains(&ext.as_str()) {
        // Already a cursor: keep the author's own file and hotspot untouched,
        // but only after Windows confirms it can load it.
        crate::cursor::engine::verify_loadable(&candidate.path)?;
        let animated = ext == "ani";
        let file_name = format!("{}.{ext}", if animated { "role-ani" } else { "role" });
        std::fs::copy(&candidate.path, dir.join(&file_name))?;
        return Ok((file_name, animated));
    }

    // An image: build a real multi-resolution cursor from it.
    let bytes = std::fs::read(&candidate.path)?;
    let source = pipeline::decode(bytes)?;
    let finish = Finish {
        tint: None,
        opacity: 1.0,
        outline: false,
    };

    match &source {
        Source::Animated(frames) => {
            let master = source.first()?.clone();
            let spot = hotspot::compute(&master, hotspot::suggest(&master));
            let size = pipeline::nearest_size(64);
            let built = pipeline::build_ani(
                frames,
                spot,
                &finish,
                size,
                1.0,
                &crate::build::ani_writer::AniMetadata {
                    name: Some(candidate.name.clone()),
                    author: None,
                },
            )?;
            let file_name = "role-ani.ani".to_owned();
            std::fs::write(dir.join(&file_name), &built)?;
            crate::cursor::engine::verify_loadable(&dir.join(&file_name))?;
            Ok((file_name, true))
        }
        Source::Static(bitmap) => {
            let master = pipeline::prepare_master(bitmap)?;
            let spot = hotspot::compute(&master, hotspot::suggest(&master));
            // Only the sizes the source can actually fill. Upscaling a 128 px
            // download to 256 px costs half a megabyte per cursor and buys blur.
            let sizes = pipeline::sizes_for_source(bitmap.width, bitmap.height);
            let built = pipeline::build_cur(&master, spot, &finish, &sizes)?;
            let file_name = "role.cur".to_owned();
            std::fs::write(dir.join(&file_name), &built)?;
            crate::cursor::engine::verify_loadable(&dir.join(&file_name))?;
            Ok((file_name, false))
        }
    }
}

/// Imports every cursor found under `folder`.
pub fn import_folder(folder: &Path) -> AppResult<ImportReport> {
    if !folder.is_dir() {
        return Err(AppError::invalid("that is not a folder"));
    }

    let scratch = paths::root()?.join("import-scratch");
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch)?;

    let mut problems: Vec<String> = Vec::new();
    let candidates = collect(folder, &scratch, &mut problems)?;

    // Group by display name so `Name--cursor` and `Name--pointer` become one
    // pack with two roles rather than two packs with one role each.
    let mut grouped: BTreeMap<String, Vec<Candidate>> = BTreeMap::new();
    for candidate in candidates {
        grouped.entry(candidate.name.clone()).or_default().push(candidate);
    }

    let root = imported_dir()?;
    let mut report = ImportReport {
        imported: 0,
        skipped: 0,
        problems: Vec::new(),
        names: Vec::new(),
    };

    for (name, files) in grouped.into_iter().take(MAX_PACKS_PER_IMPORT) {
        let slug = paths::slugify(&name);
        if slug.is_empty() {
            continue;
        }
        let dir = root.join(&slug);
        let _ = std::fs::remove_dir_all(&dir);
        if std::fs::create_dir_all(&dir).is_err() {
            report.skipped += 1;
            continue;
        }

        let mut roles: BTreeMap<Role, String> = BTreeMap::new();
        let mut animated = false;
        let mut source = String::new();

        for candidate in &files {
            // A file with no role hint becomes the arrow, which is what a lone
            // `something.cur` in a folder almost always means.
            let role = candidate.role.unwrap_or(Role::Arrow);
            if roles.contains_key(&role) {
                continue;
            }
            match install_file(candidate, &dir) {
                Ok((file_name, is_animated)) => {
                    // Two roles in one pack would collide on the shared
                    // filename, so give each role its own copy.
                    let unique = format!("{}-{file_name}", role.file_stem().to_ascii_lowercase());
                    let _ = std::fs::rename(dir.join(&file_name), dir.join(&unique));
                    roles.insert(role, unique);
                    animated |= is_animated;
                    if source.is_empty() {
                        source = candidate.source.clone();
                    }
                }
                Err(e) => {
                    if problems.len() < 12 {
                        problems.push(format!("{name}: {e}"));
                    }
                }
            }
        }

        if roles.is_empty() {
            let _ = std::fs::remove_dir_all(&dir);
            report.skipped += 1;
            continue;
        }

        let pack = ImportedPack {
            id: format!("user:{slug}"),
            name: name.chars().take(40).collect(),
            category: categorise(&name, animated).to_owned(),
            roles,
            animated,
            source: source.chars().filter(|c| !c.is_control()).take(40).collect(),
            imported: crate::util::iso_now(),
        };
        std::fs::write(dir.join("pack.json"), serde_json::to_string_pretty(&pack)?)?;

        report.imported += 1;
        if report.names.len() < 40 {
            report.names.push(pack.name.clone());
        }
    }

    let _ = std::fs::remove_dir_all(&scratch);
    problems.truncate(12);
    report.problems = problems;
    Ok(report)
}

/// Every pack the user has imported.
pub fn list() -> AppResult<Vec<ImportedPack>> {
    let root = imported_dir()?;
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(&root) else {
        return Ok(out);
    };
    for entry in entries.filter_map(Result::ok) {
        let manifest = entry.path().join("pack.json");
        let Ok(text) = std::fs::read_to_string(&manifest) else {
            continue;
        };
        if let Ok(pack) = serde_json::from_str::<ImportedPack>(crate::util::strip_bom(&text)) {
            out.push(pack);
        }
    }
    out.sort_by_key(|pack| pack.name.to_lowercase());
    Ok(out)
}

pub fn get(id: &str) -> AppResult<ImportedPack> {
    list()?
        .into_iter()
        .find(|p| p.id == id)
        .ok_or(AppError::UnknownPack)
}

/// Absolute paths for an imported pack's roles.
pub fn role_files(pack: &ImportedPack) -> AppResult<BTreeMap<Role, PathBuf>> {
    let slug = pack.id.strip_prefix("user:").unwrap_or(&pack.id);
    let dir = imported_dir()?.join(paths::validate_relative(slug)?);
    let mut out = BTreeMap::new();
    for (role, file) in &pack.roles {
        let path = dir.join(paths::validate_relative(file)?);
        if path.exists() {
            out.insert(*role, path);
        }
    }
    if out.is_empty() {
        return Err(AppError::invalid("that pack's files are missing"));
    }
    Ok(out)
}

/// A tile preview, rendered from the pack's own arrow.
pub fn preview(pack: &ImportedPack) -> AppResult<String> {
    let files = role_files(pack)?;
    let path = files
        .get(&Role::Arrow)
        .or_else(|| files.values().next())
        .ok_or_else(|| AppError::invalid("nothing to preview"))?;
    cur_reader::read(path, 64)?.to_png_data_uri()
}

pub fn remove(id: &str) -> AppResult<()> {
    let slug = id.strip_prefix("user:").unwrap_or(id);
    let dir = imported_dir()?.join(paths::validate_relative(slug)?);
    if dir.exists() {
        std::fs::remove_dir_all(&dir)?;
    }
    Ok(())
}

pub fn remove_all() -> AppResult<()> {
    let dir = imported_dir()?;
    if dir.exists() {
        std::fs::remove_dir_all(&dir)?;
    }
    std::fs::create_dir_all(&dir)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_common_download_naming_convention_is_understood() {
        let (name, role, source) = parse_name("Batman & Batarang--cursor--SweezyCursors");
        assert_eq!(name, "Batman & Batarang");
        assert_eq!(role, Some(Role::Arrow));
        assert_eq!(source, "SweezyCursors");

        let (name, role, _) = parse_name("Batman & Batarang--pointer--SweezyCursors");
        assert_eq!(name, "Batman & Batarang");
        assert_eq!(role, Some(Role::Hand), "pointer is the link-select role");
    }

    #[test]
    fn a_pair_of_files_groups_under_one_name() {
        let a = parse_name("Some Pack--cursor--Site").0;
        let b = parse_name("Some Pack--pointer--Site").0;
        assert_eq!(a, b, "both halves must land in the same pack");
    }

    #[test]
    fn loose_files_still_get_a_sensible_role() {
        assert_eq!(parse_name("arrow").1, Some(Role::Arrow));
        assert_eq!(parse_name("hand").1, Some(Role::Hand));
        assert_eq!(parse_name("wait").1, Some(Role::Wait));
        assert_eq!(parse_name("ibeam").1, Some(Role::IBeam));
        // Something unrecognised keeps its name and is treated as the arrow
        // later, which is what a lone file in a folder almost always is.
        assert_eq!(parse_name("my-cool-thing").1, None);
    }

    #[test]
    fn categories_are_roughly_right_rather_than_everything_in_one_heap() {
        assert_eq!(categorise("Minecraft Enchanted Sword", true), "ANIMATED");
        assert_eq!(
            categorise("Minecraft Enchanted Sword Animated", false),
            "GAMING",
            "a static file labelled Animated is not animated"
        );
        assert_eq!(categorise("Jujutsu Kaisen Sukuna Flame", false), "ANIME");
        assert_eq!(categorise("Hollow Knight & Game Arrow", false), "GAMING");
        assert_eq!(categorise("Grey BMW M5", false), "VEHICLES");
        assert_eq!(categorise("Pixel Racing", false), "VEHICLES");
        assert_eq!(categorise("Neon Glow Thing", false), "NEON");
        assert_eq!(categorise("something plain", false), "IMPORTED");
        assert_eq!(categorise("Batman & Batarang", false), "CHARACTERS");
    }

    #[test]
    fn importing_something_that_is_not_a_folder_is_refused() {
        assert!(import_folder(Path::new(r"C:\nope\not-a-folder")).is_err());
    }

    #[test]
    fn a_zip_entry_cannot_escape_the_scratch_directory() {
        // Entry names are flattened to their final component before use, so a
        // traversal path cannot place a file outside the target.
        for evil in ["../../evil.cur", r"..\..\evil.cur", "/abs/evil.cur"] {
            let base = Path::new(evil).file_name().unwrap().to_string_lossy().into_owned();
            assert_eq!(base, "evil.cur");
            assert!(paths::validate_relative(&base).is_ok());
        }
    }
}
