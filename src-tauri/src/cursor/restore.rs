//! First-run safety snapshot and one-click restore (PRD §4.4).
//!
//! The rule this file enforces: Cursed must never be the reason a machine
//! ends up with a pointer nobody asked for. Before the first write, the entire
//! pre-existing scheme is captured verbatim. Restore replays it exactly — and the
//! uninstaller runs the same routine before deleting anything.

use crate::cursor::scheme;
use crate::error::{AppError, AppResult};
use crate::paths;
use crate::util::iso_now;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OriginalScheme {
    /// Registry value name -> value, exactly as read. Empty means "was absent".
    pub values: BTreeMap<String, String>,
    pub cursor_base_size: Option<u32>,
    pub scheme_name: String,
    pub captured_at: String,
    /// How much this snapshot is worth. Absent in files written before 1.21.0,
    /// and absent means [`Provenance::Captured`] — every snapshot that predates
    /// the field was a real capture, because the alternative did not exist.
    #[serde(default)]
    pub provenance: Provenance,
}

/// Where a snapshot's contents came from, and therefore what restoring it means.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Provenance {
    /// Read off the machine before Cursed had written anything to it. The real
    /// thing, and the only kind worth having.
    #[default]
    Captured,
    /// Copied verbatim from the other channel's capture. Just as real — it is
    /// the same bytes, written by whichever channel got here first.
    Adopted,
    /// **Not a capture.** The machine's true original was lost before this
    /// channel ever saw it, so restoring returns the stock Windows scheme
    /// instead of something that was never on this machine. See
    /// [`stand_in_for_lost_capture`].
    Lost,
}

impl Provenance {
    /// Whether this describes the machine's real pre-Cursed pointers.
    pub fn is_real(self) -> bool {
        !matches!(self, Provenance::Lost)
    }
}

/// Captures the current scheme if — and only if — nothing has been captured yet.
///
/// Deliberately idempotent. Re-capturing on a later launch would overwrite the
/// user's real defaults with Cursed's own scheme, which would turn "restore"
/// into a no-op and quietly break the product's central promise.
///
/// Idempotence is per data directory, though, and each channel has its own. The
/// second channel to run on a machine therefore starts with no snapshot and a
/// Cursed cursor already on screen — so before capturing anything it asks
/// whether another channel got here first, and takes that channel's answer
/// verbatim. Only the first channel ever sees the true pre-Cursed pointers, and
/// only its capture is worth anything.
pub fn capture_once() -> AppResult<OriginalScheme> {
    let file = paths::original_scheme_file()?;
    // `snapshot_exists`, not `file.exists()`. A primary that is missing while
    // its backup is intact is recoverable, and treating it as "never captured"
    // would send this straight to the capture below — recording whatever Cursed
    // is currently displaying as the machine's true original.
    if snapshot_exists() {
        let Ok(existing) = read_snapshot() else {
            // Both copies unreadable. Capturing is the only thing left and it is
            // very probably wrong, so it is said as loudly as a log can say it.
            log::error!(
                "a snapshot exists but neither copy could be read; anything captured \
                 now may be a cursor Cursed applied rather than the real original"
            );
            return capture_fresh(&file);
        };
        // Registered on every launch, not only on the launch that captured it.
        // Every install that predates the cross-channel record has a snapshot
        // and no claim on it, so without this the record stays empty until
        // someone reinstalls — and a dev channel added to such a machine finds
        // nothing to adopt and captures the cursor the user channel is already
        // displaying, which is precisely the failure this is here to prevent.
        // `record_snapshot` is first-writer-wins, so repeating it is free.
        crate::cursor::crosschannel::record_snapshot(&file, &existing.captured_at);
        return Ok(existing);
    }

    if let Some(text) = crate::cursor::crosschannel::adopt_foreign_snapshot() {
        // Parsed before it is written so a corrupt file from the other channel
        // is not copied in and trusted.
        match serde_json::from_str::<OriginalScheme>(crate::util::strip_bom(&text)) {
            Ok(mut adopted) => {
                // The bytes are copied verbatim — see `adopt_foreign_snapshot`
                // for why the two files must stay identical — so only the
                // in-memory copy is relabelled. A snapshot the other channel
                // recorded as `Lost` stays lost here too, which is correct: it
                // is the same machine.
                if adopted.provenance == Provenance::Captured {
                    adopted.provenance = Provenance::Adopted;
                }
                write_snapshot(&file, &text)?;
                return Ok(adopted);
            }
            Err(e) => log::warn!(
                "cross-channel: the other channel's snapshot could not be parsed ({e}); \
                 capturing instead"
            ),
        }
    }

    // The last resort, and the one that has to be checked before it is taken.
    // Nothing has been captured and nothing can be adopted — which is the
    // ordinary state of a first run, and also the state left behind by an
    // update that deleted the data directory. The two are told apart by looking
    // at the pointer that is currently on screen.
    if ours_is_applied() {
        return stand_in_for_lost_capture(&file);
    }

    capture_fresh(&file)
}

