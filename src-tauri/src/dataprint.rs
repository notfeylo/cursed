//! A fingerprint of the data directory, so "the update did not touch your data"
//! can be checked rather than asserted.
//!
//! ## Why this is not a unit test
//!
//! The brief asks for a test that `%APPDATA%\Cursed` is byte-identical before
//! and after an update. No unit test can be that test. An update is a second
//! process, launched by a third, replacing the binary the test would be running
//! in — there is nothing to call and nothing left to assert with. Writing one
//! that *looked* like it covered this would be worse than not having it, because
//! the original data-loss bug's defining property was that it passed every test
//! in the suite while destroying users' files.
//!
//! So the assertion lives where it can actually run — `scripts/verify-release.ps1`,
//! on a VM, across a real N → N+1 update — and this module is the mechanism it
//! runs on. What *is* unit-tested here is the fingerprint itself: that it notices
//! a changed byte, a deleted file and an added one, and that it ignores exactly
//! the things that are expected to differ.
//!
//! ## What counts
//!
//! Three directories are excluded, and each for a reason that is not "it was
//! inconvenient":
//!
//! - `cache\` is rendered output. It is regenerated from the catalog on demand
//!   and its contents are a function of the settings, not of anything the user
//!   made.
//! - `logs\` is written on every launch, including the launch that runs this.
//! - `updates\` holds the downloaded installer, which the update being measured
//!   is in the middle of using.
//!
//! Everything else counts, and the four files under [`IRREPLACEABLE`] count
//! double: they are the ones whose loss cannot be undone by any amount of
//! re-doing work, and the comparison is expected to fail hard on those rather
//! than report a difference.

use crate::error::AppResult;
use crate::paths;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Directory names, relative to the root, that are not part of the fingerprint.
const VOLATILE_DIRS: [&str; 3] = ["cache", "logs", "updates"];

/// Suffixes that belong to a write in progress rather than to the user.
const VOLATILE_SUFFIXES: [&str; 1] = [".tmp"];

