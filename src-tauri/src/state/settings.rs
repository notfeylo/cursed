use crate::error::AppResult;
use crate::paths;
use serde::{Deserialize, Serialize};
use std::sync::{Mutex, OnceLock};

/// The full §9 settings surface. Every field has a defined default, and an
/// unreadable or partially-written file falls back to defaults rather than
/// refusing to start — settings are a preference, never a blocker.
/// The pointer size range the UI offers and the backend enforces.
///
/// 10 px is genuinely small — smaller than Windows' own minimum — and exists
/// because people asked for a pointer that gets out of the way. 128 px is the
/// largest Windows will draw for a cursor in practice; asking for more just
/// produces a file nothing reads.
pub const MIN_CURSOR_PX: u32 = 10;
pub const MAX_CURSOR_PX: u32 = 128;

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
    /// Whether the size control moves the hand and the text cursor as well.
    ///
    /// **On by default, changed from off.** The argument for off is still true
    /// and is still worth reading: the pointer is what a large cursor is *for*,
    /// while the hand appears under whatever is already being pointed at and the
    /// text cursor sits between two characters — so scaling those to 128 px
    /// covers the very thing they exist to indicate.
    ///
    /// It was the wrong default anyway, because it is not the question the user
    /// is answering. Somebody who sets a big pointer has said "I want a big
    /// cursor", and a hand that stays small reads as the setting not having
    /// worked. The considered reason it stayed small was invisible; the
    /// inconsistency was not. The switch is still there for anyone who wants the
    /// old behaviour, and the sentence above it now explains the trade instead
    /// of leaving it to be inferred from a pointer that half-changed.
    pub scale_all_roles: bool,
    pub apply_mode: ApplyMode,
    /// What the link hand is. See [`HoverStyle`].
    ///
    /// `Pack` by default: it is what every existing install already does, and a
    /// pack whose hand is genuinely part of the design should keep it unless
    /// somebody says otherwise.
    pub hover_style: HoverStyle,
    /// Fills the roles a custom or imported cursor does not define, so a
    /// one-role import does not leave fifteen stock Windows pointers behind it.
    pub blend_pack: String,
    /// Whether catalog tiles are recoloured to the chosen tint.
    ///
    /// Off by default. Tinting every tile the same colour makes a large catalog
    /// unreadable — two hundred identically-coloured arrows tell you nothing
    /// about which is which. Showing each pack in its own colours is how you
    /// find the one you want; the tint still applies to the cursor you actually
    /// apply, which is what it was always for.
    pub tint_previews: bool,
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
    /// Whether the user has seen the notice about their original pointer scheme
    /// having been lost to the update bug that shipped through v1.20.0.
    ///
    /// A setting rather than a marker file because it is exactly what a setting
    /// is: a per-user preference about the app's behaviour, and one the user
    /// sets by dismissing a banner. The notice is shown once and never again,
    /// and there is nothing to un-dismiss — the information is gone either way.
    pub scheme_loss_acknowledged: bool,
}

/// What the hand — the pointer Windows shows over a link — is made of.
///
/// **This exists because of a complaint that kept coming back.** A lot of the
/// catalog is character artwork, and a pack's hand is often a second, unrelated
/// drawing: pick a cursor you like and the moment you hover a link it becomes
/// something else. People read that as the cursor breaking, and the only way out
/// was to not use those packs.
///
/// Deleting the hand artwork was never the answer — plenty of packs have a hand
/// that is the whole point of the pack, and those users would have lost it. So
/// it is a choice, made where the cursor is chosen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HoverStyle {
    /// Whatever the pack draws for it. What every pack did before this existed.
    Pack,
    /// The pack's own pointer, used for the hand as well, so hovering a link
    /// changes nothing at all.
    Pointer,
    /// The Cursed mark.
    Mark,
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
            scale_all_roles: true,
            apply_mode: ApplyMode::Blend,
            hover_style: HoverStyle::Pack,
            blend_pack: "precision-gap-cross".to_owned(),
            tint_previews: false,
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
            scheme_loss_acknowledged: false,
        }
    }
}

