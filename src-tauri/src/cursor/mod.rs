//! The cursor engine.
//!
//! Three layers, in the order they matter:
//!   A. [`engine`]   — `SetSystemCursor`, instant, session-only.
//!   B. [`scheme`]   — `HKCU\Control Panel\Cursors`, survives reboot.
//!   C. [`watchdog`] — notices when something else stomps on B, and re-applies.
//!
//! Plus [`restore`], which guarantees we can always hand the machine back, and
//! [`crosschannel`], which decides which of two installed channels is allowed to
//! defend the scheme — there being only one of it per Windows user.

pub mod crosschannel;
pub mod engine;
pub mod restore;
pub mod roles;
pub mod scheme;
pub mod watchdog;

use crate::error::AppResult;
use scheme::CursorSet;
use serde::Serialize;
use std::sync::{Mutex, OnceLock};

/// Every scheme we register is named with this prefix so an uninstall can find
/// and remove exactly our entries and nobody else's.
///
/// Per channel, for that same reason: the cleanup matches by prefix, so two
/// channels sharing one prefix would mean uninstalling either strips the other's
/// saved schemes out of the Windows Pointers dropdown.
pub const SCHEME_PREFIX: &str = crate::channel::SCHEME_PREFIX;

/// The prefix used before the app was renamed, when this channel inherits it.
///
/// Kept so an uninstall still cleans up schemes registered by an earlier
/// version. Leaving those behind would put dead entries in the Windows Pointers
/// dropdown pointing at files that no longer exist. `None` on the dev channel,
/// which never carried the old name and must not clean up after the one that
/// did.
pub fn legacy_scheme_prefix() -> Option<&'static str> {
    crate::channel::legacy_scheme_prefix()
}

/// What is currently committed. Held in memory so the watchdog knows what
/// "correct" looks like without re-deriving it from disk every five seconds.
#[derive(Debug, Clone)]
pub struct Applied {
    pub set: CursorSet,
    pub scheme_name: String,
    pub size: u32,
    pub pack_id: Option<String>,
    pub pack_name: Option<String>,
    pub tint: String,
}

fn applied_slot() -> &'static Mutex<Option<Applied>> {
    static APPLIED: OnceLock<Mutex<Option<Applied>>> = OnceLock::new();
    APPLIED.get_or_init(|| Mutex::new(None))
}

pub fn applied() -> Option<Applied> {
    applied_slot().lock().ok().and_then(|guard| guard.clone())
}

fn set_applied(value: Option<Applied>) {
    if let Ok(mut guard) = applied_slot().lock() {
        *guard = value;
    }
}

/// Shown on HOME and in the tray tooltip.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActiveState {
    pub pack_id: Option<String>,
    pub pack_name: Option<String>,
    pub tint: String,
    pub size: u32,
    pub is_default: bool,
}

pub fn active_state() -> AppResult<ActiveState> {
    let size = scheme::read_base_size().unwrap_or(32);
    Ok(match applied() {
        Some(state) => ActiveState {
            pack_id: state.pack_id,
            pack_name: state.pack_name,
            tint: state.tint,
            size: state.size,
            is_default: false,
        },
        None => {
            // Nothing applied this session — but a previous session may have
            // written a scheme that is still live. Read the scheme's own name
            // back rather than showing a placeholder: the registry knows it is
            // "PLASMA", and telling the user anything vaguer is just noise.
            let name = scheme::read_scheme_name().unwrap_or_default();
            // An older version's scheme is still ours, and still what the user
            // is looking at, so the chip should name it rather than claim the
            // pointer is stock.
            let stripped = name
                .strip_prefix(SCHEME_PREFIX)
                .or_else(|| legacy_scheme_prefix().and_then(|old| name.strip_prefix(old)));
            match stripped {
                Some(pack_name) if !pack_name.is_empty() => ActiveState {
                    pack_id: None,
                    pack_name: Some(pack_name.to_owned()),
                    tint: "#2E8BFF".to_owned(),
                    size,
                    is_default: false,
                },
                _ => ActiveState {
                    pack_id: None,
                    pack_name: None,
                    tint: "#2E8BFF".to_owned(),
                    size,
                    is_default: true,
                },
            }
        }
    })
}

