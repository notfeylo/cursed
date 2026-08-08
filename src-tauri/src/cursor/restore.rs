//! First-run safety snapshot and one-click restore (PRD §4.4).
//!
//! The rule this file enforces: Cursed must never be the reason a machine
//! ends up with a pointer nobody asked for. Before the first write, the entire
//! pre-existing scheme is captured verbatim. Restore replays it exactly — and the
//! uninstaller runs the same routine before deleting anything.

use crate::cursor::scheme;
use crate::error::{AppError, AppResult};
use crate::paths;
use crate::util::iso_now;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OriginalScheme {
    /// Registry value name -> value, exactly as read. Empty means "was absent".
    pub values: BTreeMap<String, String>,
    pub cursor_base_size: Option<u32>,
    pub scheme_name: String,
    pub captured_at: String,
}

/// Captures the current scheme if — and only if — nothing has been captured yet.
///
/// Deliberately idempotent. Re-capturing on a later launch would overwrite the
/// user's real defaults with Cursed's own scheme, which would turn "restore"
/// into a no-op and quietly break the product's central promise.
pub fn capture_once() -> AppResult<OriginalScheme> {
    let file = paths::original_scheme_file()?;
    if file.exists() {
        return read_snapshot();
    }

    let (values, cursor_base_size, scheme_name) = scheme::read_all()?;
    let snapshot = OriginalScheme {
        values,
        cursor_base_size,
        scheme_name,
        captured_at: iso_now(),
    };

    let json = serde_json::to_string_pretty(&snapshot)?;
    // Write to a sibling then rename: a half-written snapshot is worse than none.
    let temp = file.with_extension("json.tmp");
    std::fs::write(&temp, json)?;
    std::fs::rename(&temp, &file)?;
    Ok(snapshot)
}

pub fn read_snapshot() -> AppResult<OriginalScheme> {
    let file = paths::original_scheme_file()?;
    let text = std::fs::read_to_string(&file).map_err(|_| {
        AppError::storage("no original scheme snapshot exists yet, so there is nothing to restore")
    })?;
    Ok(serde_json::from_str(crate::util::strip_bom(&text))?)
}

pub fn snapshot_exists() -> bool {
    paths::original_scheme_file()
        .map(|p| p.exists())
        .unwrap_or(false)
}

/// Puts the machine back exactly as Cursed found it.
pub fn restore() -> AppResult<()> {
    let snapshot = read_snapshot()?;
    scheme::write_raw(
        &snapshot.values,
        snapshot.cursor_base_size,
        &snapshot.scheme_name,
    )?;
    // Drop the live overrides too, or the session keeps showing the old pointer
    // until the next sign-in and the restore looks like it did nothing.
    crate::cursor::engine::revert_live()?;
    Ok(())
}

/// Removes every scheme name Cursed registered in the Schemes list, so an
/// uninstall does not leave dropdown entries pointing at deleted files.
pub fn deregister_our_schemes() -> AppResult<()> {
    use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_SET_VALUE};
    use winreg::RegKey;

    let Ok(schemes) = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags(scheme::SCHEMES_KEY, KEY_READ | KEY_SET_VALUE)
    else {
        return Ok(());
    };

    let ours: Vec<String> = schemes
        .enum_values()
        .filter_map(Result::ok)
        .map(|(name, _)| name)
        .filter(|name| {
            name.starts_with(crate::cursor::SCHEME_PREFIX)
                || name.starts_with(crate::cursor::LEGACY_SCHEME_PREFIX)
        })
        .collect();

    for name in ours {
        let _ = schemes.delete_value(&name);
    }
    Ok(())
}
