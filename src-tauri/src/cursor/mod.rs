//! The cursor engine.
//!
//! Three layers, in the order they matter:
//!   A. [`engine`]   — `SetSystemCursor`, instant, session-only.
//!   B. [`scheme`]   — `HKCU\Control Panel\Cursors`, survives reboot.
//!   C. [`watchdog`] — notices when something else stomps on B, and re-applies.
//!
//! Plus [`restore`], which guarantees we can always hand the machine back.

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
pub const SCHEME_PREFIX: &str = "Cursed — ";

/// The prefix used before the app was renamed.
///
/// Kept so an uninstall still cleans up schemes registered by an earlier
/// version. Leaving those behind would put dead entries in the Windows Pointers
/// dropdown pointing at files that no longer exist.
///
/// Must stay the *old* name — if it equals `SCHEME_PREFIX` it cleans up nothing.
pub const LEGACY_SCHEME_PREFIX: &str = "CursorForge — ";

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
                .or_else(|| name.strip_prefix(LEGACY_SCHEME_PREFIX));
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

/// True when the registry no longer reflects what we committed — i.e. a theme
/// change, a personalisation reset, or another cursor tool has overwritten us.
pub fn drifted() -> bool {
    let Some(state) = applied() else {
        return false;
    };
    // The registry stores an unexpanded `%APPDATA%\...` string; compare on the
    // file name, which is stable across expansion and path formatting.
    let file_name = |text: &str| {
        text.rsplit(['\\', '/'])
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase()
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
        let expected_name = expected
            .file_name()
            .map(|n| n.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        if expected_name.is_empty() {
            continue;
        }
        if file_name(&current) != expected_name {
            return true;
        }
    }
    false
}
