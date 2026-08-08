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

fn read() -> AppResult<Vec<Preset>> {
    let file = paths::presets_file()?;
    match std::fs::read_to_string(&file) {
        Ok(text) => Ok(serde_json::from_str(crate::util::strip_bom(&text)).unwrap_or_default()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(e.into()),
    }
}

fn write(presets: &[Preset]) -> AppResult<()> {
    let file = paths::presets_file()?;
    let temp = file.with_extension("json.tmp");
    std::fs::write(&temp, serde_json::to_string_pretty(presets)?)?;
    std::fs::rename(&temp, &file)?;
    Ok(())
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