/// Commits a scheme: registry first, so the choice is durable, then the live
/// layer, so it is visible immediately.
///
/// The order of the last two steps matters. Once the registry write has
/// succeeded the choice *has* been made, so it is recorded before the live layer
/// is attempted. A live-layer problem is worth reporting, but it must never
/// cause us to forget what is now sitting in the registry — that would leave the
/// cursor changed with nothing tracking it, no persistence across launches, and
/// a watchdog with nothing to protect.
pub fn commit(
    set: CursorSet,
    display_name: &str,
    size: u32,
    pack_id: Option<String>,
    tint: String,
) -> AppResult<()> {
    let scheme_name = format!("{SCHEME_PREFIX}{display_name}");
    scheme::write(&set, &scheme_name)?;
    scheme::write_base_size(size)?;

    // A role Windows refuses as a live override is still committed to the
    // registry and still correct after the next reload, so this is a warning
    // about *when* the pointer changes, not whether. Failing the command here
    // would report a durable, successful change as an error.
    if let Err(e) = engine::apply_live(&set, size) {
        log::warn!("scheme committed, but the in-session override was partial: {e}");
    }

    set_applied(Some(Applied {
        set,
        scheme_name,
        size,
        pack_id,
        pack_name: Some(display_name.to_owned()),
        tint,
    }));
    Ok(())
}

/// Adopts an already-live scheme as the current one **without writing anything**.
///
/// Used at startup: the registry already holds last session's scheme, so there
/// is nothing to apply — but the watchdog needs to know what "correct" looks
/// like, and re-applying on every launch would mean a pointless registry write
/// and broadcast every time the user signs in.
pub fn adopt(
    set: CursorSet,
    display_name: &str,
    size: u32,
    pack_id: Option<String>,
    tint: String,
) -> AppResult<()> {
    set_applied(Some(Applied {
        set,
        scheme_name: format!("{SCHEME_PREFIX}{display_name}"),
        size,
        pack_id,
        pack_name: Some(display_name.to_owned()),
        tint,
    }));
    Ok(())
}

/// Live layer only — no registry write, no broadcast. This is what catalog
/// hover uses, and it is why hovering costs nothing and reverts cleanly.
pub fn preview(set: &CursorSet, size: u32) -> AppResult<()> {
    engine::apply_live(set, size)
}

/// Drops a hover preview. Reloading from the registry restores whatever is
/// actually committed, whether that is one of our schemes or Windows' own.
pub fn clear_preview() -> AppResult<()> {
    engine::revert_live()
}

/// Returns the machine to its pre-Cursed state.
pub fn restore_default() -> AppResult<()> {
    restore::restore()?;
    set_applied(None);
    Ok(())
}

/// Re-applies the committed scheme. Used by the watchdog and by resume-from-sleep.
pub fn reapply() -> AppResult<bool> {
    let Some(state) = applied() else {
        return Ok(false);
    };
    scheme::write(&state.set, &state.scheme_name)?;
    engine::apply_live(&state.set, state.size)?;
    Ok(true)
}

