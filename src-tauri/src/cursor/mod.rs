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
pub const SCHEME_PREFIX: &str = "CursorForge — ";

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
            // written a scheme that is still live. Read it back rather than
            // claiming the pointer is stock.
            let current = scheme::read_role(roles::Role::Arrow).unwrap_or_default();
            let ours = current.to_lowercase().contains("cursorforge");
            ActiveState {
                pack_id: None,
                pack_name: ours.then(|| "RESTORED".to_owned()),
                tint: "#2E8BFF".to_owned(),
                size,
                is_default: !ours,
            }
        }
    })
}

/// Commits a scheme: registry first (so it is durable even if the live layer
/// fails), then the live layer (so the change is visible immediately).
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
    engine::apply_live(&set, size)?;

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

/// Returns the machine to its pre-CursorForge state.
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
    let Some(expected) = state.set.get(roles::Role::Arrow) else {
        return false;
    };
    let Ok(current) = scheme::read_role(roles::Role::Arrow) else {
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
    let expected_name = expected
        .file_name()
        .map(|n| n.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();

    file_name(&current) != expected_name
}
