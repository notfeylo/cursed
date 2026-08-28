use crate::cursor::roles::Role;
use crate::error::{AppError, AppResult};
use crate::paths;
use crate::util::iso_now;
use crate::state::settings::HoverStyle;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A preset is the complete, restorable state of the user's pointer (PRD §8) —
/// not just a pack id. Everything needed to reproduce the exact pointer lives
/// here, so a preset stays meaningful after the catalog changes.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Preset {
    pub id: String,
    pub name: String,
    pub created: String,
    pub base_pack: String,
    /// Role -> custom cursor id, for roles the base pack does not own.
    #[serde(default)]
    pub overrides: BTreeMap<Role, String>,
    pub tint: String,
    pub size: u32,
    pub outline: bool,
    pub hotkey: Option<String>,
    #[serde(default)]
    pub is_default: bool,
    /// What the link hand is. A preset stores the whole pointer, and after
    /// `HoverStyle` existed the hand became part of "the whole pointer".
    ///
    /// Defaulted rather than versioned: every preset written before this field
    /// existed described a pack showing its own hand, which is exactly what
    /// `Pack` means.
    #[serde(default = "default_hover")]
    pub hover_style: HoverStyle,
}

fn default_hover() -> HoverStyle {
    HoverStyle::Pack
}

impl Preset {
    pub fn new(name: &str, base_pack: &str, tint: &str, size: u32, outline: bool) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.trim().chars().take(48).collect(),
            created: iso_now(),
            base_pack: base_pack.to_owned(),
            overrides: BTreeMap::new(),
            tint: tint.to_owned(),
            size: size.clamp(32, 256),
            outline,
            hotkey: None,
            is_default: false,
            // Whatever is in effect now, so saving a cursor saves the pointer
            // that is on screen rather than a variant of it.
            hover_style: crate::state::settings::get().hover_style,
        }
    }
}

/// Reads the saved presets, recovering from the backup if the file is damaged.
///
/// This used to be `unwrap_or_default()` on a parse failure, which is the worst
/// possible answer: a `presets.json` that would not parse became an empty list,
/// the UI showed no presets, and the next save wrote that empty list over the
/// only copy. A file the user could have opened in Notepad and fixed was gone
/// by the time they noticed. Now a damaged file is set aside, the backup is
/// tried, and nothing is overwritten on the strength of a failed read.
fn read() -> AppResult<Vec<Preset>> {
    read_from(&paths::presets_file()?)
}

/// The same read, against a named file.
///
/// Split out so the regression above can actually be tested. The version that
/// took no argument could only be exercised against the developer's own
/// `presets.json`, which meant the one bug this function exists to prevent had
/// no test at all — the store's generic tests cover the mechanism, not this
/// caller's use of it, and it was this caller that got it wrong.
fn read_from(file: &std::path::Path) -> AppResult<Vec<Preset>> {
    let (presets, source) = crate::state::store::read::<Vec<Preset>>(file);
    if source == crate::state::store::Source::Backup {
        log::warn!("presets were recovered from the backup copy");
    }
    Ok(presets)
}

fn write(presets: &[Preset]) -> AppResult<()> {
    write_to(&paths::presets_file()?, presets)
}

fn write_to(file: &std::path::Path, presets: &[Preset]) -> AppResult<()> {
    crate::state::store::write(file, &serde_json::to_string_pretty(presets)?)
}

pub fn list() -> AppResult<Vec<Preset>> {
    read()
}

pub fn get(id: &str) -> AppResult<Preset> {
    read()?
        .into_iter()
        .find(|p| p.id == id)
        .ok_or(AppError::UnknownPreset)
}

/// Insert or update by id.
pub fn upsert(preset: Preset) -> AppResult<Preset> {
    let mut all = read()?;
    match all.iter_mut().find(|p| p.id == preset.id) {
        Some(existing) => *existing = preset.clone(),
        None => all.push(preset.clone()),
    }
    // Exactly one default, always.
    if preset.is_default {
        for other in all.iter_mut().filter(|p| p.id != preset.id) {
            other.is_default = false;
        }
    }
    write(&all)?;
    Ok(preset)
}

pub fn remove(id: &str) -> AppResult<()> {
    let mut all = read()?;
    let before = all.len();
    all.retain(|p| p.id != id);
    if all.len() == before {
        return Err(AppError::UnknownPreset);
    }
    write(&all)
}

pub fn set_default(id: &str) -> AppResult<()> {
    let mut all = read()?;
    if !all.iter().any(|p| p.id == id) {
        return Err(AppError::UnknownPreset);
    }
    for preset in all.iter_mut() {
        preset.is_default = preset.id == id;
    }
    write(&all)
}

