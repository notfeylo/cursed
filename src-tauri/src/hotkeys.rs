//! Global shortcuts (PRD §9).
//!
//! Registration is all-or-nothing per accelerator: a shortcut another
//! application already owns is skipped rather than fought over, and the rest
//! still register. Silently losing every hotkey because one clashed is the
//! failure mode worth avoiding.

use crate::error::{AppError, AppResult};
use crate::state::presets;
use crate::state::settings::Settings;
use std::str::FromStr;
use tauri::AppHandle;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

/// Registers every configured shortcut, replacing any previous registration.
pub fn register(app: &AppHandle, settings: &Settings) -> AppResult<()> {
    let manager = app.global_shortcut();
    let _ = manager.unregister_all();

    // A shortcut belongs to one process per session, first come first served.
    // With both channels installed, whichever launched first would take
    // `Ctrl+Alt+1` and the other would get nothing — no error, no log line, just
    // a hotkey that does not work. The dev channel therefore does not ask, so
    // the copy standing in for a real install behaves exactly as a stranger's
    // would while it is being developed beside.
    if !crate::channel::claims_hotkeys_by_default() {
        log::info!(
            "hotkeys: the {} channel yields global shortcuts to the released app",
            crate::channel::NAME
        );
        return Ok(());
    }

    let mut bindings: Vec<(String, Action)> = vec![
        (settings.hotkey_toggle.clone(), Action::Toggle),
        (settings.hotkey_open.clone(), Action::Open),
    ];
    for (index, accelerator) in settings.hotkey_presets.iter().enumerate() {
        bindings.push((accelerator.clone(), Action::PresetSlot(index)));
    }

    for (accelerator, action) in bindings {
        if accelerator.trim().is_empty() {
            continue;
        }
        let Ok(shortcut) = Shortcut::from_str(&accelerator) else {
            continue; // an unparseable accelerator is a settings typo, not a crash
        };
        let _ = manager.on_shortcut(shortcut, move |app, _shortcut, event| {
            // Fire on press only; without this every hotkey fires twice.
            if event.state() == ShortcutState::Pressed {
                action.run(app);
            }
        });
    }
    Ok(())
}

/// Whether this build can actually register that key combination.
///
/// `register` skips an accelerator it cannot parse and carries on, which is the
/// right behaviour there — one bad entry in a settings file must not cost the
/// other six shortcuts. It is the wrong thing to do to somebody who has just
/// pressed a key combination and is watching to see whether it took: they get a
/// binding that is displayed, saved, and does nothing.
///
/// So the UI asks first, through the same parser that will do the registering.
/// There is no second implementation to disagree with.
pub fn is_registerable(accelerator: &str) -> bool {
    !accelerator.trim().is_empty() && Shortcut::from_str(accelerator).is_ok()
}

pub fn unregister_all(app: &AppHandle) -> AppResult<()> {
    app.global_shortcut()
        .unregister_all()
        .map_err(|e| AppError::msg(e.to_string()))
}

#[derive(Debug, Clone, Copy)]
enum Action {
    /// Custom pointer <-> Windows default, without opening the window.
    Toggle,
    Open,
    PresetSlot(usize),
}

impl Action {
    fn run(self, app: &AppHandle) {
        match self {
            Action::Open => crate::tray::show_window(app),
            Action::Toggle => toggle(app),
            Action::PresetSlot(index) => {
                let Ok(all) = presets::list() else { return };
                if let Some(preset) = all.get(index) {
                    let _ = crate::commands::apply_preset_inner(app, preset);
                }
            }
        }
    }
}

/// One key, both directions: if something of ours is applied, put Windows back;
/// if not, re-apply what was applied last.
fn toggle(app: &AppHandle) {
    let is_ours = crate::cursor::active_state()
        .map(|state| !state.is_default)
        .unwrap_or(false);

    if is_ours {
        let _ = crate::cursor::restore_default();
    } else if let Some(descriptor) = crate::session::load() {
        // Rebuilding from the descriptor is what makes the toggle round-trip
        // after a restart, when nothing is held in memory yet.
        let _ = reapply(app, &descriptor);
    }
    crate::tray::refresh_tooltip(app);
}

fn reapply(app: &AppHandle, descriptor: &crate::session::AppliedDescriptor) -> AppResult<()> {
    use crate::packs::catalog::RenderSpec;
    use crate::session::AppliedSource;

    let spec = RenderSpec {
        tint: descriptor.tint.clone(),
        size: descriptor.size,
        outline: descriptor.outline,
    };

    let (set, pack_id) = match &descriptor.source {
        AppliedSource::Pack { pack_id, .. } => (
            crate::packs::catalog::build_set(pack_id, &spec)?,
            Some(pack_id.clone()),
        ),
        AppliedSource::Custom {
            cursor_id,
            apply_mode,
            blend_pack_id,
        } => (
            crate::custom::build_set(cursor_id, *apply_mode, blend_pack_id.as_deref(), &spec)?,
            blend_pack_id.clone(),
        ),
    };

    crate::cursor::commit(
        set,
        &descriptor.display_name,
        descriptor.size,
        pack_id,
        descriptor.tint.clone(),
    )?;
    crate::tray::refresh_tooltip(app);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The defaults must be registerable, or a fresh install ships with three
    /// hotkeys that quietly do nothing.
    #[test]
    fn every_default_hotkey_can_be_registered() {
        let defaults = Settings::default();
        assert!(is_registerable(&defaults.hotkey_toggle), "{}", defaults.hotkey_toggle);
        assert!(is_registerable(&defaults.hotkey_open), "{}", defaults.hotkey_open);
        for accelerator in &defaults.hotkey_presets {
            assert!(is_registerable(accelerator), "{accelerator}");
        }
    }

    /// A bare key is refused. A global shortcut is global: bound to `A` it
    /// would swallow that letter in every other application on the machine.
    #[test]
    fn a_shortcut_needs_more_than_one_key() {
        assert!(!is_registerable(""));
        assert!(!is_registerable("   "));
    }

    #[test]
    fn the_default_accelerators_all_parse() {
        let settings = Settings::default();
        let mut all = vec![settings.hotkey_toggle, settings.hotkey_open];
        all.extend(settings.hotkey_presets);
        for accelerator in all {
            assert!(
                Shortcut::from_str(&accelerator).is_ok(),
                "{accelerator} is not a usable accelerator"
            );
        }
    }

    #[test]
    fn nonsense_accelerators_are_rejected_rather_than_registered() {
        assert!(Shortcut::from_str("").is_err());
        assert!(Shortcut::from_str("NotAKey+Nope").is_err());
    }
}
