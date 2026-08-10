//! `.cfpack` — a preset and its custom artwork, in a zip.
//!
//! This is the one file format Cursed accepts from strangers, so it is the
//! one place that has to assume the sender is hostile. Everything in PRD §13.5
//! is enforced here, in this order: schema first, then paths, then extensions,
//! then budgets. A `.cfpack` can never carry an executable, never write outside
//! Cursed's storage, and never define a role by naming a registry key.

use crate::cursor::roles::Role;
use crate::error::{AppError, AppResult};
use crate::paths;
use crate::state::presets::Preset;
use crate::util::iso_now;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::Path;

const MANIFEST: &str = "manifest.json";
const FORMAT_VERSION: u32 = 1;
/// Extension allow-list. Anything not on it is refused outright rather than
/// ignored, so a pack cannot smuggle a payload past by being quietly skipped.
const ALLOWED_EXTENSIONS: [&str; 5] = ["png", "svg", "cur", "ani", "json"];
const MAX_ENTRIES: usize = 200;
const MAX_UNCOMPRESSED: u64 = 50 * 1024 * 1024;
/// A single entry that expands more than 200x is a zip bomb, whatever it claims
/// to be.
const MAX_RATIO: u64 = 200;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub format: u32,
    pub name: String,
    pub base_pack: String,
    pub tint: String,
    pub size: u32,
    pub outline: bool,
    #[serde(default)]
    pub overrides: BTreeMap<Role, String>,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub created: String,
}

impl Manifest {
    /// Validates every field before a single byte is extracted.
    fn validated(self) -> AppResult<Self> {
        if self.format != FORMAT_VERSION {
            return Err(AppError::invalid(format!(
                "this pack was made for format {} and this build reads format {FORMAT_VERSION}",
                self.format
            )));
        }
        if self.name.trim().is_empty() || self.name.chars().count() > 48 {
            return Err(AppError::invalid("the pack name is empty or too long"));
        }
        if crate::util::parse_hex_color(&self.tint).is_none() {
            return Err(AppError::invalid("the pack's tint is not a colour"));
        }
        if !(32..=256).contains(&self.size) {
            return Err(AppError::invalid("the pack's size is out of range"));
        }
        // `base_pack` names a catalog entry, never a path.
        if crate::packs::styles::find(&self.base_pack).is_none() {
            return Err(AppError::invalid(format!(
                "this pack is built on \"{}\", which is not in the catalog",
                sanitise(&self.base_pack)
            )));
        }
        for file_name in self.overrides.values() {
            check_entry_name(file_name)?;
        }
        Ok(self)
    }
}

/// Strips control characters from untrusted text before it reaches a message.
/// Text from a pack is data, not instruction, and it never gets to move a
/// cursor around a log line or a dialog (PRD §13.6).
fn sanitise(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_control())
        .take(64)
        .collect()
}

/// Rejects zip-slip, absolute paths, device names and disallowed extensions.
fn check_entry_name(name: &str) -> AppResult<()> {
    if name == MANIFEST {
        return Ok(());
    }
    paths::validate_relative(name)?;

    let extension = Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();

    if !ALLOWED_EXTENSIONS.contains(&extension.as_str()) {
        return Err(AppError::invalid(format!(
            "\"{}\" is not a kind of file a cursor pack may contain",
            sanitise(name)
        )));
    }
    Ok(())
}

/// Writes a `.cfpack` for a preset.
pub fn export(preset: &Preset, destination: &Path) -> AppResult<()> {
    let manifest = Manifest {
        format: FORMAT_VERSION,
        name: preset.name.clone(),
        base_pack: preset.base_pack.clone(),
        tint: preset.tint.clone(),
        size: preset.size,
        outline: preset.outline,
        overrides: preset.overrides.clone(),
        author: "feylo".to_owned(),
        created: iso_now(),
    };

    let file = std::fs::File::create(destination)?;
    let mut zip = zip::ZipWriter::new(file);
    let options: zip::write::FileOptions<'_, ()> =
        zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    zip.start_file(MANIFEST, options)
        .map_err(|e| AppError::storage(format!("could not write the manifest: {e}")))?;
    zip.write_all(serde_json::to_string_pretty(&manifest)?.as_bytes())?;

    // Custom artwork the preset overrides roles with travels alongside it,
    // otherwise the pack is a set of dangling references on the far end.
    let custom = paths::custom_dir()?;
    for file_name in preset.overrides.values() {
        let source = custom.join(paths::validate_relative(file_name)?);
        let Ok(bytes) = std::fs::read(&source) else {
            continue;
        };
        zip.start_file(file_name.as_str(), options)
            .map_err(|e| AppError::storage(format!("could not add {file_name}: {e}")))?;
        zip.write_all(&bytes)?;
    }

    zip.finish()
        .map_err(|e| AppError::storage(format!("could not finish the pack: {e}")))?;
    Ok(())
}

