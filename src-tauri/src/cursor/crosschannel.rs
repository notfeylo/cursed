//! The one thing two channels cannot each have their own of.
//!
//! Everything else Cursed touches can be named per channel — its data
//! directory, its install directory, its tray icon, its autostart entry. The
//! pointer scheme cannot. `HKCU\Control Panel\Cursors` is a single set of
//! seventeen values per Windows user, and both channels write to it, because
//! writing to it is what applying a cursor *is*.
//!
//! Two consequences follow, and both are silent failures rather than errors:
//!
//! **The watchdog fights itself.** `cursor::watchdog` notices when the scheme
//! stops matching what this build applied and puts it back. Two builds with
//! different schemes applied each read the other's write as drift, so they
//! revert each other every few seconds, indefinitely, and the pointer flickers
//! between two cursors with nothing in either log that looks like a cause.
//!
//! **The safety snapshot gets overwritten with a lie.** `restore::capture_once`
//! is idempotent, but only against its own data directory. A second channel
//! starting up has an empty one, so it captures — and what it captures is
//! whatever the *first* channel has applied. That channel's "restore the
//! original Windows pointers" then restores the other channel's cursor, for
//! ever, and nothing about it looks broken until someone tries it.
//!
//! ## What this module does about it
//!
//! Ownership is a **named mutex**, because a mutex is the one lock Windows
//! cleans up on our behalf. A lock file has to record a pid and a timestamp and
//! then be second-guessed — is that process alive, is that timestamp stale, was
//! the machine reset — and every one of those questions is a way to deadlock a
//! developer's own machine. A mutex abandoned by a dying process is reported as
//! `WAIT_ABANDONED` to the next waiter, which simply takes it. Killing the
//! holder is therefore not a special case; it is the normal path.
//!
//! Identity is a small registry record, and it is **advisory only** — it exists
//! so the other channel can say *which* build has the pointer, and so
//! `npm run channels` can print it. Nothing that matters for correctness reads
//! it. A stale record can mislabel a banner; it cannot wedge anything.
//!
//! The snapshot is the exception: that record is durable and authoritative, and
//! it is written once, by whichever channel captured first. See
//! [`adopt_foreign_snapshot`].

use crate::channel;
use crate::error::AppResult;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use windows::core::PCWSTR;
use windows::Win32::Foundation::{HANDLE, WAIT_ABANDONED, WAIT_OBJECT_0};
use windows::Win32::System::Threading::{CreateMutexW, WaitForSingleObject};
use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_WRITE};
use winreg::RegKey;

/// Whether this process holds the pointer lock.
static OWNED: AtomicBool = AtomicBool::new(false);

/// The mutex handle, kept for the life of the process.
///
/// Windows releases a mutex when the owning process ends, so there is nothing to
/// close on the way out and no path where a crash leaves the lock held. The
/// handle is only stored so it is not dropped while we still own what it
/// represents.
static HELD: std::sync::Mutex<Option<isize>> = std::sync::Mutex::new(None);

const OWNER_CHANNEL: &str = "PointerOwnerChannel";
const OWNER_PID: &str = "PointerOwnerPid";
const OWNER_SINCE: &str = "PointerOwnerSince";
const SNAPSHOT_CHANNEL: &str = "OriginalSchemeChannel";
const SNAPSHOT_PATH: &str = "OriginalSchemePath";
const SNAPSHOT_CAPTURED: &str = "OriginalSchemeCapturedAt";

/// Who holds the pointer, for the banner and for `npm run channels`.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Holder {
    /// `user` or `dev`.
    pub channel: String,
    /// What to call it in a sentence: `Cursed` or `Cursed Dev`.
    pub product_name: String,
    pub pid: u32,
    pub since: String,
    /// True when the holder is this very process.
    pub is_us: bool,
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Takes the pointer lock if it is free, or if whoever held it has gone.
///
/// Safe to call repeatedly: a process that already owns the lock returns `true`
/// without touching Windows, and one that does not tries again. The watchdog
/// calls this on its own tick, which is what makes "quit the other channel and
/// this one takes over" work without either of them being restarted.
pub fn try_claim() -> bool {
    if OWNED.load(Ordering::Relaxed) {
        return true;
    }

    let name = wide(channel::POINTER_LOCK);
    // SAFETY: the name buffer is NUL-terminated and outlives the call. Asking
    // for no initial ownership on purpose — ownership is decided by the wait
    // below, which is the only call that can distinguish "free" from
    // "abandoned by a process that died".
    let handle: HANDLE = match unsafe { CreateMutexW(None, false, PCWSTR(name.as_ptr())) } {
        Ok(handle) => handle,
        Err(e) => {
            // Without a lock the safe assumption is that we do not own the
            // pointer, so the watchdog stays out of it rather than joining a
            // fight it cannot detect.
            log::warn!("cross-channel: could not open the pointer lock: {e}");
            return false;
        }
    };

    // SAFETY: `handle` came from `CreateMutexW` above and is still open.
    let status = unsafe { WaitForSingleObject(handle, 0) };
    let acquired = match status {
        WAIT_OBJECT_0 => true,
        WAIT_ABANDONED => {
            // The previous holder died without releasing. Windows hands the
            // mutex over and tells us why; there is nothing to repair, because
            // the pointer scheme is whatever it is and this channel is about to
            // assert its own.
            log::info!("cross-channel: the previous pointer owner exited without releasing");
            true
        }
        _ => false,
    };

    if acquired {
        if let Ok(mut held) = HELD.lock() {
            *held = Some(handle.0 as isize);
        }
        OWNED.store(true, Ordering::Relaxed);
        record_ownership();
        log::info!(
            "cross-channel: this channel ({}) holds the pointer",
            channel::NAME
        );
    } else {
        match holder() {
            Some(other) => log::info!(
                "cross-channel: {} already holds the pointer (pid {})",
                other.product_name,
                other.pid
            ),
            None => log::info!("cross-channel: another process already holds the pointer"),
        }
    }

    acquired
}