/// The files whose loss is permanent, in the order they matter.
///
/// `original_scheme.json` is first because it is the only one that cannot be
/// re-made by hand: presets can be rebuilt, cursors can be re-imported, settings
/// can be set again. What the machine's pointers were before Cursed ever ran is
/// readable exactly once, on the first launch, and never again.
pub const IRREPLACEABLE: [&str; 4] = [
    r"backup\original_scheme.json",
    "presets.json",
    "settings.json",
    "applied.json",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    /// Relative to the data root, backslash-separated, lowercased for
    /// comparison — Windows paths differ in case between two enumerations of
    /// the same directory more often than anyone expects.
    pub path: String,
    pub len: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataPrint {
    pub root: String,
    pub taken_at: String,
    /// Sorted by path, so two prints of the same tree are textually identical.
    pub entries: Vec<Entry>,
    /// One digest over the whole manifest, for a single-line comparison.
    pub digest: String,
}

impl DataPrint {
    pub fn find(&self, path: &str) -> Option<&Entry> {
        let wanted = path.to_ascii_lowercase();
        self.entries.iter().find(|entry| entry.path == wanted)
    }

    /// What changed between two prints, as a list a human can read.
    ///
    /// Returns `(changed, added, removed)`, each sorted.
    pub fn diff(&self, later: &DataPrint) -> (Vec<String>, Vec<String>, Vec<String>) {
        let mut changed = Vec::new();
        let mut removed = Vec::new();
        for entry in &self.entries {
            match later.find(&entry.path) {
                Some(after) if after != entry => changed.push(entry.path.clone()),
                Some(_) => {}
                None => removed.push(entry.path.clone()),
            }
        }
        let added = later
            .entries
            .iter()
            .filter(|entry| self.find(&entry.path).is_none())
            .map(|entry| entry.path.clone())
            .collect::<Vec<_>>();
        (changed, added, removed)
    }
}

/// Fingerprints this channel's data directory.
pub fn take() -> AppResult<DataPrint> {
    take_of(&paths::root()?)
}

/// Fingerprints an arbitrary directory. Split out so the tests can use a scratch
/// tree rather than the developer's own 395 MB of imported packs.
pub fn take_of(root: &Path) -> AppResult<DataPrint> {
    let mut entries = Vec::new();
    walk(root, root, &mut entries)?;
    // Sorted before the digest is taken, or the digest depends on the order the
    // filesystem happened to hand the directory back in.
    entries.sort_by(|a, b| a.path.cmp(&b.path));

    let manifest = entries
        .iter()
        .map(|e| format!("{} {} {}\n", e.path, e.len, e.sha256))
        .collect::<String>();

    Ok(DataPrint {
        root: root.to_string_lossy().to_string(),
        taken_at: crate::util::iso_now(),
        digest: crate::hash::sha256_hex(manifest.as_bytes()),
        entries,
    })
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<Entry>) -> AppResult<()> {
    let Ok(listing) = std::fs::read_dir(dir) else {
        // A directory that cannot be read is reported as empty rather than
        // failing the whole print. The comparison then shows its files as
        // removed, which is the honest answer and the one that draws attention.
        return Ok(());
    };

    for item in listing.flatten() {
        let path = item.path();
        let Some(relative) = relative_of(root, &path) else {
            continue;
        };

        if path.is_dir() {
            if VOLATILE_DIRS.contains(&relative.as_str()) {
                continue;
            }
            walk(root, &path, out)?;
            continue;
        }

        if VOLATILE_SUFFIXES
            .iter()
            .any(|suffix| relative.ends_with(suffix))
        {
            continue;
        }

        let bytes = std::fs::read(&path).unwrap_or_default();
        out.push(Entry {
            path: relative,
            len: bytes.len() as u64,
            sha256: crate::hash::sha256_hex(&bytes),
        });
    }
    Ok(())
}

fn relative_of(root: &Path, path: &Path) -> Option<String> {
    Some(
        path.strip_prefix(root)
            .ok()?
            .to_string_lossy()
            .replace('/', "\\")
            .to_ascii_lowercase(),
    )
}

/// Writes a print to a file, for `scripts/verify-release.ps1` to compare.
///
/// A file rather than stdout because the shipped binary is built with
/// `windows_subsystem = "windows"` and has no console attached — anything
/// printed goes nowhere, and a verification step whose output silently vanishes
/// is worse than none.
pub fn write_to(dest: &Path) -> AppResult<()> {
    let print = take()?;
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(dest, serde_json::to_string_pretty(&print)?)?;
    Ok(())
}

/// Compares two print files and returns a report plus whether it passed.
///
/// "Passed" is strict about [`IRREPLACEABLE`] and lenient about nothing else —
/// any other difference is still reported, it just does not by itself mean the
/// update destroyed something. A preset file rewritten because the app relaunched
/// and saved a window position is a difference; a missing `original_scheme.json`
/// is a disaster.
pub fn compare(before: &DataPrint, after: &DataPrint) -> (bool, String) {
    use std::fmt::Write;

    let (changed, added, removed) = before.diff(after);
    let mut report = String::new();

    let _ = writeln!(report, "before: {} entries, {}", before.entries.len(), before.digest);
    let _ = writeln!(report, "after:  {} entries, {}", after.entries.len(), after.digest);

    if before.digest == after.digest {
        let _ = writeln!(report, "\nidentical.");
        return (true, report);
    }

    let mut fatal = Vec::new();
    for name in IRREPLACEABLE {
        let name = name.to_ascii_lowercase();
        let was = before.find(&name);
        let now = after.find(&name);
        match (was, now) {
            (Some(_), None) => fatal.push(format!("{name} was DELETED")),
            (Some(a), Some(b)) if a != b => fatal.push(format!("{name} was MODIFIED")),
            _ => {}
        }
    }

    for (label, list) in [
        ("changed", &changed),
        ("added", &added),
        ("removed", &removed),
    ] {
        if list.is_empty() {
            continue;
        }
        let _ = writeln!(report, "\n{label} ({}):", list.len());
        for path in list {
            let _ = writeln!(report, "  {path}");
        }
    }

    if fatal.is_empty() {
        let _ = writeln!(
            report,
            "\nno irreplaceable file was touched; the differences above are not data loss."
        );
        (true, report)
    } else {
        let _ = writeln!(report, "\nFAILED:");
        for line in &fatal {
            let _ = writeln!(report, "  {line}");
        }
        (false, report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir()
            .join("cursorforge-dataprint-tests")
            .join(name);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("backup")).expect("scratch dir");
        std::fs::create_dir_all(dir.join("custom").join("a-cursor")).expect("scratch dir");
        std::fs::write(dir.join("settings.json"), r##"{"tint":"#2E8BFF"}"##).expect("write");
        std::fs::write(dir.join("presets.json"), "[]").expect("write");
        std::fs::write(dir.join("applied.json"), "{}").expect("write");
        std::fs::write(
            dir.join("backup").join("original_scheme.json"),
            r#"{"scheme_name":"Windows Aero"}"#,
        )
        .expect("write");
        std::fs::write(dir.join("custom").join("a-cursor").join("32.cur"), [0u8; 64])
            .expect("write");
        dir
    }

    #[test]
    fn the_same_tree_prints_the_same_digest_twice() {
        let dir = scratch("stable");
        let first = take_of(&dir).expect("print");
        let second = take_of(&dir).expect("print");
        assert_eq!(first.digest, second.digest);
        assert_eq!(first.entries.len(), 5);
    }

    /// The whole point. One byte of one file, and the digest has to move.
    #[test]
    fn a_single_changed_byte_is_visible() {
        let dir = scratch("changed");
        let before = take_of(&dir).expect("print");
        std::fs::write(dir.join("presets.json"), "[ ]").expect("write");
        let after = take_of(&dir).expect("print");

        assert_ne!(before.digest, after.digest);
        let (changed, added, removed) = before.diff(&after);
        assert_eq!(changed, vec!["presets.json"]);
        assert!(added.is_empty() && removed.is_empty());
    }

    /// The failure mode being guarded against, in miniature: the update deletes
    /// the one file that cannot be re-made.
    #[test]
    fn a_deleted_snapshot_fails_the_comparison() {
        let dir = scratch("deleted");
        let before = take_of(&dir).expect("print");
        std::fs::remove_file(dir.join("backup").join("original_scheme.json")).expect("remove");
        let after = take_of(&dir).expect("print");

        let (passed, report) = compare(&before, &after);
        assert!(!passed, "losing the snapshot must fail:\n{report}");
        assert!(report.contains("DELETED"), "{report}");
    }

    /// And the whole directory going, which is what actually happened.
    #[test]
    fn losing_everything_fails_the_comparison() {
        let dir = scratch("wiped");
        let before = take_of(&dir).expect("print");
        std::fs::remove_dir_all(&dir).expect("remove");
        std::fs::create_dir_all(&dir).expect("recreate");
        let after = take_of(&dir).expect("print");

        assert!(after.entries.is_empty());
        let (passed, report) = compare(&before, &after);
        assert!(!passed, "{report}");
        // Every irreplaceable file, named, not just a count.
        for name in IRREPLACEABLE {
            assert!(report.contains(&name.to_ascii_lowercase()), "{name} missing from:\n{report}");
        }
    }

    /// Rendered cursors, logs and the installer being run are all expected to
    /// differ across an update. Counting them would make every run fail and the
    /// check would be turned off within a week.
    #[test]
    fn the_volatile_directories_are_ignored() {
        let dir = scratch("volatile");
        let before = take_of(&dir).expect("print");

        for volatile in VOLATILE_DIRS {
            let sub = dir.join(volatile);
            std::fs::create_dir_all(&sub).expect("mkdir");
            std::fs::write(sub.join("something.bin"), b"noise").expect("write");
        }
        std::fs::write(dir.join("settings.json.tmp"), b"half-written").expect("write");

        let after = take_of(&dir).expect("print");
        assert_eq!(
            before.digest, after.digest,
            "cache, logs, updates and a partial write are not the user's data"
        );
    }

    /// A `.bak` is the user's data — it is the copy a recovery reads from — so
    /// it is counted. Excluding it would hide exactly the case where the primary
    /// survived and its backup did not.
    #[test]
    fn a_backup_file_counts() {
        let dir = scratch("bak");
        let before = take_of(&dir).expect("print");
        std::fs::write(dir.join("presets.json.bak"), "[]").expect("write");
        let after = take_of(&dir).expect("print");

        let (_, added, _) = before.diff(&after);
        assert_eq!(added, vec!["presets.json.bak"]);
    }

    /// An added file is a difference but not a loss, so it is reported and does
    /// not fail. An update that writes a new state file is doing its job.
    #[test]
    fn an_added_file_is_reported_without_failing() {
        let dir = scratch("added");
        let before = take_of(&dir).expect("print");
        std::fs::write(dir.join("window.json"), "{}").expect("write");
        let after = take_of(&dir).expect("print");

        let (passed, report) = compare(&before, &after);
        assert!(passed, "{report}");
        assert!(report.contains("window.json"), "{report}");
    }

    /// The manifest is written and read back by a PowerShell script across two
    /// processes, so the shape has to survive a round trip through JSON.
    #[test]
    fn a_print_survives_a_round_trip_through_json() {
        let dir = scratch("json");
        let print = take_of(&dir).expect("print");
        let text = serde_json::to_string(&print).expect("serialise");
        let back: DataPrint = serde_json::from_str(&text).expect("deserialise");

        assert_eq!(back.digest, print.digest);
        assert_eq!(back.entries, print.entries);
    }
}
