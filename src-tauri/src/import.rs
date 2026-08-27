//! Importing a folder of cursors the user already has.
//!
//! Cursed ships original artwork only, but people collect cursors from all
//! over — downloaded packs, things they drew, files rescued from an old machine.
//! This turns any folder of them into first-class entries in the catalog.
//!
//! Nothing imported here is redistributed. The files land in the user's own
//! `%APPDATA%\Cursed\imported` and are never bundled into the installer or
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
/// Everything the decoder can already read. GIF and APNG become real animated
/// cursors; the rest become still ones. `ico` is here because a downloaded
/// "cursor" is very often an icon file wearing the wrong extension.
///
/// `tiff` sits beside `tif` because both spellings are in the wild and a folder
/// import that silently skips half of them is worse than one that refuses them
/// all. Nothing here decides what a file *is* — that is `pipeline::sniff_input`,
/// from the bytes — this only decides what is worth opening.
const IMAGE_FILES: [&str; 10] =
    ["png", "jpg", "jpeg", "webp", "bmp", "gif", "apng", "ico", "tif", "tiff"];
/// Read for metadata only — never executed, never installed.
const META_FILES: [&str; 2] = ["inf", "txt"];

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

/* ── complete cursor schemes ────────────────────────────────────
   A downloaded scheme is a folder of files named for the seventeen Windows
   pointer roles — `Normal.cur`, `Wait.ani`, `NSResize.cur` — usually beside an
   `install.inf`. Treated as loose files, each one would become its own pack
   named "Wait" or "Help", a 47-file set would explode into 47 single-role
   entries, and two schemes that both ship a `Help` would collide into one.

   So a subdirectory is imported as exactly one pack. That is also the right
   answer for the folders holding a single cursor at several resolutions.
   ──────────────────────────────────────────────────────────── */

/// How strongly a filename claims a role. The highest claim wins, which is how
/// `Normal.cur` beats `Normal-lefties.cur` for the arrow.
type Claim = i32;

/// Filenames that name no Windows role, and must not be guessed at.
///
/// These are real cursors — drag-and-drop feedback, zoom, cell select — but
/// Windows has no scheme slot for them. Letting them fall through to "probably
/// the arrow" would overwrite a scheme's actual arrow with its zoom-in icon.
fn names_no_windows_role(base: &str) -> bool {
    const UNMAPPED: [&str; 14] = [
        "alias", "cell", "copy", "context-menu", "contextmenu", "vertical-text", "verticaltext",
        "zoom-in", "zoom-out", "zoomin", "zoomout", "pirate", "draft", "all-scroll",
    ];
    base.starts_with("dnd-")
        || base.ends_with("_mask")
        || base.ends_with("-mask")
        || UNMAPPED.contains(&base)
}

/// Maps one filename stem onto a role, with a confidence.
///
/// Returns `None` both for names that mean nothing to us and for names that
/// mean something Windows cannot express — the caller skips those rather than
/// defaulting them to the arrow.
fn role_from_filename(stem: &str) -> Option<(Role, Claim)> {
    let lower = stem.trim().to_ascii_lowercase();
    let mut base = lower.as_str();
    let mut claim: Claim = 0;

    // Resolution suffixes: `_32`, `_96`, `_32-48-64`, `_72-96-128`, `_256`.
    // A file carrying several resolutions is the better master, so it outranks
    // both a single-size file and one with no suffix at all.
    if let Some(cut) = base.rfind('_') {
        let tail = &base[cut + 1..];
        if !tail.is_empty() && tail.chars().all(|c| c.is_ascii_digit() || c == '-') {
            claim += if tail.contains('-') { 6 } else { 2 };
            base = &base[..cut];
        } else {
            claim += 4;
        }
    } else {
        claim += 4;
    }

    let mut base = base.trim().to_owned();

    // Left-handed variants ship alongside the right-handed ones. Both are
    // valid; the right-handed file is the one to prefer.
    for marker in ["-lefties", "_lefties", " lefties", "-left-handed"] {
        if let Some(cut) = base.find(marker) {
            base.replace_range(cut..cut + marker.len(), "");
            claim -= 30;
        }
    }

    // `Move_1`, `NSResize_2`, `Link-hand-02` — alternates for the same role.
    while let Some(cut) = base.rfind(['-', '_']) {
        let tail = &base[cut + 1..];
        if !tail.is_empty() && tail.chars().all(|c| c.is_ascii_digit()) {
            base.truncate(cut);
            claim -= 20;
        } else {
            break;
        }
    }
    if base.contains("-alt") || base.contains("alternate-") {
        claim -= 25;
    }

    let base = base.trim().trim_matches(['-', '_', ' ']).to_owned();
    if base.is_empty() || names_no_windows_role(&base) {
        return None;
    }

    // Exact names first. These are what a real scheme uses, and an exact hit
    // must beat a substring hit somewhere else in a long filename.
    let exact = match base.as_str() {
        "normal" | "arrow" | "default" | "standard" | "pointer-normal" => Some(Role::Arrow),
        "help" | "helpselect" | "whatsthis" => Some(Role::Help),
        "appstarting" | "working" | "workinginbackground" | "progress" => Some(Role::AppStarting),
        "wait" | "busy" | "loading" => Some(Role::Wait),
        "cross" | "crosshair" | "precision" | "precisionselect" => Some(Role::Crosshair),
        "ibeam" | "text" | "beam" | "textselect" => Some(Role::IBeam),
        "nwpen" | "handwriting" | "pen" | "pencil" => Some(Role::NWPen),
        "no" | "notallowed" | "unavailable" | "nodrop" | "forbidden" => Some(Role::No),
        "sizens" | "nsresize" | "ns-resize" | "verticalresize" => Some(Role::SizeNS),
        "sizewe" | "ewresize" | "ew-resize" | "horizontalresize" => Some(Role::SizeWE),
        "sizenwse" | "nwresize" | "nwse-resize" | "seresize" => Some(Role::SizeNWSE),
        "sizenesw" | "neresize" | "nesw-resize" | "swresize" => Some(Role::SizeNESW),
        "sizeall" | "move" | "scroll" => Some(Role::SizeAll),
        "uparrow" | "up" | "alternate" | "alternateselect" => Some(Role::UpArrow),
        "hand" | "link" | "pointer" | "linkselect" | "handpointing" => Some(Role::Hand),
        "pin" | "location" => Some(Role::Pin),
        "person" | "user" => Some(Role::Person),
        _ => None,
    };
    if let Some(role) = exact {
        return Some((role, claim + 100));
    }

    // Then a looser read, for names like `Link-hand-Solstheim`.
    role_from_hint(&base).map(|role| (role, claim + 50))
}

