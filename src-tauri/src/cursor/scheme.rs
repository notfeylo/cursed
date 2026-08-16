//! Layer B — the persistence layer (PRD §4.2).
//!
//! Writes the pointer scheme into `HKEY_CURRENT_USER\Control Panel\Cursors` so
//! the cursor survives a reboot, survives Cursed not running at all, and
//! shows up as a named scheme in Control Panel → Mouse → Pointers.
//!
//! HKCU only. No administrator rights are involved anywhere in this file.

use crate::cursor::roles::{Role, ALL_ROLES};
use crate::error::{AppError, AppResult};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_SET_VALUE, REG_DWORD, REG_EXPAND_SZ, REG_SZ};
use winreg::{RegKey, RegValue};

pub const CURSORS_KEY: &str = r"Control Panel\Cursors";
pub const SCHEMES_KEY: &str = r"Control Panel\Cursors\Schemes";

/// A complete pointer scheme: one file per role, plus presentation metadata.
#[derive(Debug, Clone, Default)]
pub struct CursorSet {
    pub files: BTreeMap<Role, PathBuf>,
}

impl CursorSet {
    pub fn insert(&mut self, role: Role, path: PathBuf) {
        self.files.insert(role, path);
    }

    pub fn get(&self, role: Role) -> Option<&Path> {
        self.files.get(&role).map(PathBuf::as_path)
    }

    pub fn is_complete(&self) -> bool {
        ALL_ROLES.iter().all(|r| self.files.contains_key(r))
    }

    /// Names the roles a pack forgot. The build script and `.cfpack` import both
    /// refuse an incomplete set — a half-applied scheme is exactly the failure
    /// mode Cursed exists to avoid (PRD §4.2).
    pub fn missing(&self) -> Vec<Role> {
        ALL_ROLES
            .into_iter()
            .filter(|r| !self.files.contains_key(r))
            .collect()
    }
}

fn cursors_key(access: u32) -> AppResult<RegKey> {
    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags(CURSORS_KEY, access)
        .map_err(|e| AppError::storage(format!("Control Panel\\Cursors is unreadable: {e}")))
}

/// Rewrites an absolute path under `%APPDATA%` back into its expandable form.
/// `REG_EXPAND_SZ` with `%APPDATA%` keeps working if the profile is relocated,
/// which a hardcoded `C:\Users\...` string would not.
fn to_expandable(path: &Path) -> String {
    let text = path.to_string_lossy().replace('/', "\\");
    if let Some(appdata) = std::env::var_os("APPDATA") {
        let appdata = appdata.to_string_lossy().replace('/', "\\");
        if !appdata.is_empty() {
            if let Some(rest) = text
                .strip_prefix(&appdata)
                .or_else(|| text.strip_prefix(appdata.trim_end_matches('\\')))
            {
                return format!("%APPDATA%{}", rest);
            }
        }
    }
    text
}

fn expand_sz(value: &str) -> RegValue {
    let mut bytes: Vec<u8> = value
        .encode_utf16()
        .flat_map(|unit| unit.to_le_bytes())
        .collect();
    bytes.extend_from_slice(&[0, 0]); // terminating NUL, in UTF-16
    RegValue {
        vtype: REG_EXPAND_SZ,
        bytes,
    }
}

/// Writes the scheme, names it, registers it in the Schemes list, and broadcasts
/// the change so every running process picks it up immediately.
///
/// A role the set does not define has its value **deleted**, which is how
/// Windows spells "use the built-in cursor for this role". That is what makes
/// "apply to the arrow only" mean the arrow only, instead of leaving fifteen
/// stale paths behind pointing at a previous pack.
///
/// Catalog packs are required to define all seventeen — that check lives at the
/// point a pack is built, where a missing role is a build failure rather than a
/// user choice (PRD §4.2).
pub fn write(set: &CursorSet, scheme_name: &str) -> AppResult<()> {
    if set.files.is_empty() {
        return Err(AppError::invalid("a scheme with no roles would do nothing"));
    }

    let key = cursors_key(KEY_SET_VALUE | KEY_READ)?;
    for role in ALL_ROLES {
        match set.get(role) {
            Some(path) => key
                .set_raw_value(role.registry_value(), &expand_sz(&to_expandable(path)))
                .map_err(|e| AppError::storage(format!("could not set {role}: {e}")))?,
            None => {
                let _ = key.delete_value(role.registry_value());
            }
        }
    }

    key.set_value("", &scheme_name.to_owned())?;
    // 2 = "a scheme from the Schemes list is active" — this is what makes the
    // Pointers tab show our name instead of "(Modified)".
    key.set_value("Scheme Source", &2u32)?;

    register_scheme(set, scheme_name)?;
    broadcast()?;
    Ok(())
}