impl Settings {
    /// Clamps anything a hand-edited settings file could put out of range.
    pub fn sanitised(mut self) -> Self {
        // Clamped **and snapped to a rung of the size ladder**.
        //
        // A `.cur` carries a fixed set of resolutions and Windows draws it at
        // whatever `CursorBaseSize` says. Land between two rungs and the shell
        // scales the nearest one to fit — bilinear, unpremultiplied, no gamma
        // correction — and the pointer arrives soft with its edge stepped. The
        // size control moves in twos from 10 to 128, which is sixty positions
        // against ten rungs: fifty-two of them were a stretch, and the eight
        // that were not are exactly the preset chips underneath the slider.
        // That is why picking a preset looked sharp and dragging the slider did
        // not.
        //
        // Snapping here rather than in the UI means it holds for a hand-edited
        // settings file, a preset restored from a `.cfpack`, and a size that
        // arrived from an older build — and the number the app displays is the
        // number Windows is given, which is the only version of this that is
        // honest.
        //
        // The animated path never needed it: an `.ani` carries one resolution
        // and is rendered at the exact size asked for.
        self.cursor_size = self
            .cursor_size
            .map(|s| crate::build::pipeline::nearest_size(s.clamp(MIN_CURSOR_PX, MAX_CURSOR_PX)));
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
    SETTINGS.get_or_init(|| Mutex::new(read_from_disk().sanitised()))
}

/// Reads the settings, recovering from the backup if the file will not parse.
///
/// Cannot fail. Settings that will not load must not stop the app starting —
/// the defaults are a working app, and `sanitised` makes them a coherent one.
fn read_from_disk() -> Settings {
    let Ok(file) = paths::settings_file() else {
        return Settings::default();
    };
    let (settings, source) = crate::state::store::read::<Settings>(&file);
    if source == crate::state::store::Source::Backup {
        log::warn!("settings were recovered from the backup copy");
    }
    settings
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
    crate::state::store::write(&file, &serde_json::to_string_pretty(&next)?)?;

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

        assert_eq!(wild.cursor_size, Some(MAX_CURSOR_PX));
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

    /// A settings file written by v1.6, loaded into this build.
    ///
    /// This is the migration test, and the reason there is no schema version
    /// number to go with it: the format has only ever gained fields, and
    /// `#[serde(default)]` on the struct means a field that did not exist yet
    /// takes its default rather than failing the parse. A version number would
    /// be a second thing to keep correct that nothing would read.
    ///
    /// What matters is that the user's *choices* survive. Someone who turned
    /// autostart off in 1.6 must not find it back on after updating, and a
    /// blend pack from the 291-pack generated catalog — which no longer
    /// exists — must not leave every imported cursor failing to apply.
    #[test]
    fn a_settings_file_from_v1_6_still_loads_with_its_choices_intact() {
        // Field for field what v1.6.0 wrote: no scaleAllRoles, no
        // schemeLossAcknowledged, and a blendPack from the catalog of the day.
        let old = r##"{
            "launchOnStartup": false,
            "startMinimized": false,
            "closeToTray": true,
            "showTrayIcon": false,
            "autoCheckUpdates": false,
            "cursorSize": 64,
            "tint": "#FF6A2E",
            "outline": false,
            "applyMode": "All",
            "blendPack": "neon-plasma-042",
            "tintPreviews": true,
            "animationSpeed": 1.5,
            "reapplyOnResume": false,
            "watchdogEnabled": false,
            "watchdogIntervalSecs": 12,
            "reapplyAfterThemeChange": false,
            "hotkeyToggle": "Ctrl+Alt+9",
            "hotkeyOpen": "Ctrl+Alt+K",
            "hotkeyPresets": ["Ctrl+Alt+1", "Ctrl+Alt+2", "Ctrl+Alt+3", "Ctrl+Alt+4", "Ctrl+Alt+5"],
            "debugLogging": true,
            "firstRunDone": true
        }"##;

        let loaded: Settings = serde_json::from_str(old).expect("a v1.6 file must parse");
        let loaded = loaded.sanitised();

        // Every choice they made, still made.
        assert!(!loaded.launch_on_startup);
        assert!(!loaded.start_minimized);
        assert!(!loaded.show_tray_icon);
        assert!(!loaded.auto_check_updates);
        assert_eq!(loaded.cursor_size, Some(64));
        assert_eq!(loaded.tint, "#FF6A2E");
        assert!(!loaded.outline);
        assert_eq!(loaded.apply_mode, ApplyMode::All);
        assert!(loaded.tint_previews);
        assert_eq!(loaded.animation_speed, 1.5);
        assert!(!loaded.watchdog_enabled);
        assert_eq!(loaded.watchdog_interval_secs, 12);
        assert_eq!(loaded.hotkey_toggle, "Ctrl+Alt+9");
        assert!(loaded.debug_logging);
        assert!(loaded.first_run_done);

        // Fields that did not exist yet take their defaults — which is worth
        // stating rather than just asserting, because this one is a visible
        // change on upgrade. A file this old predates `scaleAllRoles`, so it
        // takes the current default of **on**, and the hand and text cursor a
        // v1.6 user was seeing at 32 px will grow with their 64 px pointer the
        // first time this build runs. That is the intended behaviour and the
        // switch to undo it is in the same panel as the size.
        assert!(
            loaded.scale_all_roles,
            "a file predating the field takes the current default"
        );
        assert!(
            !loaded.scheme_loss_acknowledged,
            "an older user has not been shown the notice yet"
        );

        // And the one value that has to be *corrected* rather than kept: a pack
        // id from a catalog this build does not have.
        assert!(
            crate::packs::styles::find(&loaded.blend_pack).is_some(),
            "a stale blend pack would make every imported cursor fail to apply"
        );
    }

    /// The other direction: a file written by a *newer* build, opened by this
    /// one after a downgrade or a rollback. Unknown fields must be ignored
    /// rather than fail the parse and reset everything to defaults.
    #[test]
    fn a_settings_file_from_a_newer_version_does_not_reset_everything() {
        let newer = r##"{
            "launchOnStartup": false,
            "tint": "#00FF88",
            "somethingAddedLater": { "nested": true },
            "anotherNewField": 42
        }"##;
        let loaded: Settings = serde_json::from_str(newer).expect("unknown fields must be ignored");
        assert!(!loaded.launch_on_startup);
        assert_eq!(loaded.tint, "#00FF88");
    }
}