/// What an `install.inf` says: the scheme's own name, and its role mapping.
///
/// The `.inf` is authoritative — it is what Windows itself would act on — so it
/// beats guessing from filenames. It is parsed as text and never executed.
fn parse_inf(text: &str) -> (Option<String>, Vec<(String, String)>) {
    let mut name = None;
    let mut pairs = Vec::new();

    for line in text.lines() {
        let line = line.trim();
        if !line.to_ascii_uppercase().starts_with("HKCU") {
            continue;
        }
        // `HKCU,"Control Panel\Cursors","Arrow",,"%25%\Cursors\Arrow.cur"`
        let quoted: Vec<&str> = line
            .split('"')
            .skip(1)
            .step_by(2)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect();
        let [key, value, rest @ ..] = quoted.as_slice() else {
            continue;
        };
        if !key.to_ascii_lowercase().contains("control panel\\cursors") {
            continue;
        }
        let Some(target) = rest.first() else { continue };

        if value.eq_ignore_ascii_case("(default)") {
            let cleaned: String = target.chars().filter(|c| !c.is_control()).collect();
            if !cleaned.is_empty() {
                name = Some(cleaned);
            }
            continue;
        }
        // Only the final component; the `.inf` does not get to choose a path.
        if let Some(file) = Path::new(&target.replace('\\', "/"))
            .file_name()
            .map(|f| f.to_string_lossy().into_owned())
        {
            pairs.push(((*value).to_owned(), file));
        }
    }
    (name, pairs)
}

/// Download sites append a hash to the folder they hand you.
fn strip_hash_suffix(raw: &str) -> &str {
    match raw.rsplit_once('-') {
        Some((head, tail))
            if tail.len() >= 6 && tail.chars().all(|c| c.is_ascii_hexdigit()) && !head.is_empty() =>
        {
            head
        }
        _ => raw,
    }
}

/// `Skyrim-Set-2-563e85ef` becomes `Skyrim Set 2`.
fn pretty_folder_name(raw: &str) -> String {
    let stem = strip_hash_suffix(raw);

    // A folder holding one cursor is often named for the role it fills, and
    // the role's own spelling reads better than a title-cased slug.
    if let Some((role, _)) = role_from_filename(stem) {
        if stem.replace(['-', '_', ' '], "").eq_ignore_ascii_case(role.registry_value()) {
            return role.registry_value().to_owned();
        }
    }

    let mut words: Vec<String> = stem
        .split(['-', '_', ' '])
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut chars = w.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect();

    // Sites suffix the folder with the role it was filed under. Once it is one
    // pack among several words, that suffix is noise.
    if words.len() > 1 {
        let last = words[words.len() - 1].to_ascii_lowercase();
        if ["normal", "cursor", "pointer", "link", "set"].contains(&last.as_str()) {
            words.pop();
        }
    }

    words.join(" ")
}