/// Reads a `.cfpack` and returns the preset it describes.
pub fn import(source: &Path) -> AppResult<Preset> {
    let file = std::fs::File::open(source)
        .map_err(|_| AppError::invalid("that pack file could not be opened"))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|_| AppError::invalid("that file is not a cursor pack"))?;

    if archive.len() > MAX_ENTRIES {
        return Err(AppError::invalid("that pack contains too many files"));
    }

    // Budget check across the whole archive before anything is written to disk.
    let mut declared_total = 0u64;
    for index in 0..archive.len() {
        let entry = archive
            .by_index_raw(index)
            .map_err(|_| AppError::invalid("that pack is damaged"))?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().to_owned();
        check_entry_name(&name)?;

        let uncompressed = entry.size();
        let compressed = entry.compressed_size().max(1);
        if uncompressed / compressed > MAX_RATIO {
            return Err(AppError::invalid(
                "that pack expands far more than it should and was refused",
            ));
        }
        declared_total = declared_total.saturating_add(uncompressed);
        if declared_total > MAX_UNCOMPRESSED {
            return Err(AppError::invalid("that pack is too large to unpack safely"));
        }
    }

    // Schema before extraction — a manifest we would refuse anyway should never
    // cost a single file write.
    let manifest: Manifest = {
        let mut entry = archive
            .by_name(MANIFEST)
            .map_err(|_| AppError::invalid("that pack has no manifest"))?;
        let mut text = String::new();
        entry.read_to_string(&mut text)?;
        serde_json::from_str::<Manifest>(&text)
            .map_err(|e| AppError::invalid(format!("the pack's manifest is malformed: {e}")))?
            .validated()?
    };

    let destination = paths::custom_dir()?;
    let mut written = 0u64;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|_| AppError::invalid("that pack is damaged"))?;
        if entry.is_dir() || entry.name() == MANIFEST {
            continue;
        }
        let name = entry.name().to_owned();
        check_entry_name(&name)?;

        let target = destination.join(paths::validate_relative(&name)?);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes)?;
        written = written.saturating_add(bytes.len() as u64);
        if written > MAX_UNCOMPRESSED {
            return Err(AppError::invalid("that pack unpacked to more than it declared"));
        }
        std::fs::write(&target, &bytes)?;

        // The final word on whether a .cur is a .cur belongs to Windows.
        if target.extension().is_some_and(|e| e == "cur" || e == "ani") {
            if let Err(e) = crate::cursor::engine::verify_loadable(&target) {
                let _ = std::fs::remove_file(&target);
                return Err(AppError::invalid(format!(
                    "\"{}\" is not a usable cursor: {e}",
                    sanitise(&name)
                )));
            }
        }
    }

    let mut preset = Preset::new(
        &sanitise(&manifest.name),
        &manifest.base_pack,
        &manifest.tint,
        manifest.size,
        manifest.outline,
    );
    preset.overrides = manifest.overrides;
    crate::state::presets::upsert(preset)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_names_that_escape_or_execute_are_refused() {
        for bad in [
            "../../evil.cur",
            r"..\evil.cur",
            "/absolute.cur",
            r"C:\Windows\evil.cur",
            "payload.exe",
            "payload.dll",
            "script.ps1",
            "script.bat",
            "arrow.cur.exe",
            "NUL.cur",
            "stream.cur:hidden",
        ] {
            assert!(check_entry_name(bad).is_err(), "should refuse {bad:?}");
        }
    }

    #[test]
    fn ordinary_pack_contents_are_accepted() {
        for good in ["manifest.json", "arrow.cur", "spinner.ani", "master.svg", "art/tip.png"] {
            assert!(check_entry_name(good).is_ok(), "should accept {good:?}");
        }
    }

    #[test]
    fn a_manifest_naming_an_unknown_pack_is_rejected() {
        let manifest = Manifest {
            format: FORMAT_VERSION,
            name: "TEST".into(),
            base_pack: "not-a-real-pack".into(),
            tint: "#2E8BFF".into(),
            size: 48,
            outline: true,
            overrides: BTreeMap::new(),
            author: String::new(),
            created: String::new(),
        };
        assert!(manifest.validated().is_err());
    }

    #[test]
    fn manifest_fields_are_range_checked() {
        let base = Manifest {
            format: FORMAT_VERSION,
            name: "TEST".into(),
            base_pack: "precision-gap-cross".into(),
            tint: "#2E8BFF".into(),
            size: 48,
            outline: true,
            overrides: BTreeMap::new(),
            author: String::new(),
            created: String::new(),
        };
        assert!(base.clone().validated().is_ok());

        assert!(Manifest { format: 99, ..base.clone() }.validated().is_err());
        assert!(Manifest { size: 9_999, ..base.clone() }.validated().is_err());
        assert!(Manifest { tint: "purple".into(), ..base.clone() }.validated().is_err());
        assert!(Manifest { name: String::new(), ..base.clone() }.validated().is_err());
    }

    #[test]
    fn untrusted_text_is_stripped_of_control_characters() {
        assert_eq!(sanitise("PLASMA\u{1b}[31m\n"), "PLASMA[31m");
        assert_eq!(
            sanitise("ignore previous\u{0}instructions"),
            "ignore previousinstructions",
            "text from a pack is data; nothing in it is ever obeyed"
        );
    }
}