/// The last three components of a cursor path, lowercased, for comparison.
///
/// The registry stores an unexpanded `%APPDATA%\...` string, so the head of a
/// path cannot be compared without expanding it first. The tail can be, and
/// three components is what it takes to identify one of ours:
///
/// ```text
/// cache\<pack>\<tint-outline>\Arrow.cur     a catalog pack
/// custom\<cursor-id>\32.ani                 a cursor built from an image
/// imported\<slug>\arrow.cur                 a downloaded pack
/// ```
///
/// The file name alone is not enough, and quietly was not for a long time. Every
/// pack writes the same seventeen names, so `Arrow.cur` matched `Arrow.cur`
/// whichever pack it came out of — which means the thing the watchdog exists to
/// catch, our scheme being swapped wholesale for another set of files, was the
/// one thing it could not see. It only ever noticed a role reset to a *stock
/// Windows* cursor, those being the only ones whose names differ.
///
/// Two components is not enough either: the appearance directory is shared
/// across packs, so `v2-666666-o\Arrow.cur` is the same string for every one of
/// them. The pack is the third component, and it is the one that matters.
fn path_tail(text: &str) -> String {
    let mut parts: Vec<&str> = text
        .rsplit(['\\', '/'])
        .filter(|part| !part.is_empty())
        .take(3)
        .collect();
    parts.reverse();
    parts.join("\\").to_ascii_lowercase()
}

/// One pointer role, as Windows currently has it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoleAudit {
    pub role: String,
    /// The registry value, unexpanded — `%APPDATA%\...` as Windows stores it.
    pub value: String,
    /// Where that resolves to. Empty when the value is absent.
    pub resolved: String,
    /// Absent means "Windows draws its own", which is a real state and not a
    /// fault.
    pub set: bool,
    pub exists: bool,
    /// What the first bytes say it is: `cur`, `ani`, or a complaint.
    pub format: String,
    pub ok: bool,
}

/// Reads all seventeen roles and checks each one end to end.
///
/// **This is where a role that "does not follow in a browser" is actually
/// diagnosed.** The instinct is to blame the application — Firefox does its own
/// thing, Chrome ignores this, and so on — and that instinct is nearly always
/// wrong. A pointer role that fails to change is overwhelmingly a malformed or
/// missing entry among these seventeen: a path Windows cannot resolve, a file
/// the uninstaller deleted, a `.cur` with the wrong `idType`, an `.ani` that is
/// not a RIFF. Windows silently falls back to its own cursor for any of those,
/// with no error anywhere, which looks exactly like an application refusing to
/// cooperate.
///
/// So this checks the thing that can be checked before anybody starts
/// screenshotting browsers.
pub fn audit_roles() -> Vec<RoleAudit> {
    roles::ALL_ROLES
        .into_iter()
        .map(|role| {
            let value = scheme::read_role(role).unwrap_or_default();
            if value.is_empty() {
                return RoleAudit {
                    role: role.to_string(),
                    value,
                    resolved: String::new(),
                    set: false,
                    exists: false,
                    format: "windows default".to_owned(),
                    // Not a fault. An unset role is Windows drawing its own, and
                    // "arrow only" mode deliberately leaves sixteen like this.
                    ok: true,
                };
            }

            let resolved = crate::util::expand_env(&value);
            let path = std::path::Path::new(&resolved);
            let exists = path.is_file();
            let format = if exists {
                describe_cursor_file(path)
            } else {
                "missing".to_owned()
            };
            let ok = exists && (format == "cur" || format == "ani");

            RoleAudit {
                role: role.to_string(),
                value,
                resolved,
                set: true,
                exists,
                format,
                ok,
            }
        })
        .collect()
}

/// What a file's first bytes say it is.
///
/// Read rather than inferred from the extension: a `.cur` that is really a PNG
/// loads as nothing, and the extension is the one part of the file that cannot
/// be wrong in a way Windows notices.
fn describe_cursor_file(path: &std::path::Path) -> String {
    let Ok(bytes) = std::fs::read(path) else {
        return "unreadable".to_owned();
    };
    if bytes.len() < 12 {
        return "too short".to_owned();
    }
    // `.ani` is RIFF with an ACON form type at offset 8.
    if &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"ACON" {
        return "ani".to_owned();
    }
    // `.cur` is an ICONDIR: reserved 0, type 2 (1 would be an icon), count > 0.
    let reserved = u16::from_le_bytes([bytes[0], bytes[1]]);
    let kind = u16::from_le_bytes([bytes[2], bytes[3]]);
    let count = u16::from_le_bytes([bytes[4], bytes[5]]);
    if reserved == 0 && kind == 2 && count > 0 {
        return "cur".to_owned();
    }
    if reserved == 0 && kind == 1 {
        // Windows will not use an icon as a cursor: it has no hotspot.
        return "an icon, not a cursor".to_owned();
    }
    "not a cursor file".to_owned()
}