/// Whether the pointers currently on the machine are ones Cursed put there.
///
/// Two independent signals, because either alone has a hole. The scheme *name*
/// is what the Pointers tab shows and is written by every apply — but a theme
/// change can reset the name while leaving our files in place. The role *values*
/// are the files themselves, which survive that, but a user who applied a single
/// role and then reset it would leave none. Together they cover the states that
/// actually occur.
///
/// Deliberately conservative in one direction only: a false positive costs the
/// user a snapshot they could have had, and a false negative records a Cursed
/// cursor as the machine's original for ever. The first is recoverable by
/// restoring Windows defaults; the second is not recoverable at all.
fn ours_is_applied() -> bool {
    let name = scheme::read_scheme_name().unwrap_or_default();
    if !name.is_empty()
        && (name.starts_with(crate::cursor::SCHEME_PREFIX)
            || crate::cursor::legacy_scheme_prefix().is_some_and(|old| name.starts_with(old)))
    {
        return true;
    }

    // A role pointing inside our own storage is ours whatever the scheme is
    // called. The registry holds the unexpanded `%APPDATA%\...` string most of
    // the time and an absolute path the rest of it, so both forms are compared.
    let mut roots: Vec<String> = Vec::with_capacity(2);
    if let Ok(root) = paths::root() {
        roots.push(root.to_string_lossy().to_ascii_lowercase());
    }
    roots.push(format!(
        r"%appdata%\{}",
        crate::channel::DATA_DIR.to_ascii_lowercase()
    ));

    crate::cursor::roles::ALL_ROLES.into_iter().any(|role| {
        let Ok(value) = scheme::read_role(role) else {
            return false;
        };
        if value.is_empty() {
            return false;
        }
        let value = value.replace('/', "\\").to_ascii_lowercase();
        roots.iter().any(|root| value.starts_with(root.as_str()))
    })
}

/// Records that this machine's true original scheme is gone, and stands the
/// stock Windows scheme in for it.
///
/// Reached when there is no snapshot to read, none to adopt, and a Cursed cursor
/// already applied — which up to and including v1.20.0 is what an in-app update
/// left behind, having run the previous version's uninstaller and deleted the
/// whole data directory. See `docs/UPDATE_PATH_DIAGNOSIS.md`.
///
/// **Capturing here would be the worst available option.** It would read the
/// cursor Cursed itself applied and file it as "what the pointers were before
/// Cursed", so Restore would put a Cursed cursor back, for ever, and nothing
/// about it would look broken until someone tried it.
///
/// So an empty scheme is written instead. Every role empty means "the value was
/// absent", which [`scheme::write_raw`] spells as a deletion, which is how
/// Windows is told to use its own built-in pointers — a real, correct end state,
/// just not the user's. It is marked [`Provenance::Lost`] so the app can say so
/// once rather than quietly pretending.
///
/// Written to disk rather than kept in memory so the decision is made once. A
/// second launch would otherwise reach this same branch, and a third — and any
/// one of them that ran while the pointer happened to be stock would capture
/// that and call it the original.
///
/// Nothing is recorded cross-channel: [`crosschannel::record_snapshot`] means
/// "this channel holds the machine's true original", and this is precisely the
/// case where nobody does.
fn stand_in_for_lost_capture(file: &std::path::Path) -> AppResult<OriginalScheme> {
    log::error!(
        "no original-scheme snapshot exists and a Cursed cursor is already applied, so the \
         machine's true original pointers are gone. Restore will return the stock Windows \
         scheme rather than record the current cursor as the original."
    );

    let snapshot = OriginalScheme {
        values: crate::cursor::roles::ALL_ROLES
            .into_iter()
            .map(|role| (role.registry_value().to_owned(), String::new()))
            .collect(),
        cursor_base_size: None,
        scheme_name: String::new(),
        captured_at: iso_now(),
        provenance: Provenance::Lost,
    };

    write_snapshot(file, &serde_json::to_string_pretty(&snapshot)?)?;
    Ok(snapshot)
}

