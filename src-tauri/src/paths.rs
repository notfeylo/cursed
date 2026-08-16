//! Everything Cursed writes lives under one root, and nothing it is asked
//! to read may live outside it. This module is the only place that decides what
//! a legal path is (PRD §13.4).

use crate::error::{AppError, AppResult};
use std::path::{Component, Path, PathBuf};

/// `Cursed`, or `Cursed (Dev)` on the development channel. Never written as a
/// literal anywhere else — see `crate::channel`.
const APP_DIR: &str = crate::channel::DATA_DIR;

/// `%APPDATA%\Cursed`, or `%APPDATA%\Cursed (Dev)`
///
/// Renaming the app must not cost anyone their settings, presets and imported
/// cursors, so the first call after an upgrade moves the old folder across.
/// Losing somebody's imported artwork because the product changed name would be
/// a self-inflicted wound.
pub fn root() -> AppResult<PathBuf> {
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .ok_or_else(|| AppError::storage("APPDATA is not set"))?;
    let dir = base.join(APP_DIR);

    // The migration fires when this channel's directory is absent — which is
    // true of the dev channel on its very first run, on a machine whose
    // `CursorForge` folder belongs to the *user* channel. Without this gate the
    // first `npm run tauri dev` would move the real install's settings, presets
    // and imported artwork into the dev channel and leave the user channel to
    // start from nothing. Only the channel that inherited the old name may
    // claim the old folder.
    if !dir.exists() {
        if let Some(legacy_name) = crate::channel::legacy_data_dir() {
            let legacy = base.join(legacy_name);
            if legacy.is_dir() {
                // A rename is atomic and cheap. If it fails — the old folder is
                // open in Explorer, say — fall through and start fresh rather
                // than refuse to launch.
                match std::fs::rename(&legacy, &dir) {
                    Ok(()) => log::info!("moved existing data from {legacy_name} to {APP_DIR}"),
                    Err(e) => log::warn!("could not migrate {legacy_name}: {e}"),
                }
            }
        }
    }

    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn sub(name: &str) -> AppResult<PathBuf> {
    let dir = root()?.join(name);
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Cursors rendered from catalog packs at the current tint/size.
pub fn cache_dir() -> AppResult<PathBuf> {
    sub("cache")
}

/// Cursors built from user-supplied images.
pub fn custom_dir() -> AppResult<PathBuf> {
    sub("custom")
}

/// The untouched pre-install pointer scheme.
pub fn backup_dir() -> AppResult<PathBuf> {
    sub("backup")
}

pub fn logs_dir() -> AppResult<PathBuf> {
    sub("logs")
}

pub fn settings_file() -> AppResult<PathBuf> {
    Ok(root()?.join("settings.json"))
}

pub fn presets_file() -> AppResult<PathBuf> {
    Ok(root()?.join("presets.json"))
}

pub fn original_scheme_file() -> AppResult<PathBuf> {
    Ok(backup_dir()?.join("original_scheme.json"))
}

/// Windows device names are reserved in every directory, with or without an
/// extension. A file called `NUL.cur` is not a file.
const RESERVED_STEMS: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Rejects a *relative* component string before it is ever joined to a root.
/// Used for pack ids, preset names turned into filenames, and zip entry names.
pub fn validate_relative(candidate: &str) -> AppResult<PathBuf> {
    if candidate.is_empty() || candidate.len() > 255 {
        return Err(AppError::invalid("empty or over-long name"));
    }
    // A colon is either a drive letter or an alternate data stream. Neither is
    // ever legitimate in a relative name we constructed.
    if candidate.contains(':') || candidate.contains('\0') {
        return Err(AppError::PathEscape);
    }
    if candidate.starts_with("\\\\") || candidate.starts_with("//") {
        return Err(AppError::PathEscape); // UNC
    }

    let path = Path::new(candidate);
    for component in path.components() {
        match component {
            Component::Normal(part) => {
                let text = part
                    .to_str()
                    .ok_or_else(|| AppError::invalid("name is not valid Unicode"))?;
                let stem = text
                    .split('.')
                    .next()
                    .unwrap_or(text)
                    .trim_end_matches(' ')
                    .to_ascii_uppercase();
                if RESERVED_STEMS.contains(&stem.as_str()) {
                    return Err(AppError::invalid(format!("{text} is a reserved name")));
                }
                if text.ends_with(' ') || text.ends_with('.') {
                    return Err(AppError::invalid("name ends with a space or dot"));
                }
                if text.chars().any(|c| matches!(c, '<' | '>' | '"' | '|' | '?' | '*')) {
                    return Err(AppError::invalid("name contains an illegal character"));
                }
            }
            // `..`, a leading `\`, or a drive prefix all mean "leave the root".
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(AppError::PathEscape)
            }
            Component::CurDir => {}
        }
    }
    Ok(path.to_path_buf())
}

/// Strips the `\\?\` verbatim prefix `canonicalize` adds, so two canonical
/// paths can be compared without one of them silently failing `starts_with`.
fn strip_verbatim(path: &Path) -> PathBuf {
    let text = path.to_string_lossy();
    match text.strip_prefix(r"\\?\") {
        Some(rest) => PathBuf::from(rest),
        None => path.to_path_buf(),
    }
}

/// Resolves an absolute path supplied from outside and asserts it is inside
/// Cursed's storage. Symlinks are followed *before* the check, so a link
/// planted inside the root cannot point out of it.
pub fn ensure_inside_storage(candidate: &Path) -> AppResult<PathBuf> {
    let root = strip_verbatim(&root()?.canonicalize()?);
    let resolved = strip_verbatim(&candidate.canonicalize().map_err(|_| AppError::PathEscape)?);
    if !resolved.starts_with(&root) {
        return Err(AppError::PathEscape);
    }
    Ok(resolved)
}

/// Turns a display name into something safe to use as a directory name.
pub fn slugify(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut last_dash = true;
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    let trimmed = out.trim_matches('-').to_owned();
    if trimmed.is_empty() {
        "untitled".to_owned()
    } else {
        trimmed.chars().take(64).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_traversal_and_device_names() {
        for bad in [
            "..",
            "../escape",
            r"..\escape",
            r"C:\Windows\System32",
            r"\\server\share",
            "NUL",
            "nul.cur",
            "com1.cur",
            "arrow.cur:hidden",
            "trailing ",
            "trailing.",
            "wild*card",
            "",
        ] {
            assert!(validate_relative(bad).is_err(), "should reject {bad:?}");
        }
    }

    #[test]
    fn accepts_ordinary_names() {
        for good in ["arrow.cur", "neon-plasma/Arrow.cur", "my logo.png", "a.b.c.cur"] {
            assert!(validate_relative(good).is_ok(), "should accept {good:?}");
        }
    }

    /// A blanket find-and-replace during the rename set this to the new name,
    /// which would have quietly stranded every existing user's data. The two
    /// must never be equal.
    #[test]
    fn the_legacy_data_folder_is_not_the_current_one() {
        if let Some(legacy) = crate::channel::legacy_data_dir() {
            assert_ne!(
                APP_DIR, legacy,
                "migration would be a no-op and existing data would be orphaned"
            );
        }
    }

    /// The dev channel must not adopt the folder the user channel came from.
    /// If it ever does, the first dev launch on this machine takes the real
    /// install's data with it.
    #[test]
    fn the_dev_channel_inherits_nothing() {
        if crate::channel::IS_DEV {
            assert!(crate::channel::legacy_data_dir().is_none());
        }
    }

    /// The two channels must not share a data directory. This is the single
    /// most important separation in the product: one root holds the settings,
    /// the presets, the imported artwork and the original-scheme snapshot.
    #[test]
    fn the_channels_do_not_share_a_root() {
        assert_eq!(APP_DIR == "Cursed", !crate::channel::IS_DEV);
    }

    #[test]
    fn slugify_is_filesystem_safe() {
        assert_eq!(slugify("Neon / Plasma!!"), "neon-plasma");
        assert_eq!(slugify("   "), "untitled");
        assert_eq!(slugify("../.."), "untitled");
    }
}
