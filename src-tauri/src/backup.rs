//! Everything the user made, in one file they can put somewhere else.
//!
//! ## Why this exists
//!
//! Because the app deleted people's data. Every argument for a backup feature is
//! usually hypothetical; here it is a thing that happened, to real installs,
//! through three releases. The fix stops it recurring and gives nobody their
//! presets back.
//!
//! So: one button, one archive, no cloud, no account, no format anybody has to
//! learn. A `.zip` the user can put on a memory stick.
//!
//! ## What goes in
//!
//! The same split [`crate::dataprint`] draws, for the same reason: `cache\`,
//! `logs\` and `updates\` are regenerated, rewritten every launch, and holding a
//! downloaded installer respectively. A backup that includes them is mostly a
//! copy of things nobody wants back, and the one that matters —
//! `backup\original_scheme.json`, four kilobytes — gets lost in it.
//!
//! ## Restoring is a merge, not a replacement
//!
//! Restore writes the archive's files over the data directory and leaves
//! anything not in the archive alone. Wiping first would be tidier and is the
//! wrong trade: a user restoring a three-month-old backup to recover one preset
//! should not lose the twenty cursors they have imported since. If they want the
//! old state exactly, deleting the directory first is a thing they can do and
//! choose; silently doing it for them is not.
//!
//! ## The archive is treated as hostile
//!
//! It arrives from a filesystem, which means it may have been edited, and the
//! same rules as `packs::cfpack` apply: every entry name is validated before a
//! byte is written, nothing lands outside the data directory, and the extension
//! allow-list has no executable on it.

use crate::error::{AppError, AppResult};
use crate::paths;
use std::io::{Read, Write};
use std::path::Path;

/// Not backed up: regenerated, per-launch, or a download in flight.
const SKIPPED_DIRS: [&str; 3] = ["cache", "logs", "updates"];

/// What a restore is willing to write.
///
/// No `.exe`, no `.dll`, no `.lnk`, no `.ps1`. A backup archive is a file that
/// arrives from outside the app, and the data directory is a place the app runs
/// things from — `custom\` holds cursors, and a cursor is a file Windows loads.
const ALLOWED_EXTENSIONS: [&str; 6] = ["json", "cur", "ani", "png", "svg", "txt"];

const MAX_ENTRIES: usize = 20_000;
const MAX_UNCOMPRESSED: u64 = 2 * 1024 * 1024 * 1024;
/// One entry expanding more than 200x is a zip bomb whatever it claims to be.
const MAX_RATIO: u64 = 200;

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupReport {
    pub path: String,
    pub files: usize,
    pub bytes: u64,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreReport {
    pub files: usize,
    pub skipped: usize,
    /// Named so the user is told rather than left to notice.
    pub problems: Vec<String>,
}

/// Writes every file worth keeping into one zip.
pub fn export(dest: &Path) -> AppResult<BackupReport> {
    if dest.extension().is_none_or(|e| e != "zip") {
        return Err(AppError::invalid("a backup must be saved with a .zip name"));
    }

    let root = paths::root()?;
    let mut names = Vec::new();
    collect(&root, &root, &mut names)?;

    let file = std::fs::File::create(dest)?;
    let mut zip = zip::ZipWriter::new(file);
    let options: zip::write::FileOptions<'_, ()> =
        zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    let mut bytes = 0u64;
    for relative in &names {
        let source = root.join(relative);
        let Ok(contents) = std::fs::read(&source) else {
            // A file that vanished between the listing and the read is not worth
            // failing a backup over. It is worth not pretending it is in there.
            log::warn!("backup: {} could not be read and is not in the archive", source.display());
            continue;
        };
        // Forward slashes: the zip spec says so, and an archive written with
        // backslashes extracts as one file with a strange name everywhere but
        // Windows.
        zip.start_file(relative.replace('\\', "/"), options)
            .map_err(|e| AppError::storage(format!("could not add {relative}: {e}")))?;
        zip.write_all(&contents)?;
        bytes += contents.len() as u64;
    }
    zip.finish()
        .map_err(|e| AppError::storage(format!("could not finish the backup: {e}")))?;

    log::info!("backup: {} files, {bytes} bytes -> {}", names.len(), dest.display());
    Ok(BackupReport {
        path: dest.to_string_lossy().into_owned(),
        files: names.len(),
        bytes,
    })
}

fn collect(root: &Path, dir: &Path, out: &mut Vec<String>) -> AppResult<()> {
    let Ok(listing) = std::fs::read_dir(dir) else {
        return Ok(());
    };
    for item in listing.flatten() {
        let path = item.path();
        let Ok(relative) = path.strip_prefix(root) else {
            continue;
        };
        let relative = relative.to_string_lossy().replace('/', "\\");

        if path.is_dir() {
            if SKIPPED_DIRS.contains(&relative.to_ascii_lowercase().as_str()) {
                continue;
            }
            collect(root, &path, out)?;
        } else {
            out.push(relative);
        }
    }
    out.sort();
    Ok(())
}

/// Restores an archive over the data directory.
pub fn import(src: &Path) -> AppResult<RestoreReport> {
    let root = paths::root()?;
    let file = std::fs::File::open(src)
        .map_err(|_| AppError::invalid("that backup could not be opened"))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|_| AppError::invalid("that file is not a Cursed backup"))?;

    if archive.len() > MAX_ENTRIES {
        return Err(AppError::invalid("that archive holds more files than a backup ever should"));
    }

    let mut written = 0usize;
    let mut skipped = 0usize;
    let mut problems: Vec<String> = Vec::new();
    let mut total = 0u64;

    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|_| AppError::invalid("that backup is damaged"))?;
        if entry.is_dir() {
            continue;
        }

        // `enclosed_name` is the zip crate's own zip-slip check: it returns
        // `None` for an absolute path, a drive letter, or anything containing
        // `..`. Ours is checked on top of it rather than instead, because that
        // one guards traversal and this one guards *what* may be written.
        let Some(name) = entry.enclosed_name() else {
            skipped += 1;
            problems.push(format!("entry {index} has a name that would write outside the folder"));
            continue;
        };

        let relative = name.to_string_lossy().to_string();
        if paths::validate_relative(&relative).is_err() {
            skipped += 1;
            problems.push(format!("{relative} is not a name this app will write"));
            continue;
        }

        let extension = name
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if !ALLOWED_EXTENSIONS.contains(&extension.as_str()) {
            skipped += 1;
            problems.push(format!("{relative} is not a file type a backup contains"));
            continue;
        }

        let declared = entry.size();
        let compressed = entry.compressed_size().max(1);
        if declared / compressed > MAX_RATIO {
            skipped += 1;
            problems.push(format!("{relative} expands more than {MAX_RATIO}x"));
            continue;
        }
        total = total.saturating_add(declared);
        if total > MAX_UNCOMPRESSED {
            return Err(AppError::invalid("that archive expands to more than this app will write"));
        }

        // Read through a limited reader rather than trusting the declared size:
        // the header is written by whoever made the archive, and the only
        // number that cannot lie is how many bytes actually arrive.
        let mut contents = Vec::new();
        entry
            .by_ref()
            .take(MAX_UNCOMPRESSED - total.min(MAX_UNCOMPRESSED) + declared)
            .read_to_end(&mut contents)?;

        let destination = root.join(&relative);
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // Through the store for the state files, so a restore interrupted
        // half-way leaves the previous copy rather than a truncated one.
        match extension.as_str() {
            "json" => {
                let text = String::from_utf8_lossy(&contents).into_owned();
                crate::state::store::write(&destination, &text)?;
            }
            _ => std::fs::write(&destination, &contents)?,
        }
        written += 1;
    }

    log::info!("restore: {written} files written, {skipped} skipped, from {}", src.display());
    Ok(RestoreReport {
        files: written,
        skipped,
        problems: problems.into_iter().take(10).collect(),
    })
}