pub fn duplicate(id: &str) -> AppResult<Preset> {
    let source = get(id)?;
    let copy = Preset {
        id: uuid::Uuid::new_v4().to_string(),
        name: format!("{} COPY", source.name).chars().take(48).collect(),
        created: iso_now(),
        hotkey: None,
        is_default: false,
        ..source
    };
    upsert(copy)
}

/// The preset bound to a given hotkey slot, if any.
pub fn by_hotkey(accelerator: &str) -> AppResult<Option<Preset>> {
    Ok(read()?
        .into_iter()
        .find(|p| p.hotkey.as_deref() == Some(accelerator)))
}

pub fn default_preset() -> AppResult<Option<Preset>> {
    Ok(read()?.into_iter().find(|p| p.is_default))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join("cursorforge-preset-tests").join(name);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch dir");
        dir.join("presets.json")
    }

    /// **The regression.**
    ///
    /// `read` was `unwrap_or_default()` on a parse failure, which is the worst
    /// available answer: a `presets.json` that would not parse became an empty
    /// list, the UI showed no presets, and the next save wrote that empty list
    /// over the only copy. A file the user could have fixed in Notepad was gone
    /// by the time they noticed it was missing.
    ///
    /// Two things have to hold. The backup has to be used, and the damaged file
    /// has to still exist somewhere afterwards.
    #[test]
    fn a_damaged_presets_file_falls_back_to_the_backup_rather_than_to_nothing() {
        let file = scratch("damaged");
        let one = Preset::new("PLASMA", "precision-gap-cross", "#2E8BFF", 48, true);
        let two = Preset::new("EMBER", "precision-gap-cross", "#FF6A2E", 32, false);

        write_to(&file, std::slice::from_ref(&one)).expect("first save");
        write_to(&file, &[one.clone(), two]).expect("second save");

        // Something truncates it: a crash mid-write, a bad sector, an antivirus.
        std::fs::write(&file, r#"[{"id":"pl"#).expect("truncate");

        let recovered = read_from(&file).expect("read");
        assert_eq!(recovered.len(), 1, "the backup held one preset");
        assert_eq!(recovered[0].name, "PLASMA");

        let kept = file.with_file_name("presets.json.corrupt");
        assert!(kept.is_file(), "the damaged bytes are the user's and must survive");
    }

    /// And the file is healed, so the *next* launch is an ordinary one rather
    /// than a second recovery from a backup that is now one save out of date.
    #[test]
    fn a_recovered_presets_file_is_put_back() {
        let file = scratch("healed");
        let preset = Preset::new("PLASMA", "precision-gap-cross", "#2E8BFF", 48, true);
        write_to(&file, std::slice::from_ref(&preset)).expect("first save");
        write_to(&file, std::slice::from_ref(&preset)).expect("second save");
        std::fs::write(&file, "}{").expect("damage");

        assert_eq!(read_from(&file).expect("read").len(), 1);
        assert_eq!(read_from(&file).expect("read again").len(), 1);
        assert!(file.is_file());
    }

    /// A genuinely empty list is not damage, and must not be treated as such.
    #[test]
    fn no_presets_is_a_normal_state() {
        let file = scratch("empty");
        write_to(&file, &[]).expect("save");
        assert!(read_from(&file).expect("read").is_empty());
        assert!(!file.with_file_name("presets.json.corrupt").exists());
    }

    /// A first run has no file at all.
    #[test]
    fn a_missing_presets_file_reads_as_no_presets() {
        let file = scratch("absent");
        assert!(read_from(&file).expect("read").is_empty());
    }

    /// A preset written by v1.6 has no `overrides` and no `isDefault`, because
    /// neither field existed. It has to load, or every user who has had this app
    /// since then opens it one day to an empty SAVED screen.
    #[test]
    fn a_preset_from_an_older_version_still_loads() {
        let file = scratch("old");
        std::fs::write(
            &file,
            r##"[{
                "id": "8f1a-old",
                "name": "PLASMA",
                "created": "2026-08-08T10:00:00Z",
                "basePack": "precision-gap-cross",
                "tint": "#2E8BFF",
                "size": 48,
                "outline": true,
                "hotkey": null
            }]"##,
        )
        .expect("write");

        let loaded = read_from(&file).expect("an old preset must load");
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].name, "PLASMA");
        assert!(loaded[0].overrides.is_empty());
        assert!(!loaded[0].is_default);
    }
}
