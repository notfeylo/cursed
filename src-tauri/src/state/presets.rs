use crate::cursor::roles::Role;
use crate::error::{AppError, AppResult};
use crate::paths;
use crate::util::iso_now;
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
    let file = paths::presets_file()?;
    let (presets, source) = crate::state::store::read::<Vec<Preset>>(&file);
    if source == crate::state::store::Source::Backup {
        log::warn!("presets were recovered from the backup copy");
    }
    Ok(presets)
}

fn write(presets: &[Preset]) -> AppResult<()> {
    let file = paths::presets_file()?;
    crate::state::store::write(&file, &serde_json::to_string_pretty(presets)?)
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