/// True when the registry no longer reflects what we committed — i.e. a theme
/// change, a personalisation reset, or another cursor tool has overwritten us.
pub fn drifted() -> bool {
    let Some(state) = applied() else {
        return false;
    };

    // Every role, not only the arrow.
    //
    // Watching the arrow alone missed a whole class of reset: something restores
    // the busy and working pointers to the Windows defaults and leaves the arrow
    // untouched, so the watchdog sees nothing wrong. The user then gets the stock
    // spinner every time the machine is busy — which is precisely when the
    // pointer is most conspicuous.
    //
    // It is a handful of registry string reads every few seconds, which is not a
    // cost worth trading correctness for.
    for (role, expected) in &state.set.files {
        let Ok(current) = scheme::read_role(*role) else {
            continue;
        };
        let expected_tail = path_tail(&expected.to_string_lossy());
        if expected_tail.is_empty() {
            continue;
        }
        if path_tail(&current) != expected_tail {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The registry's `%APPDATA%` form and our absolute form must compare equal,
    /// or the watchdog reads its own correct scheme as drift and re-applies it
    /// every few seconds for ever — which, on an animated cursor, restarts the
    /// animation on every pass.
    #[test]
    fn the_same_cursor_written_two_ways_is_not_drift() {
        assert_eq!(
            path_tail(r"%APPDATA%\Cursed\cache\pack\v2-666666-o\Arrow.cur"),
            path_tail(r"C:\Users\someone\AppData\Roaming\Cursed\cache\pack\v2-666666-o\Arrow.cur")
        );
        // Separator and case are both formatting, not identity.
        assert_eq!(path_tail("a/b/c/Arrow.cur"), path_tail(r"a\b\c\arrow.CUR"));
    }

    /// Every pack writes the same seventeen file names, so the name alone cannot
    /// tell two cursors apart. This is the comparison that can.
    #[test]
    fn two_packs_sharing_a_file_name_are_told_apart() {
        assert_ne!(
            path_tail(r"%APPDATA%\Cursed\cache\pack-a\v2-666666-o\Arrow.cur"),
            path_tail(r"%APPDATA%\Cursed\cache\pack-b\v2-666666-o\Arrow.cur"),
            "a different pack is a different cursor"
        );
        assert_ne!(
            path_tail(r"cache\pack\v2-666666-o\Arrow.cur"),
            path_tail(r"cache\pack\v2-2e8bff-o\Arrow.cur"),
            "a different colour is a different cursor"
        );
        assert_ne!(
            path_tail(r"custom\trump-5b6b27cc\32.ani"),
            path_tail(r"custom\elon-ad9854ad\32.ani"),
            "a different custom cursor is a different cursor"
        );
        // And a stock Windows cursor, which is what a reset leaves behind.
        assert_ne!(
            path_tail(r"%SystemRoot%\cursors\aero_arrow.cur"),
            path_tail(r"%APPDATA%\Cursed\cache\pack\v2-666666-o\Arrow.cur")
        );
    }

    /// A short path has no third component to take, and must not panic reaching
    /// for one.
    #[test]
    fn a_path_with_no_parent_is_still_comparable() {
        assert_eq!(path_tail("Arrow.cur"), "arrow.cur");
        assert_eq!(path_tail(r"pack\Arrow.cur"), r"pack\arrow.cur");
        assert_eq!(path_tail(""), "");
        // A trailing separator must not swallow the file name.
        assert_eq!(path_tail(r"a\b\c\"), r"a\b\c");
    }
}
