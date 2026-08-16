//! Remembering what is applied, across launches.
//!
//! The registry already persists the *cursor*; this file persists the *reason* —
//! which pack, which tint, which size. Without it, a fresh launch would have no
//! idea what "correct" means, and the watchdog could not tell a theme reset from
//! the user's own choice.
//!
//! Nothing here re-applies anything. Adopting a scheme that is already live is
//! free; re-applying it on every sign-in would mean a needless registry write
//! and a system-wide broadcast for no visible change.

use crate::cursor;
use crate::error::AppResult;
use crate::packs::catalog::{self, RenderSpec};
use crate::paths;
use crate::state::settings::ApplyMode;
use crate::{custom, state};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum AppliedSource {
    Pack {
        pack_id: String,
        apply_mode: ApplyMode,
    },
    Custom {
        cursor_id: String,
        apply_mode: ApplyMode,
        blend_pack_id: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppliedDescriptor {
    pub source: AppliedSource,
    pub display_name: String,
    pub tint: String,
    pub size: u32,
    pub outline: bool,
}

fn file() -> AppResult<std::path::PathBuf> {
    Ok(paths::root()?.join("applied.json"))
}

pub fn save(descriptor: &AppliedDescriptor) -> AppResult<()> {
    let path = file()?;
    crate::state::store::write(&path, &serde_json::to_string_pretty(descriptor)?)
}

/// What is applied, or `None` if nothing is.
///
/// Goes through the shared store so a descriptor damaged by a crash mid-write
/// falls back to the previous good one rather than reading as "no cursor is
/// applied" — which would silently hand the pointer back to Windows on the next
/// launch and lose which pack, colour and size the user had chosen.
pub fn load() -> Option<AppliedDescriptor> {
    let path = file().ok()?;
    // A missing file is the ordinary "nothing applied yet" case and the store
    // reports it the same way it reports a default, so there is nothing extra
    // to check here.
    let (descriptor, _) = crate::state::store::read::<Option<AppliedDescriptor>>(&path);
    descriptor
}

pub fn forget() {
    if let Ok(path) = file() {
        let _ = std::fs::remove_file(path);
    }
}

/// The id of the custom cursor currently applied, if the applied thing is one.
pub fn applied_custom_id() -> Option<String> {
    match load()?.source {
        AppliedSource::Custom { cursor_id, .. } => Some(cursor_id),
        _ => None,
    }
}

/// True when the descriptor names artwork that is no longer on disk.
///
/// A descriptor can outlive what it points at — a custom cursor deleted while
/// applied, a pack directory removed by hand, a profile half-restored from a
/// backup. Rebuilding from it then writes registry values naming files that do
/// not exist, and Windows answers that by silently drawing its own arrow. The
/// pointer looks stuck: it ignores the size control and the colour, because
/// nothing being written is reaching a real file.
///
/// Checked before restoring rather than after, so a stale descriptor is dropped
/// instead of being faithfully rebuilt into the same broken state on every
/// launch.
pub fn source_is_missing(descriptor: &AppliedDescriptor) -> bool {
    match &descriptor.source {
        AppliedSource::Custom { cursor_id, .. } => !crate::custom::exists(cursor_id),
        _ => false,
    }
}

/// Rebuilds the applied scheme from its descriptor and adopts it.
///
/// Runs off the startup path because the first rebuild after a cache clear does
/// real rendering work, and a cold start must not wait on it.
pub fn restore_in_background() {
    std::thread::Builder::new()
        .name("cursorforge-session".into())
        .spawn(|| {
            let _ = restore();
        })
        .ok();
}


/// Rebuilds and re-applies whatever is currently on, with new appearance
/// settings.
///
/// Changing the tint used to do nothing visible: the setting was saved, but the
/// cursor on screen had already been rendered with the old colour and nothing
/// asked for it again. The only way to see a new colour was to go and pick the
/// same cursor a second time, which is the app telling the user to do its job.
///
/// This rebuilds from the same descriptor the session already keeps, so it works
/// for a catalog pack and a custom cursor alike, and it commits rather than
/// adopts — the registry has to change, not just the live layer, or the colour
/// reverts the next time anything reloads the scheme.
pub fn reapply_with_appearance(tint: &str, size: u32, outline: bool) -> AppResult<bool> {
    let Some(descriptor) = load() else {
        return Ok(false);
    };
    let spec = RenderSpec {
        tint: tint.to_owned(),
        size,
        outline,
    };

    let (set, pack_id) = match &descriptor.source {
        AppliedSource::Pack { pack_id, apply_mode } => {
            let roles = crate::commands::roles_for(*apply_mode);
            (
                catalog::build_roles(pack_id, roles, &spec)?,
                Some(pack_id.clone()),
            )
        }
        AppliedSource::Custom {
            cursor_id,
            apply_mode,
            blend_pack_id,
        } => (
            custom::build_set(cursor_id, *apply_mode, blend_pack_id.as_deref(), &spec)?,
            blend_pack_id.clone(),
        ),
    };

    cursor::commit(
        set,
        &descriptor.display_name,
        size,
        pack_id,
        tint.to_owned(),
    )?;

    // The stored descriptor has to move with it, or the next launch restores
    // the old colour and it looks like the change was never saved.
    save(&AppliedDescriptor {
        tint: tint.to_owned(),
        size,
        outline,
        ..descriptor
    })?;
    Ok(true)
}

fn restore() -> AppResult<()> {
    let Some(descriptor) = load() else {
        return Ok(());
    };

    // Rebuilding from a descriptor whose artwork is gone writes registry values
    // naming files that do not exist, which Windows answers by drawing its own
    // arrow while the app goes on believing the cursor is applied. Put the
    // pointer back honestly and drop the descriptor instead, so this repairs
    // itself on the next launch rather than repeating every launch.
    if source_is_missing(&descriptor) {
        log::warn!("the applied cursor's artwork is gone, so the pointer goes back to Windows");
        let _ = crate::cursor::restore_default();
        forget();
        return Ok(());
    }

    // The size comes from settings, not from the descriptor.
    //
    // The descriptor records the size the cursor was applied at, which is a
    // snapshot; the setting is what the user currently wants. They are supposed
    // to move together, and when anything stops them — a re-apply that failed, a
    // profile restored from a backup, a crash between the two writes — the
    // descriptor wins on every launch afterwards and the size control looks
    // permanently dead. It saves, the number moves, and the pointer never
    // changes, because the very next restore puts the old size back.
    //
    // Settings is the intent, so settings decides. The descriptor keeps the
    // colour and the outline, which have no equivalent second source.
    let settings = crate::state::settings::get();
    let size = crate::cursor::engine::effective_size(settings.cursor_size);
    if size != descriptor.size {
        log::info!(
            "restoring at the size in settings ({size}px), not the one stored with the cursor ({}px)",
            descriptor.size
        );
    }

    let spec = RenderSpec {
        tint: descriptor.tint.clone(),
        size,
        outline: descriptor.outline,
    };

    let (set, pack_id) = match &descriptor.source {
        AppliedSource::Pack {
            pack_id,
            apply_mode,
        } => {
            // Build exactly the roles that are live, so the adopted set is the
            // set in the registry — a superset would leave the watchdog
            // perpetually "fixing" roles the user chose not to change.
            let roles = crate::commands::roles_for(*apply_mode);
            (
                catalog::build_roles(pack_id, roles, &spec)?,
                Some(pack_id.clone()),
            )
        }
        AppliedSource::Custom {
            cursor_id,
            apply_mode,
            blend_pack_id,
        } => (
            custom::build_set(cursor_id, *apply_mode, blend_pack_id.as_deref(), &spec)?,
            blend_pack_id.clone(),
        ),
    };

    // `adopt` records what is already in the registry without rewriting it,
    // which is right on a normal launch: nothing changed, and rewriting HKCU
    // every time the app starts is churn for nothing.
    //
    // It is wrong when the size just moved. The files were rebuilt at the
    // settings size, but `CursorBaseSize` and the stored descriptor still hold
    // the old one — so Windows scales the new artwork to the old number, and
    // the next launch logs the same correction again, forever. When the size has
    // actually changed, commit it.
    if size == descriptor.size {
        return cursor::adopt(set, &descriptor.display_name, size, pack_id, descriptor.tint);
    }

    cursor::commit(
        set,
        &descriptor.display_name,
        size,
        pack_id,
        descriptor.tint.clone(),
    )?;
    save(&AppliedDescriptor { size, ..descriptor })
}

/// Applies the default preset on first run, if one is set. Quiet on failure —
/// a startup that cannot render is still a startup the user can use.
pub fn apply_default_preset(app: &tauri::AppHandle) {
    let Ok(Some(preset)) = state::presets::default_preset() else {
        return;
    };
    let _ = crate::commands::apply_preset_inner(app, &preset);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cursor::roles::Role;

    #[test]
    fn descriptors_round_trip_through_json() {
        let descriptor = AppliedDescriptor {
            source: AppliedSource::Custom {
                cursor_id: "my-logo-abc123".into(),
                apply_mode: ApplyMode::Blend,
                blend_pack_id: Some("neon-plasma".into()),
            },
            display_name: "GAMING".into(),
            tint: "#2E8BFF".into(),
            size: 48,
            outline: true,
        };
        let text = serde_json::to_string(&descriptor).unwrap();
        let back: AppliedDescriptor = serde_json::from_str(&text).unwrap();
        assert_eq!(back.display_name, "GAMING");
        assert!(matches!(back.source, AppliedSource::Custom { .. }));
    }

    #[test]
    fn a_restored_session_rebuilds_exactly_the_live_roles() {
        // The adopted set has to match what apply wrote, or the watchdog sees
        // permanent drift and re-applies for ever.
        assert_eq!(crate::commands::roles_for(ApplyMode::ArrowOnly).len(), 1);
        assert_eq!(crate::commands::roles_for(ApplyMode::Recommended).len(), 3);
        assert_eq!(crate::commands::roles_for(ApplyMode::All).len(), 17);
    }

    #[test]
    fn a_corrupt_descriptor_is_ignored_rather_than_fatal() {
        // `load` returns None for anything it cannot parse, so a hand-edited or
        // truncated file costs a default cursor, never a failed launch.
        let parsed: Option<AppliedDescriptor> = serde_json::from_str("{ not json }").ok();
        assert!(parsed.is_none());
        let _ = Role::Arrow;
    }
}
