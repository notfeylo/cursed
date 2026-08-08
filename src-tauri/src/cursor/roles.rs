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

    /// `Pin` and `Person` exist in the scheme but are not documented as
    /// `SetSystemCursor` targets. We still try — and we never treat a refusal as
    /// a failure of the whole apply.
    pub const fn live_layer_is_best_effort(self) -> bool {
        matches!(self, Role::Pin | Role::Person)
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
    use std::collections::HashSet;

    #[test]
    fn every_role_has_a_unique_registry_value_and_ocr_id() {
        let values: HashSet<_> = ALL_ROLES.iter().map(|r| r.registry_value()).collect();
        let ids: HashSet<_> = ALL_ROLES.iter().map(|r| r.ocr_id()).collect();
        assert_eq!(values.len(), 17, "registry value names must be distinct");
        assert_eq!(ids.len(), 17, "OCR ids must be distinct");
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