/// What the snapshot on disk is worth, for the one notice the app owes the user.
///
/// `Captured` when there is nothing to read: no snapshot is a first run, and a
/// first run has lost nothing.
pub fn provenance() -> Provenance {
    read_snapshot()
        .map(|snapshot| snapshot.provenance)
        .unwrap_or_default()
}

/// Reads the scheme that is on the machine right now and records it as the
/// original.
///
/// Only correct when nothing of ours has ever been applied. Every caller has to
/// have established that first — this function cannot tell the difference.
fn capture_fresh(file: &std::path::Path) -> AppResult<OriginalScheme> {
    let (values, cursor_base_size, scheme_name) = scheme::read_all()?;
    let snapshot = OriginalScheme {
        values,
        cursor_base_size,
        scheme_name,
        captured_at: iso_now(),
        provenance: Provenance::Captured,
    };

    let json = serde_json::to_string_pretty(&snapshot)?;
    write_snapshot(file, &json)?;
    crate::cursor::crosschannel::record_snapshot(file, &snapshot.captured_at);
    Ok(snapshot)
}

/// Writes the snapshot durably, and keeps a copy beside it.
///
/// **This is the most irreplaceable file the app owns.** Every other piece of
/// state can be rebuilt by the user: a preset can be re-made, a cursor
/// re-imported, a setting set again. The record of what the pointers were
/// *before* Cursed ever ran cannot — once a Cursed cursor is applied, the
/// original is no longer anywhere on the machine to read back. Losing this file
/// means "restore Windows defaults" restores something that was never the
/// default, permanently and invisibly.
///
/// So it goes through the shared store, which flushes to the hardware before
/// renaming and leaves a `.bak` — the same treatment as presets, for the one
/// file that deserves it most.
fn write_snapshot(file: &std::path::Path, json: &str) -> AppResult<()> {
    crate::state::store::write(file, json)
}

pub fn read_snapshot() -> AppResult<OriginalScheme> {
    let file = paths::original_scheme_file()?;
    // Through the store, so a snapshot damaged by a crash or a bad sector is
    // recovered from its backup rather than reported as "there is nothing to
    // restore" — which would be a lie, and the one lie this app cannot take
    // back.
    let (snapshot, source) = crate::state::store::read::<Option<OriginalScheme>>(&file);
    if source == crate::state::store::Source::Backup {
        log::warn!("the original-scheme snapshot was recovered from its backup");
    }
    snapshot.ok_or_else(|| {
        AppError::storage("no original scheme snapshot exists yet, so there is nothing to restore")
    })
}

/// Whether the machine's original pointers have been recorded anywhere.
///
/// Counts the backup. A primary that is missing while the backup is intact is a
/// recoverable state, and answering "no snapshot" for it would let
/// `capture_once` record the cursor Cursed is currently displaying as the
/// original.
pub fn snapshot_exists() -> bool {
    let Ok(file) = paths::original_scheme_file() else {
        return false;
    };
    file.exists() || backup_of(&file).exists()
}

/// The store's backup name, which is the primary's with `.bak` appended.
fn backup_of(file: &std::path::Path) -> std::path::PathBuf {
    let mut name = file.file_name().unwrap_or_default().to_os_string();
    name.push(".bak");
    file.with_file_name(name)
}

/// Puts the machine back exactly as Cursed found it.
///
/// Or, when the record of how Cursed found it was destroyed by the update bug
/// this app shipped, back to the stock Windows scheme — which is a normal
/// working pointer and an honest one, rather than a guess dressed up as a
/// restore. [`stand_in_for_lost_capture`] has the reasoning.
pub fn restore() -> AppResult<()> {
    let snapshot = read_snapshot()?;
    if !snapshot.provenance.is_real() {
        log::warn!(
            "restoring the stock Windows scheme: this machine's original pointers were lost \
             before Cursed could record them"
        );
    }
    scheme::write_raw(
        &snapshot.values,
        snapshot.cursor_base_size,
        &snapshot.scheme_name,
    )?;
    // Drop the live overrides too, or the session keeps showing the old pointer
    // until the next sign-in and the restore looks like it did nothing.
    crate::cursor::engine::revert_live()?;
    Ok(())
}