/// Credit the author when the pack says who they are.
fn author_from_readme(text: &str) -> Option<String> {
    for line in text.lines().take(40) {
        let line = line.trim();
        // Compared on a lowercased copy rather than by slicing the original:
        // `line[..prefix.len()]` splits any line that opens with a multi-byte
        // character, and readme files are full of emoji. Byte offsets still
        // line up because ASCII lowercasing never changes a byte's width.
        let lower = line.to_ascii_lowercase();
        for prefix in ["author:", "by:", "created by:", "artist:"] {
            if lower.starts_with(prefix) {
                let Some(rest) = line.get(prefix.len()..) else {
                    continue;
                };
                let who: String = rest
                    .trim()
                    .chars()
                    .filter(|c| !c.is_control())
                    .take(40)
                    .collect();
                if !who.is_empty() {
                    return Some(who);
                }
            }
        }
    }
    None
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
fn categorise(_name: &str, _animated: bool) -> &'static str {
    // Everything imported lands in OPTIMAL CURSED for now. MINIMAL CURSED
    // exists alongside it and is deliberately empty until there is something to
    // put there — an empty, named shelf is clearer than guessing which cursors
    // belong on it.
    "OPTIMAL CURSED"
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

/// One subdirectory, resolved down to a single pack.
struct SchemeCandidate {
    name: String,
    author: String,
    /// The best file found for each role, and the claim that won it.
    picks: BTreeMap<Role, (PathBuf, Claim)>,
}

/// Every cursor and metadata file inside one directory.
fn files_within(dir: &Path, budget: &mut usize) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    let mut guard = 0usize;

    while let Some(next) = stack.pop() {
        guard += 1;
        if guard > 64 || *budget == 0 {
            break;
        }
        let Ok(entries) = std::fs::read_dir(&next) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            if *budget == 0 {
                break;
            }
            *budget -= 1;
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

/// Turns one directory into a single pack, or `None` if it holds no cursor.
///
/// The `install.inf`, when there is one, is authoritative: it is the mapping
/// Windows itself would act on, so it outranks anything inferred from a
/// filename. Everything else falls back to reading the name, and files naming a
/// role Windows cannot express are skipped rather than guessed at.
fn resolve_dir(dir: &Path, budget: &mut usize) -> Option<SchemeCandidate> {
    let files = files_within(dir, budget);

    let mut inf_roles: BTreeMap<String, Role> = BTreeMap::new();
    let mut scheme_name: Option<String> = None;
    let mut author = String::new();

    for path in &files {
        match extension_of(path).as_str() {
            "inf" => {
                let Ok(text) = std::fs::read_to_string(path) else {
                    continue;
                };
                let (name, pairs) = parse_inf(crate::util::strip_bom(&text));
                if let Some(name) = name {
                    scheme_name = Some(name);
                }
                for (value, file) in pairs {
                    if let Some((role, _)) = role_from_filename(&value) {
                        inf_roles.insert(file.to_ascii_lowercase(), role);
                    }
                }
            }
            "txt" if author.is_empty() => {
                if let Ok(text) = std::fs::read_to_string(path) {
                    if let Some(who) = author_from_readme(crate::util::strip_bom(&text)) {
                        author = who;
                    }
                }
            }
            _ => {}
        }
    }

    let mut picks: BTreeMap<Role, (PathBuf, Claim)> = BTreeMap::new();
    for path in &files {
        let ext = extension_of(path);
        if !CURSOR_FILES.contains(&ext.as_str()) && !IMAGE_FILES.contains(&ext.as_str()) {
            continue;
        }
        let file_name = path
            .file_name()
            .map(|f| f.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();

        let claimed = match inf_roles.get(&file_name) {
            Some(role) => Some((*role, 200 + role_from_filename(&stem).map_or(0, |(_, c)| c))),
            None => role_from_filename(&stem),
        };
        let Some((role, claim)) = claimed else {
            continue;
        };
        match picks.get(&role) {
            Some((_, best)) if *best >= claim => {}
            _ => {
                picks.insert(role, (path.clone(), claim));
            }
        }
    }

    // A folder whose files are named for what they *depict* rather than for a
    // role — `Baby Zombie - Minecraft_32-48-64.ani` — names no role at all. It
    // is still a cursor the user downloaded and still wants, so the best file
    // becomes the arrow, exactly as a lone loose file would.
    if picks.is_empty() {
        let best = files
            .iter()
            .filter(|p| {
                let ext = extension_of(p);
                CURSOR_FILES.contains(&ext.as_str()) || IMAGE_FILES.contains(&ext.as_str())
            })
            .max_by_key(|p| {
                let stem = p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
                // Reuse the resolution ranking: multi-size beats plain beats
                // one fixed size.
                let lower = stem.to_ascii_lowercase();
                match lower.rsplit_once('_') {
                    Some((_, tail))
                        if !tail.is_empty()
                            && tail.chars().all(|c| c.is_ascii_digit() || c == '-') =>
                    {
                        if tail.contains('-') {
                            6
                        } else {
                            2
                        }
                    }
                    _ => 4,
                }
            })?;
        picks.insert(Role::Arrow, (best.clone(), 1));
    }

    let raw = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let name = scheme_name.unwrap_or_else(|| pretty_folder_name(&raw));
    if name.trim().is_empty() {
        return None;
    }

    Some(SchemeCandidate {
        name,
        author,
        picks,
    })
}

/// The loose files sitting directly in `folder`, paired by the `--` convention.
fn collect_loose(folder: &Path, budget: &mut usize) -> Vec<Candidate> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(folder) else {
        return out;
    };
    for entry in entries.filter_map(Result::ok) {
        if *budget == 0 {
            break;
        }
        *budget -= 1;
        let path = entry.path();
        if path.is_dir() {
            continue;
        }
        let ext = extension_of(&path);
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
        // Files that follow the `--` convention already carry a clean name.
        // Anything else is a raw download filename, so it gets the same tidying
        // a folder name gets: `sukuna-human-finger-f4e78762` is not a name to
        // show anyone.
        let name = if stem.contains("--") {
            name
        } else {
            let pretty = pretty_folder_name(&name);
            if pretty.is_empty() { name } else { pretty }
        };
        out.push(Candidate {
            path,
            name,
            role,
            source,
        });
    }
    out
}

/// Subdirectories to import, including archives expanded into scratch.
///
/// An archive sitting beside an already-extracted copy of itself is skipped.
/// Download sites hand you both, and importing each would produce two identical
/// packs from one download.
fn scheme_dirs(folder: &Path, scratch: &Path, problems: &mut Vec<String>) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    let mut archives: Vec<PathBuf> = Vec::new();
    let Ok(entries) = std::fs::read_dir(folder) else {
        return dirs;
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            dirs.push(path);
        } else if extension_of(&path) == "zip" {
            archives.push(path);
        }
    }

    let extracted: Vec<String> = dirs
        .iter()
        .filter_map(|d| d.file_name().map(|n| n.to_string_lossy().to_ascii_lowercase()))
        .collect();

    for archive in archives {
        let stem = archive
            .file_stem()
            .map(|s| s.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        if extracted.contains(&stem) {
            continue;
        }
        match expand_zip(&archive, scratch) {
            Ok(dir) => dirs.push(dir),
            Err(e) => problems.push(format!(
                "{}: {e}",
                archive.file_name().unwrap_or_default().to_string_lossy()
            )),
        }
    }

    dirs.sort();
    dirs
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
        // `.inf` and `.txt` come out too, but only ever to be read as text for
        // the scheme's name, role mapping and author. Neither is installed and
        // neither is executed.
        if !CURSOR_FILES.contains(&ext.as_str())
            && !IMAGE_FILES.contains(&ext.as_str())
            && !META_FILES.contains(&ext.as_str())
        {
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

/// How many resolutions a `.cur`/`.ico` declares, read from its directory.
///
/// The count is the only part of the header needed to decide whether the file is
/// worth rebuilding, and reading just it avoids decoding a file that is about to
/// be copied unchanged.
fn declared_entries(bytes: &[u8]) -> Option<usize> {
    if !crate::build::icon_reader::looks_like_an_icon(bytes) {
        return None;
    }
    Some(u16::from_le_bytes([*bytes.get(4)?, *bytes.get(5)?]) as usize)
}

/// Rebuilds a static cursor file onto our size ladder, or `None` to leave it be.
///
/// The author's hotspot travels with it as a fraction of the artwork, which is
/// the only form that survives being redrawn at ten sizes. The bitmap is used
/// exactly as decoded — no matte, no trim, no squaring — because an existing
/// cursor already carries its own alpha, and moving the artwork inside its
/// canvas would move the hotspot off the point the author chose.
fn rebuilt_onto_the_ladder(path: &Path) -> Option<Vec<u8>> {
    let bytes = std::fs::read(path).ok()?;
    let declared = declared_entries(&bytes)?;

    let icon = crate::build::icon_reader::decode_icon(&bytes).ok()?;
    let (w, h) = (icon.bitmap.width, icon.bitmap.height);
    if w != h || w == 0 {
        return None; // resizing a non-square cursor to a square one distorts it
    }

    // **Only a file with exactly one entry.**
    //
    // Anything with two or more has resolutions the author chose, and small
    // cursors are very often hand-tuned rather than downscaled — a 16 px arrow
    // drawn pixel by pixel beats any resample of the 32 px one. Rebuilding those
    // would trade the author's work for ours and could easily look worse.
    //
    // A single-entry file has nothing to lose: the one resolution it has is
    // regenerated at its own size (a resample at scale 1.0), and every other
    // rung is new.
    if declared != 1 {
        return None;
    }
    let sizes = pipeline::sizes_for_source(w, h);
    if sizes.len() <= 1 {
        return None;
    }

    let spot = crate::build::icon_reader::hotspot_fraction(&bytes).unwrap_or((0.0, 0.0));
    let finish = Finish {
        tint: None,
        opacity: 1.0,
        outline: false,
    };
    let built = pipeline::build_cur(&icon.bitmap, spot, &finish, &sizes).ok()?;
    log::info!(
        "{}: rebuilt a lone {w}px entry onto {} rungs",
        path.file_name().unwrap_or_default().to_string_lossy(),
        sizes.len()
    );
    Some(built)
}

/// Rebuilds single-entry cursors already sitting in the library.
///
/// Without this the improvement above only reaches packs imported after it
/// shipped, and a library of thirty-odd packs collected over months keeps its
/// ceiling until every one of them is imported again by hand. Nobody does that.
///
/// Runs once. The marker is written whatever happens, including when nothing
/// needed doing, because the alternative is walking the whole library on every
/// launch to discover the same nothing.
///
/// Every failure is skipped rather than propagated: this is an improvement to
/// files that already work, so the worst outcome it may cause is that they carry
/// on working exactly as they did.
pub fn upgrade_thin_ladders() {
    let Ok(root) = imported_dir() else { return };
    let marker = root.join(".ladders-v1");
    if marker.exists() {
        return;
    }

    let mut rebuilt = 0usize;
    let mut looked_at = 0usize;
    if let Ok(packs) = std::fs::read_dir(&root) {
        for pack in packs.flatten() {
            let Ok(files) = std::fs::read_dir(pack.path()) else { continue };
            for file in files.flatten() {
                let path = file.path();
                if extension_of(&path) != "cur" {
                    continue;
                }
                looked_at += 1;
                let Some(bytes) = rebuilt_onto_the_ladder(&path) else { continue };

                // Written beside the original and renamed over it, so a crash
                // mid-write cannot leave a half a cursor where a whole one was.
                let temp = path.with_extension("cur.rebuilding");
                if std::fs::write(&temp, &bytes).is_err() {
                    continue;
                }
                if crate::cursor::engine::verify_loadable(&temp).is_err() {
                    let _ = std::fs::remove_file(&temp);
                    continue;
                }
                if std::fs::rename(&temp, &path).is_ok() {
                    rebuilt += 1;
                } else {
                    let _ = std::fs::remove_file(&temp);
                }
            }
        }
    }

    let _ = std::fs::write(&marker, b"rebuilt single-entry cursors onto the size ladder
");
    if rebuilt > 0 {
        log::info!("imported library: rebuilt {rebuilt} of {looked_at} cursors onto the ladder");
    }
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

        // A static cursor with a thin ladder is worth rebuilding onto ours.
        //
        // Most `.cur` files in circulation carry **one** entry, almost always
        // 32 px, because that is what cursor editors have always written. Copied
        // in as-is, such a file is exact at 32 and stretched by the shell at
        // every other size — and the shell's stretch is bilinear, unpremultiplied
        // and not gamma corrected. Measured on one machine's library, 36 of 37
        // imported packs were in this state, which is a quality ceiling sitting
        // over almost everything the app draws.
        //
        // Rebuilding is not inventing detail: `sizes_for_source` still refuses
        // to enlarge past `MAX_UPSCALE`, and where an enlargement does happen it
        // is Lanczos3 in linear light with premultiplied alpha instead of the
        // shell's. Somebody has to do it; it should not be Windows.
        //
        // Left alone when the author already shipped a full ladder, when the
        // artwork is not square (resizing to a square would distort it), and on
        // any failure at all — a worse-looking cursor beats a missing one.
        if !animated {
            if let Some(built) = rebuilt_onto_the_ladder(&candidate.path) {
                let target = dir.join(&file_name);
                std::fs::write(&target, &built)?;
                if crate::cursor::engine::verify_loadable(&target).is_ok() {
                    return Ok((file_name, false));
                }
                log::debug!("{}: rebuilt ladder was refused, keeping the original", candidate.name);
            }
        }

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

/// Writes one resolved directory out as a single multi-role pack.
fn install_scheme(scheme: &SchemeCandidate, root: &Path) -> AppResult<ImportedPack> {
    let slug = paths::slugify(&scheme.name);
    if slug.is_empty() {
        return Err(AppError::invalid("that pack has no usable name"));
    }
    let dir = root.join(&slug);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir)?;

    let mut roles: BTreeMap<Role, String> = BTreeMap::new();
    let mut animated = false;

    for (role, (path, _)) in &scheme.picks {
        let candidate = Candidate {
            path: path.clone(),
            name: scheme.name.clone(),
            role: Some(*role),
            source: scheme.author.clone(),
        };
        // One bad file in a 47-file scheme must not cost the other 46.
        let Ok((file_name, is_animated)) = install_file(&candidate, &dir) else {
            continue;
        };
        let unique = format!("{}-{file_name}", role.file_stem().to_ascii_lowercase());
        let _ = std::fs::rename(dir.join(&file_name), dir.join(&unique));
        roles.insert(*role, unique);
        animated |= is_animated;
    }

    if roles.is_empty() {
        let _ = std::fs::remove_dir_all(&dir);
        return Err(AppError::invalid("none of its files could be read as a cursor"));
    }

    let pack = ImportedPack {
        id: format!("user:{slug}"),
        name: scheme.name.chars().take(40).collect(),
        category: categorise(&scheme.name, animated).to_owned(),
        roles,
        animated,
        source: scheme.author.chars().filter(|c| !c.is_control()).take(40).collect(),
        imported: crate::util::iso_now(),
    };
    std::fs::write(dir.join("pack.json"), serde_json::to_string_pretty(&pack)?)?;
    Ok(pack)
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
    let mut budget = MAX_FILES_SCANNED;

    let root = imported_dir()?;
    let mut report = ImportReport {
        imported: 0,
        skipped: 0,
        problems: Vec::new(),
        names: Vec::new(),
    };

    // Each subdirectory is one pack. A downloaded scheme is a folder of files
    // named for the Windows roles, so treated as loose files it would become
    // forty packs called "Wait" and "Help" — and two schemes that both ship a
    // `Help` would land in the same pack and overwrite each other.
    for dir in scheme_dirs(folder, &scratch, &mut problems) {
        if report.imported >= MAX_PACKS_PER_IMPORT {
            break;
        }
        let Some(scheme) = resolve_dir(&dir, &mut budget) else {
            continue;
        };
        match install_scheme(&scheme, &root) {
            Ok(pack) => {
                report.imported += 1;
                if report.names.len() < 40 {
                    report.names.push(pack.name);
                }
            }
            Err(e) => {
                report.skipped += 1;
                if problems.len() < 12 {
                    problems.push(format!("{}: {e}", scheme.name));
                }
            }
        }
    }

    // Files sitting directly in the folder keep the old pairing: this is the
    // `Name--cursor--Site.png` / `Name--pointer--Site.png` convention.
    let candidates = collect_loose(folder, &mut budget);
    let mut grouped: BTreeMap<String, Vec<Candidate>> = BTreeMap::new();
    for candidate in candidates {
        grouped.entry(candidate.name.clone()).or_default().push(candidate);
    }

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
/// The pixel size a catalog thumbnail is generated at.
///
/// The grid draws it at 40 CSS px, 44 while hovered. 128 is that with room for a
/// 200% display, where 40 CSS px is 80 device px and a 64 px source would be
/// enlarged by the browser. Generating above what is shown costs a few kilobytes
/// in a `data:` URI and removes the only case where the thumbnail is upscaled by
/// something other than us.
const THUMBNAIL_PX: u32 = 128;

/// A `data:` URI of a pack's arrow, for the catalog grid.
///
/// **Decoded from the file's own bytes rather than through `LoadImageW`.**
/// `cur_reader` asks Windows to produce the bitmap at a size, and Windows
/// resizes with a bilinear filter, on straight alpha, with no gamma correction —
/// the same resize the rest of this codebase exists to avoid. It was the one
/// user-facing image left going through it.
///
/// Now the largest resolution in the file is read out whole and brought down by
/// `Bitmap::resized`: Lanczos3, in linear light, on premultiplied alpha.
///
/// Falls back to the old path on anything it cannot parse. A thumbnail is not
/// worth failing a catalog over.
pub fn preview(pack: &ImportedPack) -> AppResult<String> {
    let files = role_files(pack)?;
    let path = files
        .get(&Role::Arrow)
        .or_else(|| files.values().next())
        .ok_or_else(|| AppError::invalid("nothing to preview"))?;

    let best = std::fs::read(path).ok().and_then(|bytes| {
        if crate::build::icon_reader::looks_like_an_ani(&bytes) {
            crate::build::icon_reader::decode_ani(&bytes)
                .ok()?
                .into_iter()
                .next()
                .map(|(frame, _)| frame)
        } else if crate::build::icon_reader::looks_like_an_icon(&bytes) {
            crate::build::icon_reader::decode_icon(&bytes).ok().map(|icon| icon.bitmap)
        } else {
            None
        }
    });

    match best {
        Some(bitmap) => bitmap.resized(THUMBNAIL_PX, THUMBNAIL_PX)?.to_png_data_uri(),
        None => cur_reader::read(path, THUMBNAIL_PX)?.to_png_data_uri(),
    }
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
mod ladder_tests {
    use super::*;
    use crate::build::bitmap::Bitmap;
    use crate::build::cur_writer::{write_cur, CursorImage};

    /// The shape most `.cur` files in circulation have: one entry, 32 px.
    fn one_entry_32px() -> Vec<u8> {
        let mut art = Bitmap::new(32, 32);
        for y in 0..32u32 {
            for x in 0..32u32 {
                if x + y > 8 && x < 24 && y < 26 {
                    art.set_pixel(x, y, [230, 230, 240, 255]);
                }
            }
        }
        write_cur(&[CursorImage::new(art, (2, 2))]).expect("a cursor")
    }

    fn scratch(name: &str, bytes: &[u8]) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join("cursorforge-ladder-tests");
        std::fs::create_dir_all(&dir).expect("scratch");
        let path = dir.join(name);
        std::fs::write(&path, bytes).expect("write");
        path
    }

    fn entry_count(bytes: &[u8]) -> usize {
        u16::from_le_bytes([bytes[4], bytes[5]]) as usize
    }

    #[test]
    fn a_single_entry_cursor_is_rebuilt_onto_the_ladder() {
        let original = one_entry_32px();
        assert_eq!(entry_count(&original), 1, "the fixture is the sparse case");
        let path = scratch("sparse.cur", &original);

        let rebuilt = rebuilt_onto_the_ladder(&path).expect("worth rebuilding");
        let expected = pipeline::sizes_for_source(32, 32).len();
        assert_eq!(entry_count(&rebuilt), expected, "one rung per size the source can fill");
        assert!(expected > 1, "a 32 px source can fill more than one rung");

        // The only authority on whether these bytes are a cursor.
        let out = scratch("sparse-rebuilt.cur", &rebuilt);
        assert!(crate::cursor::engine::verify_loadable(&out).is_ok());
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&out);
    }

    /// The author's hotspot is the one thing a rebuild must not lose: it is the
    /// pixel the click lands on, and the artwork is being redrawn around it.
    #[test]
    fn the_authors_hotspot_survives_the_rebuild() {
        let path = scratch("hotspot.cur", &one_entry_32px());
        let rebuilt = rebuilt_onto_the_ladder(&path).expect("worth rebuilding");

        // 2/31 of the way across a 32 px cursor, carried onto every rung.
        for i in 0..entry_count(&rebuilt) {
            let e = 6 + 16 * i;
            let width = if rebuilt[e] == 0 { 256u32 } else { rebuilt[e] as u32 };
            let hx = u16::from_le_bytes([rebuilt[e + 4], rebuilt[e + 5]]) as f32;
            let expected = (2.0 / 31.0) * (width - 1) as f32;
            assert!(
                (hx - expected).abs() <= 1.0,
                "{width}px rung put the hotspot at {hx}, expected about {expected}"
            );
        }
        let _ = std::fs::remove_file(&path);
    }

    /// A file that already carries a full ladder is the author's work and is
    /// left exactly as it is.
    /// Two entries is the author making a choice, and a hand-drawn 16 px arrow
    /// beats any resample of the 32 px one.
    #[test]
    fn a_cursor_with_more_than_one_entry_is_the_authors_work() {
        let mut art = Bitmap::new(16, 16);
        art.set_pixel(0, 0, [255, 255, 255, 255]);
        let mut big = Bitmap::new(32, 32);
        big.set_pixel(0, 0, [255, 255, 255, 255]);
        let two = write_cur(&[CursorImage::new(art, (0, 0)), CursorImage::new(big, (0, 0))])
            .expect("a cursor");
        let path = scratch("two.cur", &two);
        assert!(
            rebuilt_onto_the_ladder(&path).is_none(),
            "two entries means the author picked them"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_rich_ladder_is_left_alone() {
        let mut images = Vec::new();
        for size in crate::build::cur_writer::TARGET_SIZES {
            let mut art = Bitmap::new(size, size);
            art.set_pixel(0, 0, [255, 255, 255, 255]);
            images.push(CursorImage::new(art, (0, 0)));
        }
        let rich = write_cur(&images).expect("a cursor");
        let path = scratch("rich.cur", &rich);
        assert!(
            rebuilt_onto_the_ladder(&path).is_none(),
            "an author's full ladder must not be replaced with ours"
        );
        let _ = std::fs::remove_file(&path);
    }
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

    /// Everything imported currently lands in one category on purpose.
    /// MINIMAL CURSED exists but is empty until there is something to put in it.
    #[test]
    fn every_import_lands_in_optimal_cursed_for_now() {
        for (name, animated) in [
            ("Batman & Batarang", false),
            ("Minecraft Enchanted Sword", true),
            ("something plain", false),
            ("", false),
        ] {
            assert_eq!(categorise(name, animated), "OPTIMAL CURSED", "for {name:?}");
        }
    }

    /// The names a downloaded scheme actually uses, from the three sets in
    /// hand: Windows spellings, CSS/W3C spellings, and `*Resize` spellings.
    #[test]
    fn real_scheme_filenames_map_to_the_right_roles() {
        let cases = [
            ("Normal", Role::Arrow),
            ("Arrow", Role::Arrow),
            ("Help", Role::Help),
            ("AppStarting", Role::AppStarting),
            ("Wait", Role::Wait),
            ("Cross", Role::Crosshair),
            ("Precision", Role::Crosshair),
            ("IBeam", Role::IBeam),
            ("Text", Role::IBeam),
            ("Handwriting", Role::NWPen),
            ("pencil", Role::NWPen),
            ("NO", Role::No),
            ("NotAllowed", Role::No),
            ("SizeNS", Role::SizeNS),
            ("NSResize", Role::SizeNS),
            ("SizeWE", Role::SizeWE),
            ("EWResize", Role::SizeWE),
            ("SizeNWSE", Role::SizeNWSE),
            ("NWResize", Role::SizeNWSE),
            ("SizeNESW", Role::SizeNESW),
            ("NEResize", Role::SizeNESW),
            ("SizeAll", Role::SizeAll),
            ("Move", Role::SizeAll),
            ("UpArrow", Role::UpArrow),
            ("Alternate", Role::UpArrow),
            ("Hand", Role::Hand),
            ("Link", Role::Hand),
        ];
        for (stem, want) in cases {
            let got = role_from_filename(stem).map(|(r, _)| r);
            assert_eq!(got, Some(want), "{stem} should be {want:?}");
        }
    }

    /// Resolution suffixes are not part of the role, and the file carrying
    /// several resolutions is the better master.
    #[test]
    fn resolution_suffixes_are_stripped_and_multi_size_wins() {
        let plain = role_from_filename("Wait").expect("Wait");
        let multi = role_from_filename("Wait_32-48-64").expect("Wait_32-48-64");
        let single = role_from_filename("Wait_256").expect("Wait_256");

        assert_eq!(multi.0, Role::Wait);
        assert_eq!(single.0, Role::Wait);
        assert!(multi.1 > plain.1, "a multi-resolution file is the better master");
        assert!(plain.1 > single.1, "one fixed size is the weakest master");
    }

    /// Both are real cursors and both are wanted, but only one can be the
    /// scheme's arrow — and it is the right-handed one.
    #[test]
    fn right_handed_and_first_alternates_beat_their_variants() {
        let normal = role_from_filename("Normal").expect("Normal");
        let lefty = role_from_filename("Normal-lefties").expect("Normal-lefties");
        assert_eq!(lefty.0, Role::Arrow, "still an arrow, just not the one to use");
        assert!(normal.1 > lefty.1);

        let move_first = role_from_filename("Move").expect("Move");
        let move_alt = role_from_filename("Move_1").expect("Move_1");
        assert_eq!(move_alt.0, Role::SizeAll);
        assert!(move_first.1 > move_alt.1);
    }

    /// Windows has no slot for these. Guessing would let a scheme's zoom-in
    /// icon overwrite its actual arrow.
    #[test]
    fn cursors_with_no_windows_role_are_skipped_not_guessed() {
        for stem in [
            "alias",
            "cell",
            "copy",
            "context-menu",
            "vertical-text",
            "zoom-in",
            "zoom-out",
            "dnd-ask",
            "dnd-link",
            "dnd-none",
            "pirate",
            "dot_box_mask",
        ] {
            assert_eq!(role_from_filename(stem), None, "{stem} names no Windows role");
        }
    }

    /// `vertical-text` must not take IBeam away from `Text`, and `dnd-link`
    /// must not take Hand away from `Link`.
    #[test]
    fn a_near_miss_never_outranks_the_real_thing() {
        assert!(role_from_filename("vertical-text").is_none());
        assert_eq!(role_from_filename("Text").map(|(r, _)| r), Some(Role::IBeam));
        assert!(role_from_filename("dnd-link").is_none());
        assert_eq!(role_from_filename("Link").map(|(r, _)| r), Some(Role::Hand));
    }

    #[test]
    fn an_inf_yields_the_scheme_name_and_its_role_mapping() {
        let inf = r#"
[Scheme.Reg]
HKCU,"Control Panel\Cursors","Arrow",,"%25%\Cursors\Arrow.cur"
HKCU,"Control Panel\Cursors","Wait",,"%25%\Cursors\Wait.ani"
HKCU,"Control Panel\Cursors","(Default)",,"Ghost"
HKCU,"Control Panel\Cursors\Schemes","Ghost",,""
"#;
        let (name, pairs) = parse_inf(inf);
        assert_eq!(name.as_deref(), Some("Ghost"));
        assert!(pairs.contains(&("Arrow".into(), "Arrow.cur".into())));
        assert!(pairs.contains(&("Wait".into(), "Wait.ani".into())));
        // The Schemes subkey is a different key and carries no role.
        assert_eq!(pairs.len(), 2, "only Control Panel\\Cursors values count");
    }

    /// An `.inf` names its own files. It does not get to name a path.
    #[test]
    fn an_inf_cannot_point_outside_its_own_folder() {
        let inf = r#"HKCU,"Control Panel\Cursors","Arrow",,"..\..\Windows\System32\evil.cur""#;
        let (_, pairs) = parse_inf(inf);
        assert_eq!(pairs, vec![("Arrow".to_owned(), "evil.cur".to_owned())]);
    }

    #[test]
    fn folder_names_lose_their_download_hash() {
        assert_eq!(pretty_folder_name("Skyrim-Set-2-563e85ef"), "Skyrim Set 2");
        assert_eq!(pretty_folder_name("Ghost-8e871896"), "Ghost");
        assert_eq!(pretty_folder_name("Geared-Brass-3715df67"), "Geared Brass");
        // A trailing role word is the site's filing, not part of the name.
        assert_eq!(
            pretty_folder_name("glowing-futuristic-arrow-normal-b7dac674"),
            "Glowing Futuristic Arrow"
        );
        // A folder named for the role it fills reads better spelled properly.
        assert_eq!(pretty_folder_name("sizenwse-319e6096"), "SizeNWSE");
    }

    /// A raw download filename is not a name to show anyone, but the `--`
    /// convention's name already is and must survive untouched.
    #[test]
    fn raw_download_filenames_are_tidied_but_conventional_ones_are_not() {
        assert_eq!(pretty_folder_name("sukuna-human-finger-f4e78762"), "Sukuna Human Finger");
        assert_eq!(pretty_folder_name("paper-airplane-e7ea7488"), "Paper Airplane");
        // Not a hash, so nothing is stripped.
        assert_eq!(pretty_folder_name("cur1020"), "Cur1020");
        // The `--` convention keeps its own name, punctuation and all.
        assert_eq!(parse_name("Batman & Batarang--cursor--Sweezy").0, "Batman & Batarang");
    }

    #[test]
    fn an_author_is_credited_when_the_readme_names_one() {
        assert_eq!(
            author_from_readme("Ghost\n=====\n\nAuthor: treetog\n").as_deref(),
            Some("treetog")
        );
        assert_eq!(author_from_readme("no attribution here"), None);
    }

    /// Readme files are full of emoji, and a line that opens with one used to
    /// crash the whole import on a byte-index slice.
    #[test]
    fn a_readme_full_of_emoji_does_not_crash_the_import() {
        assert_eq!(author_from_readme("💫 sparkly pack 💫\nAuthor: someone\n").as_deref(), Some("someone"));
        assert_eq!(author_from_readme("💫"), None);
        assert_eq!(author_from_readme("by: 🎨 artist"), Some("🎨 artist".to_owned()));
        // Every prefix, against a line too short to hold it.
        for line in ["b", "by", "au", "💫b", "🎨"] {
            let _ = author_from_readme(line);
        }
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