/// Whether this process may write the pointer scheme unprompted.
///
/// Applying a cursor because someone clicked one is always allowed — that is a
/// direct instruction from the person at the keyboard, and both channels write
/// the same seventeen values. This governs the *unprompted* writes: the
/// watchdog's reverts, which are the ones that fight.
pub fn owns_pointer() -> bool {
    OWNED.load(Ordering::Relaxed)
}

/// Opens the shared key for writing — and refuses to under `cargo test`.
///
/// The tests below exercise the real lock against the real machine, because a
/// named mutex has no seam worth faking: the behaviour under test *is* what
/// Windows does with it. Recording is different. `record_snapshot` is durable
/// and first-writer-wins, so a test process that reached it would permanently
/// claim the machine's original pointer scheme on behalf of a run that lasted
/// eight seconds — and `record_ownership` would leave `npm run channels`
/// reporting a channel that was never installed. Neither is recoverable by
/// running the tests again.
fn key_for_write() -> Option<RegKey> {
    if cfg!(test) {
        return None;
    }
    RegKey::predef(HKEY_CURRENT_USER)
        .create_subkey(channel::CROSS_CHANNEL_KEY)
        .ok()
        .map(|(key, _)| key)
}

fn key_for_read() -> Option<RegKey> {
    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags(channel::CROSS_CHANNEL_KEY, KEY_READ)
        .ok()
}

fn record_ownership() {
    let Some(key) = key_for_write() else {
        return;
    };
    // Advisory. A failure here costs a banner its accuracy and nothing else, so
    // none of these are worth failing a launch over.
    let _ = key.set_value(OWNER_CHANNEL, &channel::NAME);
    let _ = key.set_value(OWNER_PID, &std::process::id());
    let _ = key.set_value(OWNER_SINCE, &crate::util::iso_now());
}

/// Who currently claims the pointer, if anyone has ever said.
pub fn holder() -> Option<Holder> {
    let key = key_for_read()?;
    let name: String = key.get_value(OWNER_CHANNEL).ok()?;
    let pid: u32 = key.get_value(OWNER_PID).unwrap_or(0);
    let since: String = key.get_value(OWNER_SINCE).unwrap_or_default();
    let is_us = name == channel::NAME && pid == std::process::id();
    Some(Holder {
        product_name: if name == "dev" { "Cursed Dev" } else { "Cursed" }.to_owned(),
        channel: name,
        pid,
        since,
        is_us,
    })
}

/// What to tell the person at the keyboard when this channel is standing down.
///
/// `None` when there is nothing to say — either we hold the pointer, or nobody
/// does.
pub fn deferral_notice() -> Option<String> {
    if owns_pointer() {
        return None;
    }
    let other = holder()?;
    if other.is_us {
        return None;
    }
    Some(format!(
        "{} is currently managing the cursor, so this copy will not defend the scheme.",
        other.product_name
    ))
}

/// The channel that captured the machine's true original scheme, and where it
/// put the file — the one durable, authoritative thing recorded here.
fn recorded_snapshot() -> Option<(String, PathBuf)> {
    let key = key_for_read()?;
    let owner: String = key.get_value(SNAPSHOT_CHANNEL).ok()?;
    let path: String = key.get_value(SNAPSHOT_PATH).ok()?;
    Some((owner, PathBuf::from(path)))
}

/// Records that this channel captured the original scheme — **only** if no
/// channel has claimed it before.
///
/// First capture wins, permanently. The first channel to run on a machine is the
/// only one that ever sees the true pre-Cursed pointers; every later claim would
/// be a claim about a scheme Cursed itself applied.
pub fn record_snapshot(path: &std::path::Path, captured_at: &str) {
    if recorded_snapshot().is_some() {
        return;
    }
    let Some(key) = key_for_write() else {
        return;
    };
    let _ = key.set_value(SNAPSHOT_CHANNEL, &channel::NAME);
    let _ = key.set_value(SNAPSHOT_PATH, &path.to_string_lossy().to_string());
    let _ = key.set_value(SNAPSHOT_CAPTURED, &captured_at);
    log::info!(
        "cross-channel: this channel ({}) captured the original scheme",
        channel::NAME
    );
}

