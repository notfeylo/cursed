//! Which of the two installs this binary is.
//!
//! Cursed ships one product but is developed as two: the build being iterated
//! on, and an exact copy of what a stranger downloads, installed side by side on
//! the same PC. Two copies of one app collide unless every shared resource is
//! named per channel, so every such name is derived from the constants here and
//! nowhere else.
//!
//! **Resolved at compile time, never at runtime.** A runtime flag can be flipped
//! by accident — an environment variable inherited from a parent process, a
//! stray argument — and a shipped binary that can be talked into behaving like a
//! dev build is worse than no separation at all.
//!
//! The values are `#[cfg]`-gated constants rather than a `match` on an enum on
//! purpose. A match would leave *both* channels' strings in the source of the
//! compiled crate and rely on the optimiser to drop the unreachable arm; the
//! release guard in `check-bundle.mjs` works by searching the shipped `.exe` for
//! [`MARKER`], and a guard that depends on an optimisation nobody verified is
//! not a guard. With `#[cfg]` exactly one marker exists in the artifact.

/// What the USER channel is called everywhere Windows can see it.
#[cfg(not(feature = "dev-channel"))]
mod values {
    pub const DATA_DIR: &str = "Cursed";
    pub const PRODUCT_NAME: &str = "Cursed";
    pub const IDENTIFIER: &str = "dev.feylo.cursed";
    /// Searched for in the shipped binary by `npm run check:bundle`.
    pub const MARKER: &str = "CURSED-CHANNEL:USER";
    pub const NAME: &str = "user";
    pub const IS_DEV: bool = false;
    /// Prefixes the entries this channel registers in the Windows Pointers
    /// dropdown. Per channel because an uninstall removes its own schemes by
    /// matching this prefix — shared, uninstalling either channel would strip
    /// the other's saved schemes out of Windows.
    pub const SCHEME_PREFIX: &str = "Cursed — ";
}

#[cfg(feature = "dev-channel")]
mod values {
    pub const DATA_DIR: &str = "Cursed (Dev)";
    pub const PRODUCT_NAME: &str = "Cursed Dev";
    pub const IDENTIFIER: &str = "dev.feylo.cursed.dev";
    /// Finding this string in a release artifact fails the release.
    pub const MARKER: &str = "CURSED-CHANNEL:DEV";
    pub const NAME: &str = "dev";
    pub const IS_DEV: bool = true;
    pub const SCHEME_PREFIX: &str = "Cursed Dev — ";
}

pub use values::{DATA_DIR, IDENTIFIER, IS_DEV, MARKER, NAME, PRODUCT_NAME, SCHEME_PREFIX};

/// The scheme prefix this channel is allowed to clean up on top of its own.
///
/// Same rule as [`legacy_data_dir`]: the dev channel inherits nothing, or a dev
/// uninstall would strip the user channel's older entries out of the Windows
/// Pointers dropdown.
pub const fn legacy_scheme_prefix() -> Option<&'static str> {
    if IS_DEV {
        None
    } else {
        Some("CursorForge — ")
    }
}

/// The name of the cross-channel pointer lock.
///
/// Deliberately **not** channel-scoped — it is the one name both channels must
/// agree on, because there is exactly one system cursor scheme to own. `Local\`
/// rather than `Global\` keeps it per-session, so a second signed-in Windows
/// user still gets their own.
pub const POINTER_LOCK: &str = r"Local\dev.feylo.cursed.pointer";

/// Where the two channels leave each other notes. Also not channel-scoped.
pub const CROSS_CHANNEL_KEY: &str = r"Software\Cursed";

/// True when this build must not register system-wide hotkeys by default.
///
/// Global shortcuts are first-come-first-served: whichever channel launches
/// first takes `Ctrl+Alt+1` and the other silently gets nothing, with no error
/// either of them can report. The dev build yields.
pub const fn claims_hotkeys_by_default() -> bool {
    !IS_DEV
}