/// Where a backup should be offered by default, and under what name.
///
/// Dated, because the second thing anybody does with a backup is make another
/// one, and `cursed-backup.zip` overwriting `cursed-backup.zip` is how a good
/// copy is lost to a bad one.
pub fn suggested_name() -> String {
    let today = crate::util::iso_now();
    let day = today.split('T').next().unwrap_or("backup").to_owned();
    format!("cursed-backup-{day}.zip")
}

/// The path this file would restore to, or `None` if it must not be written.
///
/// Extracted so the rules can be tested without building an archive: the checks
/// are the point, and every one of them is a way a hostile zip gets refused.
#[cfg(test)]
fn permitted(relative: &str) -> Option<std::path::PathBuf> {
    let path = paths::validate_relative(relative).ok()?;
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if !ALLOWED_EXTENSIONS.contains(&extension.as_str()) {
        return None;
    }
    Some(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_backup_must_be_named_as_a_zip() {
        assert!(export(Path::new("C:/x/backup.rar")).is_err());
        assert!(export(Path::new("C:/x/backup")).is_err());
    }

    /// Every one of these is an archive entry that has to be refused. The first
    /// two would write outside the data directory; the rest would put something
    /// executable inside it, where the app loads files from.
    #[test]
    fn a_hostile_archive_entry_is_refused() {
        for hostile in [
            r"..\..\Windows\System32\evil.cur",
            r"C:\Windows\System32\evil.cur",
            "custom/payload.exe",
            "custom/payload.dll",
            "custom/payload.ps1",
            "custom/payload.lnk",
            "custom/payload.bat",
            "settings.json.exe",
            "NUL.cur",
            "custom/stream.cur:hidden",
        ] {
            assert!(permitted(hostile).is_none(), "{hostile} should be refused");
        }
    }

    /// And the things a real backup is made of are accepted.
    #[test]
    fn the_files_a_backup_actually_contains_are_allowed() {
        for ordinary in [
            "settings.json",
            "presets.json",
            "applied.json",
            r"backup\original_scheme.json",
            r"custom\my-logo-8f1a\32.cur",
            r"custom\my-logo-8f1a\64.ani",
            r"custom\my-logo-8f1a\source.png",
            r"imported\some-pack\arrow.cur",
        ] {
            assert!(permitted(ordinary).is_some(), "{ordinary} should be allowed");
        }
    }

    /// The directories a backup deliberately leaves out. Including them would
    /// bury the four kilobytes that matter under a few hundred megabytes of
    /// rendered cursors that regenerate on demand anyway.
    #[test]
    fn the_regenerated_directories_are_not_backed_up() {
        assert_eq!(SKIPPED_DIRS.len(), 3);
        for skipped in ["cache", "logs", "updates"] {
            assert!(SKIPPED_DIRS.contains(&skipped));
        }
        // And the one that must never be on that list.
        assert!(!SKIPPED_DIRS.contains(&"backup"), "the snapshot is the point");
        assert!(!SKIPPED_DIRS.contains(&"custom"));
    }

    /// Two backups on two days must not be one file.
    #[test]
    fn the_suggested_name_is_dated_and_a_zip() {
        let name = suggested_name();
        assert!(name.starts_with("cursed-backup-"), "{name}");
        assert!(name.ends_with(".zip"), "{name}");
        // `YYYY-MM-DD` between the prefix and the extension.
        let day = name
            .trim_start_matches("cursed-backup-")
            .trim_end_matches(".zip");
        assert_eq!(day.len(), 10, "{name}");
        assert!(day.chars().all(|c| c.is_ascii_digit() || c == '-'), "{name}");
    }
}