/// Adds the scheme to `Control Panel\Cursors\Schemes` so it appears in the
/// Windows Pointers dropdown as a first-class, selectable scheme.
fn register_scheme(set: &CursorSet, scheme_name: &str) -> AppResult<()> {
    let (schemes, _) = RegKey::predef(HKEY_CURRENT_USER)
        .create_subkey(SCHEMES_KEY)
        .map_err(|e| AppError::storage(format!("could not open the Schemes key: {e}")))?;

    // Windows expects the 17 paths in exactly this order, semicolon separated.
    let joined = ALL_ROLES
        .into_iter()
        .map(|role| set.get(role).map(to_expandable).unwrap_or_default())
        .collect::<Vec<_>>()
        .join(";");

    schemes.set_raw_value(scheme_name, &expand_sz(&joined))?;
    Ok(())
}

pub fn remove_registered_scheme(scheme_name: &str) -> AppResult<()> {
    if let Ok(schemes) =
        RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(SCHEMES_KEY, KEY_SET_VALUE)
    {
        // Absent is the desired end state, so a "not found" is a success.
        let _ = schemes.delete_value(scheme_name);
    }
    Ok(())
}

/// Reads one role's current registry value, unexpanded env vars and all.
pub fn read_role(role: Role) -> AppResult<String> {
    let key = cursors_key(KEY_READ)?;
    match key.get_value::<String, _>(role.registry_value()) {
        Ok(value) => Ok(value),
        // An absent value means "Windows default", which is a real state, not an error.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(AppError::storage(e.to_string())),
    }
}

/// The active scheme's display name, as shown in the Windows Pointers tab.
pub fn read_scheme_name() -> AppResult<String> {
    let key = cursors_key(KEY_READ)?;
    Ok(key.get_value::<String, _>("").unwrap_or_default())
}

/// Snapshot of every value Cursed is capable of changing.
pub fn read_all() -> AppResult<(BTreeMap<String, String>, Option<u32>, String)> {
    let key = cursors_key(KEY_READ)?;
    let mut values = BTreeMap::new();
    for role in ALL_ROLES {
        let name = role.registry_value();
        let value = key.get_value::<String, _>(name).unwrap_or_default();
        values.insert(name.to_owned(), value);
    }
    let base_size = key.get_value::<u32, _>("CursorBaseSize").ok();
    let scheme_name = key.get_value::<String, _>("").unwrap_or_default();
    Ok((values, base_size, scheme_name))
}

/// Restores an exact snapshot. An empty string means "the value was absent",
/// so we delete rather than write an empty path — a dangling empty value would
/// leave the Pointers tab in a state Windows itself never produces.
pub fn write_raw(
    values: &BTreeMap<String, String>,
    base_size: Option<u32>,
    scheme_name: &str,
) -> AppResult<()> {
    let key = cursors_key(KEY_SET_VALUE | KEY_READ)?;
    for role in ALL_ROLES {
        let name = role.registry_value();
        match values.get(name) {
            Some(value) if !value.is_empty() => {
                key.set_raw_value(name, &expand_sz(value))?;
            }
            _ => {
                let _ = key.delete_value(name);
            }
        }
    }

    match base_size {
        Some(size) => key.set_value("CursorBaseSize", &size)?,
        None => {
            let _ = key.delete_value("CursorBaseSize");
        }
    }

    if scheme_name.is_empty() {
        let _ = key.delete_value("");
        let _ = key.delete_value("Scheme Source");
    } else {
        key.set_raw_value(
            "",
            &RegValue {
                vtype: REG_SZ,
                bytes: scheme_name
                    .encode_utf16()
                    .flat_map(|u| u.to_le_bytes())
                    .chain([0, 0])
                    .collect(),
            },
        )?;
    }

    broadcast()
}

/// `CursorBaseSize` is what the Windows accessibility cursor-size slider writes.
/// We read it so a fresh apply matches whatever the user already chose, and we
/// write it so our own slider replaces that trip to the Settings app entirely.
pub fn read_base_size() -> AppResult<u32> {
    let key = cursors_key(KEY_READ)?;
    Ok(key.get_value::<u32, _>("CursorBaseSize").unwrap_or(32))
}

pub fn write_base_size(size: u32) -> AppResult<()> {
    let size = size.clamp(32, 256);
    let key = cursors_key(KEY_SET_VALUE | KEY_READ)?;
    key.set_raw_value(
        "CursorBaseSize",
        &RegValue {
            vtype: REG_DWORD,
            bytes: size.to_le_bytes().to_vec(),
        },
    )?;
    broadcast()
}

