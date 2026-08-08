use crate::error::AppResult;
use crate::paths;
use serde::{Deserialize, Serialize};
use std::sync::{Mutex, OnceLock};

/// The full §9 settings surface. Every field has a defined default, and an
/// unreadable or partially-written file falls back to defaults rather than
/// refusing to start — settings are a preference, never a blocker.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    // General
    pub launch_on_startup: bool,
    pub start_minimized: bool,
    pub close_to_tray: bool,
    pub show_tray_icon: bool,
    pub auto_check_updates: bool,

    // Cursor
    /// `None` means "inherit whatever Windows' own size slider says".
    pub cursor_size: Option<u32>,
    pub tint: String,
    pub outline: bool,
    pub apply_mode: ApplyMode,
    /// Fills the roles a custom or imported cursor does not define, so a
    /// one-role import does not leave fifteen stock Windows pointers behind it.
    pub blend_pack: String,
    pub animation_speed: f32,
    pub reapply_on_resume: bool,

    // Protection
    pub watchdog_enabled: bool,
    pub watchdog_interval_secs: u64,
    pub reapply_after_theme_change: bool,

    // Hotkeys
    pub hotkey_toggle: String,
    pub hotkey_open: String,
    pub hotkey_presets: Vec<String>,

    // Advanced
    pub debug_logging: bool,
    pub first_run_done: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ApplyMode {
    ArrowOnly,
    Recommended,
    All,
    /// Custom arrow over a catalog pack for the other sixteen roles — the one
    /// mode that produces a coherent pointer set from a single user image.
    Blend,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            launch_on_startup: true,
            start_minimized: true,
            close_to_tray: true,
            show_tray_icon: true,
            auto_check_updates: true,

            cursor_size: None,
            tint: "#2E8BFF".to_owned(),
            outline: true,
            apply_mode: ApplyMode::Blend,
            blend_pack: "precision-gap-cross".to_owned(),
            animation_speed: 1.0,
            reapply_on_resume: true,

            watchdog_enabled: true,
            watchdog_interval_secs: 5,
            reapply_after_theme_change: true,

            hotkey_toggle: "Ctrl+Alt+0".to_owned(),
            hotkey_open: "Ctrl+Alt+C".to_owned(),
            hotkey_presets: (1..=5).map(|n| format!("Ctrl+Alt+{n}")).collect(),

            debug_logging: false,
            first_run_done: false,
        }
    }
}

impl Settings {
    /// Clamps anything a hand-edited settings file could put out of range.
    pub fn sanitised(mut self) -> Self {
        self.cursor_size = self.cursor_size.map(|s| s.clamp(32, 256));
        self.animation_speed = self.animation_speed.clamp(0.5, 2.0);
        self.watchdog_interval_secs = self.watchdog_interval_secs.clamp(3, 30);
        if crate::util::parse_hex_color(&self.tint).is_none() {
            self.tint = "#2E8BFF".to_owned();
        }
        self.hotkey_presets.truncate(5);
        while self.hotkey_presets.len() < 5 {
            let n = self.hotkey_presets.len() + 1;
            self.hotkey_presets.push(format!("Ctrl+Alt+{n}"));
        }
        // The blend pack fills roles an import does not define, so it has to be
        // a pack that actually exists — a stale id from an older build would
        // make every imported cursor fail to apply.
        if crate::packs::styles::find(&self.blend_pack).is_none() {
            self.blend_pack = "precision-gap-cross".to_owned();
        }
        self
    }
}

fn slot() -> &'static Mutex<Settings> {
    static SETTINGS: OnceLock<Mutex<Settings>> = OnceLock::new();
    SETTINGS.get_or_init(|| Mutex::new(read_from_disk().unwrap_or_default().sanitised()))
}

fn read_from_disk() -> AppResult<Settings> {
    let file = paths::settings_file()?;
    let text = std::fs::read_to_string(file)?;
    Ok(serde_json::from_str::<Settings>(crate::util::strip_bom(&text))?)
}

pub fn get() -> Settings {
    slot()
        .lock()
        .map(|guard| guard.clone())
        .unwrap_or_default()
}

pub fn save(next: Settings) -> AppResult<Settings> {
    let next = next.sanitised();
    let file = paths::settings_file()?;
    let temp = file.with_extension("json.tmp");
    std::fs::write(&temp, serde_json::to_string_pretty(&next)?)?;
    std::fs::rename(&temp, &file)?;

    if let Ok(mut guard) = slot().lock() {
        *guard = next.clone();
    }
    Ok(next)
}

/// Applies the settings that other subsystems cache.
pub fn propagate(settings: &Settings) {
    crate::cursor::watchdog::configure(
        settings.watchdog_enabled,
        settings.watchdog_interval_secs,
        settings.reapply_after_theme_change,
        settings.reapply_on_resume,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_the_specification() {
        let s = Settings::default();
        assert!(s.launch_on_startup && s.close_to_tray && s.watchdog_enabled);
        assert_eq!(s.watchdog_interval_secs, 5);
        assert_eq!(s.apply_mode, ApplyMode::Blend);
        assert_eq!(s.hotkey_presets.len(), 5);
        assert_eq!(s.cursor_size, None, "size inherits from Windows by default");
    }

    #[test]
    fn hand_edited_files_cannot_put_values_out_of_range() {
        let wild = Settings {
            cursor_size: Some(9_999),
            animation_speed: 40.0,
            watchdog_interval_secs: 0,
            tint: "not a colour".into(),
            hotkey_presets: vec!["A".into()],
            ..Settings::default()
        }
        .sanitised();

        assert_eq!(wild.cursor_size, Some(256));
        assert_eq!(wild.animation_speed, 2.0);
        assert_eq!(wild.watchdog_interval_secs, 3);
        assert_eq!(wild.tint, "#2E8BFF");
        assert_eq!(wild.hotkey_presets.len(), 5);
    }

    #[test]
    fn a_blend_pack_that_no_longer_exists_falls_back_to_a_real_one() {
        let stale = Settings {
            blend_pack: "removed-in-an-older-build".into(),
            ..Settings::default()
        }
        .sanitised();
        assert!(crate::packs::styles::find(&stale.blend_pack).is_some());
    }
}
