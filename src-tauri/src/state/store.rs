//! Every state file this app owns is read and written through here.
//!
//! Settings, presets, the applied-cursor descriptor and the window position are
//! four files written by four modules that each had their own idea of what
//! "save" meant. Three did a temp-then-rename; one wrote straight over the top.
//! None of them flushed, none of them kept a copy, and one of them treated a
//! file it could not parse as an empty one — which is not a recovery, it is the
//! deletion happening quietly on the next save.
//!
//! ## What "atomic" actually requires
//!
//! `write` then `rename` is the right shape and is not sufficient on its own.
//! The rename is atomic; the *write* is not durable until the data has reached
//! the disk, and until then the filesystem is free to record the rename while
//! the contents are still in a cache. A power cut in that window leaves a file
//! that exists, has the right name, and contains nothing — or worse, the tail of
//! whatever occupied those blocks before. So the sequence here is write, flush,
//! **`sync_all`**, then rename. `sync_all` is the step that was missing
//! everywhere, and it is the only one that talks to the hardware.
//!
//! ## Why there is a `.bak`
//!
//! Atomicity protects the *transition*. It does nothing about a file that was
//! written perfectly and is wrong — a serialisation bug, a disk that flipped a
//! bit, an antivirus that truncated it. The previous good copy is kept beside
//! every file for exactly that, and is used automatically when the primary will
//! not parse.
//!
//! ## Why a corrupt file is never simply discarded
//!
//! It is moved aside, not overwritten. A user whose presets stopped loading has
//! lost nothing that a text editor could not get back, and the alternative —
//! defaulting to empty and saving over it a moment later — turns a recoverable
//! problem into a permanent one.

use crate::error::AppResult;
use serde::de::DeserializeOwned;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Where a file's previous good copy lives.
fn backup_path(file: &Path) -> PathBuf {
    with_suffix(file, "bak")
}

/// `settings.json` -> `settings.json.<suffix>`.
///
/// Appended rather than `with_extension`, which would turn `settings.json` into
/// `settings.bak` and put the backup of one file on top of another's name the
/// moment two files in a directory differ only by extension.
fn with_suffix(file: &Path, suffix: &str) -> PathBuf {
    let mut name = file.file_name().unwrap_or_default().to_os_string();
    name.push(".");
    name.push(suffix);
    file.with_file_name(name)
}

/// Writes a state file durably, keeping the previous contents beside it.
///
/// The order matters and is the whole point: the backup is taken **before** the
/// new bytes are anywhere, the new bytes are made durable **before** they are
/// given the real name, and the rename is last because it is the only step that
/// cannot half-happen.
pub fn write(file: &Path, contents: &str) -> AppResult<()> {
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // The copy is best-effort. Refusing to save because a spare could not be
    // written would trade a small risk for a certain failure.
    if file.is_file() {
        if let Err(e) = std::fs::copy(file, backup_path(file)) {
            log::warn!("could not back up {} before writing: {e}", file.display());
        }
    }

    let temp = with_suffix(file, "tmp");
    {
        let mut handle = std::fs::File::create(&temp)?;
        handle.write_all(contents.as_bytes())?;
        handle.flush()?;
        // The step that makes the rename below mean something. Without it the
        // name can land before the bytes do.
        handle.sync_all()?;
    }
    std::fs::rename(&temp, file)?;
    Ok(())
}

/// Where a value came from, so the caller can say so.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// Parsed from the file itself.
    Primary,
    /// The file would not parse; this came from the `.bak` beside it.
    Backup,
    /// Nothing readable. The caller's defaults are in use.
    Missing,
}

/// Reads a state file, falling back to its backup, then to `T::default()`.
///
/// Never returns an error. A state file that cannot be read must not stop the
/// app from starting — an app that refuses to open because its window position
/// is malformed is worse than one that opens in the wrong place.
pub fn read<T: DeserializeOwned + Default>(file: &Path) -> (T, Source) {
    match parse(file) {
        Some(value) => (value, Source::Primary),
        None => {
            // Only now is the primary worth worrying about. `parse` returns
            // `None` for "not there" as well, which is every first run.
            if file.is_file() {
                log::error!("{} could not be read; trying the backup", file.display());
                preserve_corrupt(file);
            }
            match parse(&backup_path(file)) {
                Some(value) => {
                    // Put the good copy back under the real name.
                    //
                    // Not a tidiness measure. `preserve_corrupt` has just moved
                    // the damaged primary aside, so without this the file is
                    // *absent* on the next launch — and callers reasonably treat
                    // absent as "nothing was ever saved". For most files that
                    // means starting empty; for the original-scheme snapshot it
                    // means capturing the Cursed cursor currently on screen and
                    // recording it as the machine's true original, which is
                    // permanent and silent. Recovery has to leave the file where
                    // the next read will find it.
                    if let Err(e) = std::fs::copy(backup_path(file), file) {
                        log::warn!("could not restore {} from its backup: {e}", file.display());
                    }
                    log::warn!("recovered {} from its backup", file.display());
                    (value, Source::Backup)
                }
                None => (T::default(), Source::Missing),
            }
        }
    }
}

fn parse<T: DeserializeOwned>(file: &Path) -> Option<T> {
    let text = std::fs::read_to_string(file).ok()?;
    serde_json::from_str(crate::util::strip_bom(&text)).ok()
}