/// `SPI_SETCURSORS` with `SPIF_UPDATEINIFILE | SPIF_SENDCHANGE`: persist, then
/// tell every window the pointer scheme changed.
///
/// **Never fails.** This is a notification, not the change itself — by the time
/// it runs the registry already holds the new scheme and the cursor is already
/// correct. `SystemParametersInfoW` can return FALSE without setting an error
/// code (a busy desktop, a session in an odd state), and treating that as a
/// failed apply put "Windows rejected the request" on screen over an apply that
/// had in fact worked.
///
/// A broadcast that does not land costs a redraw, not a wrong cursor.
pub fn broadcast() -> AppResult<()> {
    use windows::Win32::UI::WindowsAndMessaging::{
        SystemParametersInfoW, SPIF_SENDCHANGE, SPIF_UPDATEINIFILE, SPI_SETCURSORS,
    };
    // SAFETY: no pointer is passed; all arguments are plain values, and the call
    // only touches the calling user's own settings.
    let result = unsafe {
        SystemParametersInfoW(
            SPI_SETCURSORS,
            0,
            None,
            SPIF_UPDATEINIFILE | SPIF_SENDCHANGE,
        )
    };
    if let Err(e) = result {
        log::debug!(
            "the scheme was written but the change broadcast did not land: {}",
            crate::error::describe_win32(&e)
        );
    }
    Ok(())
}

/// Same reasoning as [`broadcast`]: reverting a hover preview is housekeeping,
/// and a failure here must not surface as an error over a catalog someone is
/// browsing.
///
/// Drops the in-session `SetSystemCursor` overrides by making Windows reload
/// every cursor from the registry (PRD §4.1).
pub fn reload_from_registry() -> AppResult<()> {
    use windows::Win32::UI::WindowsAndMessaging::{
        SystemParametersInfoW, SPI_SETCURSORS, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
    };
    // SAFETY: as above — value arguments only. No SPIF flags, so nothing is
    // written to disk and no broadcast storm is triggered for a hover preview.
    let result = unsafe {
        SystemParametersInfoW(
            SPI_SETCURSORS,
            0,
            None,
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    };
    if let Err(e) = result {
        log::debug!(
            "cursors could not be reloaded from the registry: {}",
            crate::error::describe_win32(&e)
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand_sz_is_utf16_and_nul_terminated() {
        let value = expand_sz("AB");
        assert_eq!(value.vtype, REG_EXPAND_SZ);
        assert_eq!(value.bytes, vec![b'A', 0, b'B', 0, 0, 0]);
    }

    #[test]
    fn appdata_prefix_becomes_a_variable() {
        let Some(appdata) = std::env::var_os("APPDATA") else {
            return;
        };
        let path = PathBuf::from(appdata).join("Cursed").join("a.cur");
        assert_eq!(to_expandable(&path), r"%APPDATA%\Cursed\a.cur");
    }

    /// A broadcast is a notification about a change that has already happened.
    /// If it could fail the caller, a successful apply would report itself as an
    /// error — which is exactly the bug this guards against.
    #[test]
    fn broadcasting_never_fails_the_caller() {
        assert!(broadcast().is_ok());
        assert!(reload_from_registry().is_ok());
    }

    /// Same trap as the data folder: if the legacy prefix equals the current
    /// one, an uninstall cleans up nothing from older versions. The dev channel
    /// inherits no prefix at all, and so has nothing to compare.
    #[test]
    fn the_legacy_scheme_prefix_is_not_the_current_one() {
        if let Some(legacy) = crate::cursor::legacy_scheme_prefix() {
            assert_ne!(
                crate::cursor::SCHEME_PREFIX,
                legacy,
                "old schemes would never be cleaned up"
            );
        }
    }

    /// The two channels must not register schemes under one prefix. Cleanup
    /// matches by prefix, so a shared one means uninstalling either channel
    /// strips the other's saved schemes out of the Windows Pointers dropdown.
    #[test]
    fn each_channel_has_its_own_scheme_prefix() {
        assert_eq!(
            crate::cursor::SCHEME_PREFIX == "Cursed — ",
            !crate::channel::IS_DEV
        );
    }

    #[test]
    fn incomplete_sets_are_reported_precisely() {
        let mut set = CursorSet::default();
        set.insert(Role::Arrow, PathBuf::from("a.cur"));
        assert!(!set.is_complete());
        assert_eq!(set.missing().len(), 16);
        assert!(!set.missing().contains(&Role::Arrow));
    }
}
