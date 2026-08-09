use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};

/// The complete, closed set of Windows pointer roles.
///
/// This enum is the security boundary described in PRD §13.4: the frontend can
/// only ever name a variant of this type. It never supplies a registry value
/// name, a key path, or an OCR id — those are derived here, in Rust, from a
/// hardcoded table that cannot be influenced by IPC input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Role {
    Arrow,
    Help,
    AppStarting,
    Wait,
    Crosshair,
    IBeam,
    NWPen,
    No,
    SizeNS,
    SizeWE,
    SizeNWSE,
    SizeNESW,
    SizeAll,
    UpArrow,
    Hand,
    Pin,
    Person,
}

/// Every role, in the order the Windows Pointers tab lists them.
pub const ALL_ROLES: [Role; 17] = [
    Role::Arrow,
    Role::Help,
    Role::AppStarting,
    Role::Wait,
    Role::Crosshair,
    Role::IBeam,
    Role::NWPen,
    Role::No,
    Role::SizeNS,
    Role::SizeWE,
    Role::SizeNWSE,
    Role::SizeNESW,
    Role::SizeAll,
    Role::UpArrow,
    Role::Hand,
    Role::Pin,
    Role::Person,
];

/// The size at which the link hand and the text I-beam stop growing.
///
/// 32 px is the Windows base metric — the size these have always been on a
/// default install, and the size at which an I-beam still sits cleanly between
/// two characters.
pub const UNSCALED_ROLE_PX: u32 = 32;

/// The roles a "Recommended" custom-cursor application touches.
pub const RECOMMENDED_ROLES: [Role; 3] = [Role::Arrow, Role::Hand, Role::Crosshair];

impl Role {
    /// The `HKCU\Control Panel\Cursors` value name. Fixed strings only.
    pub const fn registry_value(self) -> &'static str {
        match self {
            Role::Arrow => "Arrow",
            Role::Help => "Help",
            Role::AppStarting => "AppStarting",
            Role::Wait => "Wait",
            Role::Crosshair => "Crosshair",
            Role::IBeam => "IBeam",
            Role::NWPen => "NWPen",
            Role::No => "No",
            Role::SizeNS => "SizeNS",
            Role::SizeWE => "SizeWE",
            Role::SizeNWSE => "SizeNWSE",
            Role::SizeNESW => "SizeNESW",
            Role::SizeAll => "SizeAll",
            Role::UpArrow => "UpArrow",
            Role::Hand => "Hand",
            Role::Pin => "Pin",
            Role::Person => "Person",
        }
    }

    /// The `OCR_*` / `IDC_*` identifier `SetSystemCursor` expects.
    pub const fn ocr_id(self) -> u32 {
        match self {
            Role::Arrow => 32512,       // OCR_NORMAL
            Role::IBeam => 32513,       // OCR_IBEAM
            Role::Wait => 32514,        // OCR_WAIT
            Role::Crosshair => 32515,   // OCR_CROSS
            Role::UpArrow => 32516,     // OCR_UP
            Role::NWPen => 32631,       // pen / handwriting
            Role::SizeNWSE => 32642,    // OCR_SIZENWSE
            Role::SizeNESW => 32643,    // OCR_SIZENESW
            Role::SizeWE => 32644,      // OCR_SIZEWE
            Role::SizeNS => 32645,      // OCR_SIZENS
            Role::SizeAll => 32646,     // OCR_SIZEALL
            Role::No => 32648,          // OCR_NO
            Role::Hand => 32649,        // OCR_HAND
            Role::AppStarting => 32650, // OCR_APPSTARTING
            Role::Help => 32651,        // OCR_HELP
            Role::Pin => 32671,         // IDC_PIN     (Win8+, undocumented for SetSystemCursor)
            Role::Person => 32672,      // IDC_PERSON  (Win8+, undocumented for SetSystemCursor)
        }
    }

    /// Whether `SetSystemCursor` may legitimately refuse this role.
    ///
    /// `SetSystemCursor` accepts only the fourteen documented `OCR_*` values.
    /// `NWPen`, `Pin` and `Person` are `IDC_*` resource ids — real scheme roles
    /// that Windows will write to the registry and honour after a reload, but
    /// that it will not accept as a live in-session override.
    ///
    /// This distinction is not cosmetic. Treating a refusal here as a failed
    /// apply makes *every* apply report an error, which in turn skips recording
    /// what was applied — so the cursor changes, but nothing persists and the
    /// watchdog has nothing to protect.
    pub const fn live_layer_is_best_effort(self) -> bool {
        matches!(self, Role::NWPen | Role::Pin | Role::Person)
    }

    /// The size this role should actually be drawn at.
    ///
    /// The size control scales **the pointer**. It does not scale the link hand
    /// or the text I-beam, and that is deliberate rather than an oversight.
    ///
    /// Those two are tools, not the pointer wearing a hat. A 128 px I-beam
    /// cannot be placed between two characters — the thing it exists to do — and
    /// a hand blown up to match reads as a slab covering whatever it is hovering
    /// over. Windows scales every role together, which is why a large pointer
    /// has always come with those side effects.
    ///
    /// It is a cap rather than a fixed value, so choosing a *small* pointer does
    /// not leave a comparatively enormous hand behind: at 10 px everything is
    /// 10 px, and only growth past the comfortable size is held back.
    pub fn size_from(self, requested: u32) -> u32 {
        match self {
            Role::Hand | Role::IBeam => requested.min(UNSCALED_ROLE_PX),
            _ => requested,
        }
    }

    /// Filename stem used for this role inside a pack directory.
    pub const fn file_stem(self) -> &'static str {
        self.registry_value()
    }

    pub const fn label(self) -> &'static str {
        match self {
            Role::Arrow => "Normal select",
            Role::Help => "Help select",
            Role::AppStarting => "Working in background",
            Role::Wait => "Busy",
            Role::Crosshair => "Precision select",
            Role::IBeam => "Text select",
            Role::NWPen => "Handwriting",
            Role::No => "Unavailable",
            Role::SizeNS => "Vertical resize",
            Role::SizeWE => "Horizontal resize",
            Role::SizeNWSE => "Diagonal resize 1",
            Role::SizeNESW => "Diagonal resize 2",
            Role::SizeAll => "Move",
            Role::UpArrow => "Alternate select",
            Role::Hand => "Link select",
            Role::Pin => "Location select",
            Role::Person => "Person select",
        }
    }

    /// Roles Windows animates by default. A pack may ship `.ani` for these.
    pub const fn is_animatable(self) -> bool {
        matches!(self, Role::Wait | Role::AppStarting)
    }

    /// Parses a variant name arriving over IPC. Anything unrecognised is
    /// rejected outright rather than defaulted — a silent fallback to `Arrow`
    /// would let a malformed caller quietly rewrite the wrong pointer.
    pub fn parse(name: &str) -> AppResult<Self> {
        ALL_ROLES
            .into_iter()
            .find(|r| r.registry_value() == name)
            .ok_or_else(|| AppError::UnknownRole(name.to_owned()))
    }
}