/// True when this build should run the pointer watchdog by default.
///
/// The user channel is the one simulating a real install, so it is the one that
/// should be defending the scheme. Two watchdogs would fight; see
/// `cursor::crosschannel`.
pub const fn guards_pointer_by_default() -> bool {
    !IS_DEV
}

/// What `%APPDATA%` folder the app *used* to use, if this channel inherits it.
///
/// Only the user channel does. The migration in `paths::root` fires whenever the
/// current data directory is absent, which is true of every dev channel on its
/// first run — so without this the first `npm run tauri dev` on a machine still
/// holding a `CursorForge` folder would move the real user's settings, presets
/// and imported artwork into the dev channel and leave the user channel empty.
pub const fn legacy_data_dir() -> Option<&'static str> {
    if IS_DEV {
        None
    } else {
        Some("CursorForge")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every name that Windows uses to tell two installs apart must actually
    /// differ between the channels. This test only sees one channel's values, so
    /// it checks them against the channel it was compiled for.
    #[test]
    fn the_names_match_the_channel_this_was_compiled_for() {
        if IS_DEV {
            assert_eq!(DATA_DIR, "Cursed (Dev)");
            assert_eq!(PRODUCT_NAME, "Cursed Dev");
            assert_eq!(IDENTIFIER, "dev.feylo.cursed.dev");
            assert_eq!(MARKER, "CURSED-CHANNEL:DEV");
        } else {
            assert_eq!(DATA_DIR, "Cursed");
            assert_eq!(PRODUCT_NAME, "Cursed");
            assert_eq!(IDENTIFIER, "dev.feylo.cursed");
            assert_eq!(MARKER, "CURSED-CHANNEL:USER");
        }
    }

    /// The user channel is not "the dev build with a different name" — it is
    /// what ships. If these ever disagree the installed copy being tested is not
    /// the copy being released, and the whole exercise is worthless.
    #[test]
    fn the_user_channel_matches_what_the_bundler_ships() {
        if IS_DEV {
            return;
        }
        let tauri: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).expect("tauri.conf.json");
        assert_eq!(tauri["productName"].as_str(), Some(PRODUCT_NAME));
        assert_eq!(tauri["identifier"].as_str(), Some(IDENTIFIER));
    }

    /// The dev channel's overrides are a separate config file, and nothing at
    /// build time checks it against these constants — the bundler simply merges
    /// whatever it is given. A typo there produces a dev build that installs
    /// over the user channel, which is the exact accident this module exists to
    /// prevent.
    #[test]
    fn the_dev_override_file_agrees_with_the_dev_constants() {
        let dev: serde_json::Value =
            serde_json::from_str(include_str!("../dev.tauri.conf.json")).expect("dev config");
        assert_eq!(dev["productName"].as_str(), Some("Cursed Dev"));
        assert_eq!(dev["mainBinaryName"].as_str(), Some("Cursed Dev"));
        assert_eq!(dev["identifier"].as_str(), Some("dev.feylo.cursed.dev"));

        // Every icon the dev bundle uses must come from `icons/dev`. Left
        // pointing at the shipped set, the dev build is indistinguishable from
        // the released one in the tray, the taskbar and the Start menu — which
        // is most of what installing them side by side was supposed to fix.
        let icons = dev["bundle"]["icon"].as_array().expect("bundle.icon");
        assert!(!icons.is_empty());
        for icon in icons {
            let path = icon.as_str().unwrap_or_default();
            assert!(path.starts_with("icons/dev/"), "{path} is not a dev icon");
        }
        assert_eq!(
            dev["bundle"]["windows"]["nsis"]["installerIcon"].as_str(),
            Some("icons/dev/icon.ico")
        );
    }

    /// Both channels must ask for the same lock, or neither can see the other.
    #[test]
    fn the_pointer_lock_is_not_channel_scoped() {
        assert!(!POINTER_LOCK.contains("dev.feylo.cursed.dev"));
        assert!(!CROSS_CHANNEL_KEY.to_ascii_lowercase().contains("dev)"));
    }

    #[test]
    fn only_the_user_channel_inherits_the_old_folder() {
        assert_eq!(legacy_data_dir().is_some(), !IS_DEV);
    }
}