/// Removes every scheme name Cursed registered in the Schemes list, so an
/// uninstall does not leave dropdown entries pointing at deleted files.
pub fn deregister_our_schemes() -> AppResult<()> {
    use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_SET_VALUE};
    use winreg::RegKey;

    let Ok(schemes) = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags(scheme::SCHEMES_KEY, KEY_READ | KEY_SET_VALUE)
    else {
        return Ok(());
    };

    let ours: Vec<String> = schemes
        .enum_values()
        .filter_map(Result::ok)
        .map(|(name, _)| name)
        .filter(|name| {
            name.starts_with(crate::cursor::SCHEME_PREFIX)
                || crate::cursor::legacy_scheme_prefix().is_some_and(|old| name.starts_with(old))
        })
        .collect();

    for name in ours {
        let _ = schemes.delete_value(&name);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every snapshot written before 1.21.0 has no `provenance` field, and every
    /// one of them is a real capture — the alternative did not exist yet. A
    /// default of anything else would relabel every existing user's good
    /// snapshot as lost and put a notice on screen about a bug they never hit.
    #[test]
    fn a_snapshot_written_before_the_field_existed_is_a_real_capture() {
        let old = r#"{
            "values": { "Arrow": "%SystemRoot%\\cursors\\aero_arrow.cur" },
            "cursor_base_size": 32,
            "scheme_name": "Windows Aero",
            "captured_at": "2026-08-01T00:00:00Z"
        }"#;
        let parsed: OriginalScheme = serde_json::from_str(old).expect("old snapshots must parse");
        assert_eq!(parsed.provenance, Provenance::Captured);
        assert!(parsed.provenance.is_real());
    }

    /// The stand-in must restore to *Windows'* pointers, which is spelled as
    /// every value being absent. A single non-empty value here would be a path
    /// written back into the registry, and the only paths available to write are
    /// Cursed's own — which is the exact outcome this whole branch exists to
    /// avoid.
    #[test]
    fn the_stand_in_restores_windows_own_pointers_and_nothing_else() {
        let stand_in = OriginalScheme {
            values: crate::cursor::roles::ALL_ROLES
                .into_iter()
                .map(|role| (role.registry_value().to_owned(), String::new()))
                .collect(),
            cursor_base_size: None,
            scheme_name: String::new(),
            captured_at: iso_now(),
            provenance: Provenance::Lost,
        };

        assert_eq!(stand_in.values.len(), 17, "all seventeen roles are reset");
        assert!(
            stand_in.values.values().all(String::is_empty),
            "an empty value is how `write_raw` is told to delete"
        );
        assert!(stand_in.scheme_name.is_empty(), "no scheme name to re-select");
        assert_eq!(stand_in.cursor_base_size, None);
        assert!(!stand_in.provenance.is_real());
    }

    /// The label has to survive the round trip through disk, or the notice is
    /// shown once and then forgotten on the next launch — and worse, the
    /// snapshot starts claiming to be a real capture.
    #[test]
    fn a_lost_snapshot_still_reads_as_lost_after_a_round_trip() {
        let lost = OriginalScheme {
            values: BTreeMap::new(),
            cursor_base_size: None,
            scheme_name: String::new(),
            captured_at: iso_now(),
            provenance: Provenance::Lost,
        };
        let json = serde_json::to_string(&lost).expect("serialise");
        assert!(json.contains("\"lost\""), "written in kebab-case: {json}");

        let back: OriginalScheme = serde_json::from_str(&json).expect("deserialise");
        assert_eq!(back.provenance, Provenance::Lost);
    }

    /// An adopted snapshot is somebody's real capture of this same machine, so
    /// it is worth exactly as much as our own would have been.
    #[test]
    fn an_adopted_snapshot_is_a_real_one() {
        assert!(Provenance::Adopted.is_real());
        assert!(Provenance::Captured.is_real());
        assert!(!Provenance::Lost.is_real());
    }

    /// The backup's name has to match what the store actually writes, or
    /// `snapshot_exists` misses a recoverable snapshot and `capture_once` walks
    /// straight past it.
    #[test]
    fn the_backup_is_looked_for_where_the_store_puts_it() {
        let file = std::path::Path::new(r"C:\x\backup\original_scheme.json");
        assert_eq!(
            backup_of(file),
            std::path::Path::new(r"C:\x\backup\original_scheme.json.bak")
        );
    }
}