/// Moves a file that would not parse out of the way instead of over-writing it.
///
/// Named once and reused, so repeated failed launches cannot fill the directory
/// with a copy per start. Whatever the user lost is one rename from being back.
fn preserve_corrupt(file: &Path) {
    let kept = with_suffix(file, "corrupt");
    if let Err(e) = std::fs::rename(file, &kept) {
        log::warn!("could not set {} aside: {e}", file.display());
    } else {
        log::error!("{} was unreadable and has been kept as {}", file.display(), kept.display());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
    struct Thing {
        name: String,
        count: u32,
    }

    fn scratch(test: &str) -> PathBuf {
        let dir = std::env::temp_dir().join("cursorforge-store-tests").join(test);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch dir");
        dir.join("state.json")
    }

    #[test]
    fn a_written_file_reads_back() {
        let file = scratch("roundtrip");
        let thing = Thing { name: "arrow".into(), count: 17 };
        write(&file, &serde_json::to_string_pretty(&thing).unwrap()).unwrap();

        let (read_back, source) = read::<Thing>(&file);
        assert_eq!(read_back, thing);
        assert_eq!(source, Source::Primary);
    }

    /// The temp file is an implementation detail and must not survive the save.
    /// One left behind is a file the next write silently overwrites, which is
    /// fine — and a file a user sees in their profile, which is not.
    #[test]
    fn nothing_is_left_behind_by_a_save() {
        let file = scratch("no-litter");
        write(&file, "{}").unwrap();
        assert!(!with_suffix(&file, "tmp").exists());
    }

    /// The second save is the one that can lose data: it is the first that has
    /// something to overwrite.
    #[test]
    fn the_previous_contents_are_kept_beside_the_file() {
        let file = scratch("backup");
        write(&file, r#"{"name":"first","count":1}"#).unwrap();
        write(&file, r#"{"name":"second","count":2}"#).unwrap();

        let (current, _) = read::<Thing>(&file);
        assert_eq!(current.name, "second");

        let (kept, _) = read::<Thing>(&backup_path(&file));
        assert_eq!(kept.name, "first", "the backup should hold what was there before");
    }

    /// The failure this whole module exists for: a file that parsed yesterday
    /// and does not today. Defaulting to empty and saving over it is how a
    /// recoverable problem becomes a permanent one.
    #[test]
    fn a_corrupt_file_falls_back_to_the_backup_rather_than_to_nothing() {
        let file = scratch("corrupt");
        write(&file, r#"{"name":"real","count":7}"#).unwrap();
        write(&file, r#"{"name":"newer","count":8}"#).unwrap();

        // Something truncates it mid-write.
        std::fs::write(&file, r#"{"name":"newe"#).unwrap();

        let (recovered, source) = read::<Thing>(&file);
        assert_eq!(source, Source::Backup);
        assert_eq!(recovered.name, "real", "the backup is the last good copy");
    }

    /// Recovery has to leave the file behind, not just return its contents.
    ///
    /// The damaged primary is moved aside during a recovery, so a read that
    /// stopped there would leave no file at all — and "no file" is how every
    /// caller spells "nothing was ever saved". For the original-scheme snapshot
    /// that mistake is unrecoverable: the next launch would capture whatever
    /// Cursed itself had applied and record it as the machine's true original.
    #[test]
    fn a_recovery_puts_the_file_back_where_the_next_read_will_find_it() {
        let file = scratch("healed");
        write(&file, r#"{"name":"good","count":1}"#).unwrap();
        write(&file, r#"{"name":"newer","count":2}"#).unwrap();
        std::fs::write(&file, "}{ truncated").unwrap();

        let (_recovered, source) = read::<Thing>(&file);
        assert_eq!(source, Source::Backup);

        // The very next read must be an ordinary one.
        let (again, source) = read::<Thing>(&file);
        assert_eq!(source, Source::Primary, "the file should have been healed");
        assert_eq!(again.name, "good");
    }

    /// And the unreadable bytes are kept, because they are the user's.
    #[test]
    fn a_corrupt_file_is_set_aside_not_destroyed() {
        let file = scratch("preserved");
        std::fs::write(&file, "this was never json").unwrap();

        let (_value, source) = read::<Thing>(&file);
        assert_eq!(source, Source::Missing, "no backup exists to recover from");

        let kept = with_suffix(&file, "corrupt");
        assert!(kept.is_file(), "the unreadable file must still exist somewhere");
        assert_eq!(std::fs::read_to_string(&kept).unwrap(), "this was never json");
    }

    /// A first run has no file, no backup and nothing wrong. It must not log an
    /// error or leave a `.corrupt` behind.
    #[test]
    fn a_missing_file_is_not_a_corrupt_one() {
        let file = scratch("first-run");
        let (value, source) = read::<Thing>(&file);
        assert_eq!(value, Thing::default());
        assert_eq!(source, Source::Missing);
        assert!(!with_suffix(&file, "corrupt").exists());
    }

    /// The backup must not collide with a sibling that differs only by
    /// extension — `with_extension` would turn both `a.json` and `a.txt` into
    /// `a.bak`, and one file's history would overwrite the other's.
    #[test]
    fn the_backup_name_appends_rather_than_replaces() {
        let json = Path::new(r"C:\x\settings.json");
        assert_eq!(backup_path(json), Path::new(r"C:\x\settings.json.bak"));
    }
}