/// The other channel's snapshot, if one exists and this channel has none.
///
/// Returns the file's contents verbatim so the caller can write its own copy.
/// **Copying rather than sharing is deliberate.** A single shared file would be
/// deleted by whichever channel is uninstalled first, taking the other channel's
/// only record of the machine's real pointers with it. Two identical copies
/// survive either uninstall, and the contents never change after capture, so
/// they cannot drift apart.
pub fn adopt_foreign_snapshot() -> Option<String> {
    let (owner, path) = recorded_snapshot()?;
    if owner == channel::NAME {
        return None;
    }
    match std::fs::read_to_string(&path) {
        Ok(text) => {
            log::info!(
                "cross-channel: adopting the original scheme captured by the {owner} channel \
                 rather than capturing the cursor that channel has applied"
            );
            Some(text)
        }
        Err(e) => {
            // The other channel was uninstalled, or its data directory was
            // cleared. Nothing can be adopted, and the caller falls back to
            // capturing — which is the best available answer even though what
            // is on screen may be a Cursed cursor.
            log::warn!("cross-channel: the {owner} channel's snapshot could not be read: {e}");
            None
        }
    }
}

/// Removes this channel's cross-channel record. Run by the uninstaller.
///
/// Only this channel's entries go, and the whole key only when nothing of the
/// other channel's is left in it. The record is shared, and the other channel may
/// still be installed: deleting its claim on the original scheme would leave it
/// adopting nothing and re-capturing whatever cursor happens to be applied — the
/// exact failure this module exists to prevent, caused by uninstalling something
/// else.
pub fn forget_this_channel() -> AppResult<()> {
    // Opened, not created — `key_for_write` would create the key on a machine
    // that never had one, purely so the lines below could find it empty and
    // delete it again. Same refusal under `cargo test`, for the same reason.
    if cfg!(test) {
        return Ok(());
    }
    let Ok(key) = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags(channel::CROSS_CHANNEL_KEY, KEY_READ | KEY_WRITE)
    else {
        return Ok(());
    };

    if holder().is_some_and(|held| held.channel == channel::NAME) {
        for value in [OWNER_CHANNEL, OWNER_PID, OWNER_SINCE] {
            let _ = key.delete_value(value);
        }
    }

    if recorded_snapshot().is_some_and(|(owner, _)| owner == channel::NAME) {
        for value in [SNAPSHOT_CHANNEL, SNAPSHOT_PATH, SNAPSHOT_CAPTURED] {
            let _ = key.delete_value(value);
        }
    }

    // An empty key left in HKCU is litter, and `scripts/verify-uninstall.ps1`
    // fails the release if it survives. A key still holding the other channel's
    // record is not litter, so it stays.
    let empty = key.enum_values().next().is_none();
    drop(key);
    if empty {
        let _ = RegKey::predef(HKEY_CURRENT_USER).delete_subkey(channel::CROSS_CHANNEL_KEY);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Both channels must ask Windows for the same name, or each creates its own
    /// mutex, both believe they own the pointer, and the revert loop this module
    /// exists to prevent happens anyway — with a lock in place that says it
    /// cannot.
    #[test]
    fn the_lock_name_does_not_mention_a_channel() {
        assert!(!channel::POINTER_LOCK.contains("(Dev)"));
        assert!(!channel::POINTER_LOCK.ends_with(".dev"));
        assert!(channel::POINTER_LOCK.starts_with(r"Local\"));
    }

    /// Everything that touches the lock, in one test on one thread.
    ///
    /// It reads as three tests and was written as three. It cannot be: `OWNED`
    /// is process-global and `cargo test` runs each test on its own thread, so a
    /// second test claiming between this one's two calls flips the answer
    /// underneath it — `first` false, `second` true, and a failure that appears
    /// on roughly one run in three with nothing in the diff to explain it. That
    /// is not a race worth papering over with a test-only mutex: the state
    /// really is process-global, so the assertions about it belong in one place.
    ///
    /// The lock itself is real, and deliberately so — a named mutex has no seam
    /// worth faking, because the behaviour under test *is* what Windows does
    /// with it. Nothing is recorded: `key_for_write` refuses under `cfg!(test)`.
    #[test]
    fn claiming_the_pointer() {
        // Idempotent within a process. A mutex is re-entrant for its owning
        // thread, so a second wait would succeed again and leave a recursion
        // count nobody releases.
        let first = try_claim();
        let second = try_claim();
        assert_eq!(first, second);

        if first {
            assert!(owns_pointer());
            // A channel holding the pointer has nothing to defer to.
            assert!(deferral_notice().is_none());
        } else {
            // The only way to reach this branch is with the released app
            // running on the same machine, which is the ordinary state of a
            // developer's PC and not a failure.
            assert!(!owns_pointer());
        }
    }

    /// Running the suite must not leave a record behind on the machine that ran
    /// it. Asserted rather than left to review: the failure is silent, lands on
    /// a developer's own PC, and outlives every later run.
    #[test]
    fn a_test_run_cannot_write_the_shared_record() {
        assert!(key_for_write().is_none());
    }

}
