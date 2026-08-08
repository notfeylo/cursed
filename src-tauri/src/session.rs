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
    let temp = path.with_extension("json.tmp");
    std::fs::write(&temp, serde_json::to_string_pretty(descriptor)?)?;
    std::fs::rename(&temp, &path)?;
    Ok(())
}

pub fn load() -> Option<AppliedDescriptor> {
    let path = file().ok()?;
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(crate::util::strip_bom(&text)).ok()
}

pub fn forget() {
    if let Ok(path) = file() {
        let _ = std::fs::remove_file(path);
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

fn restore() -> AppResult<()> {
    let Some(descriptor) = load() else {
        return Ok(());
    };
    let spec = RenderSpec {
        tint: descriptor.tint.clone(),
        size: descriptor.size,
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

    cursor::adopt(
        set,
        &descriptor.display_name,
        descriptor.size,
        pack_id,
        descriptor.tint,
    )
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