impl std::fmt::Display for Role {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.registry_value())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The size control scales the pointer. A 128 px I-beam cannot be placed
    /// between two characters, and a hand blown up to match covers whatever it
    /// is hovering over.
    #[test]
    fn the_hand_and_the_ibeam_stop_growing_with_the_pointer() {
        assert_eq!(Role::Arrow.size_from(128), 128);
        assert_eq!(Role::Hand.size_from(128), UNSCALED_ROLE_PX);
        assert_eq!(Role::IBeam.size_from(128), UNSCALED_ROLE_PX);

        // Every other role still follows the pointer, including the ones that
        // are the pointer wearing a hat.
        for role in [Role::Wait, Role::AppStarting, Role::Help, Role::SizeAll] {
            assert_eq!(role.size_from(96), 96, "{role} should follow the pointer");
        }
    }

    /// It is a cap, not a fixed size. Choosing a small pointer must not leave a
    /// comparatively enormous hand behind it.
    #[test]
    fn a_small_pointer_does_not_keep_a_full_size_hand() {
        for requested in [10u32, 16, 24, 32] {
            assert_eq!(Role::Hand.size_from(requested), requested);
            assert_eq!(Role::IBeam.size_from(requested), requested);
        }
    }
    use std::collections::HashSet;

    #[test]
    fn every_role_has_a_unique_registry_value_and_ocr_id() {
        let values: HashSet<_> = ALL_ROLES.iter().map(|r| r.registry_value()).collect();
        let ids: HashSet<_> = ALL_ROLES.iter().map(|r| r.ocr_id()).collect();
        assert_eq!(values.len(), 17, "registry value names must be distinct");
        assert_eq!(ids.len(), 17, "OCR ids must be distinct");
    }

    /// The fourteen ids `SetSystemCursor` documents. Any role outside this set
    /// must be best-effort on the live layer, or a refusal Windows considers
    /// normal gets reported as a failed apply.
    #[test]
    fn exactly_the_undocumented_roles_are_best_effort() {
        const DOCUMENTED_OCR: [u32; 14] = [
            32512, 32513, 32514, 32515, 32516, 32642, 32643, 32644, 32645, 32646, 32648, 32649,
            32650, 32651,
        ];
        for role in ALL_ROLES {
            let documented = DOCUMENTED_OCR.contains(&role.ocr_id());
            assert_eq!(
                role.live_layer_is_best_effort(),
                !documented,
                "{role} ({}) is {}documented, so best-effort should be {}",
                role.ocr_id(),
                if documented { "" } else { "un" },
                !documented
            );
        }
    }

    #[test]
    fn every_role_still_persists_through_the_registry() {
        // Best-effort applies only to the in-session override. All seventeen
        // roles are written to the scheme regardless, which is what makes the
        // cursor survive a reboot.
        for role in ALL_ROLES {
            assert!(!role.registry_value().is_empty());
        }
    }

    #[test]
    fn parse_round_trips_and_rejects_junk() {
        for role in ALL_ROLES {
            assert_eq!(Role::parse(role.registry_value()).unwrap(), role);
        }
        assert!(Role::parse("Arrow\\..\\..\\evil").is_err());
        assert!(Role::parse("").is_err());
        assert!(Role::parse("arrow").is_err(), "matching is exact, not lax");
    }
}
