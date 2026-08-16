//! The update path: check, download, verify, install.
//!
//! Implemented on WinHTTP rather than an HTTP crate. That is not austerity for
//! its own sake: WinHTTP uses the OS certificate store and proxy configuration,
//! it is already resident, and it keeps roughly two megabytes of TLS stack out
//! of a binary whose entire budget is twelve.
//!
//! **Nothing downloaded is trusted.** An installer is an executable we are about
//! to run, so before it is launched it must match the SHA-256 published in the
//! release's own `SHA256SUMS.txt`. If the hash is missing or does not match, the
//! file is deleted and the update fails loudly. That check is the whole reason
//! this module is allowed to exist — see [`verified_installer`].
//!
//! The installer is then run with [`INSTALLER_ARGUMENTS`], and those four flags
//! matter as much as the checksum does: without them the Tauri NSIS template
//! takes its fresh-install path, which runs the previous version's uninstaller
//! and destroys the user's data. `docs/UPDATE_PATH_DIAGNOSIS.md` has the trace.
//!
//! The check itself sends nothing but the request: no identifiers, no telemetry.

use crate::error::{AppError, AppResult};
use crate::paths;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use windows::core::PCWSTR;
use windows::Win32::Networking::WinHttp::{
    WinHttpCloseHandle, WinHttpConnect, WinHttpOpen, WinHttpOpenRequest, WinHttpQueryDataAvailable,
    WinHttpQueryHeaders, WinHttpReadData, WinHttpReceiveResponse, WinHttpSendRequest,
    WinHttpSetOption, WinHttpSetTimeouts, WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, WINHTTP_FLAG_SECURE,
    WINHTTP_OPTION_REDIRECT_POLICY, WINHTTP_OPTION_REDIRECT_POLICY_ALWAYS,
    WINHTTP_QUERY_CONTENT_LENGTH, WINHTTP_QUERY_FLAG_NUMBER, WINHTTP_QUERY_STATUS_CODE,
};

const API_HOST: &str = "api.github.com";
const RELEASE_PATH: &str = "/repos/notfeylo/cursed/releases/latest";
const DOWNLOAD_HOST: &str = "github.com";
pub const RELEASES_URL: &str = "https://github.com/notfeylo/cursed/releases";

/// GitHub requires a User-Agent and rejects requests without one.
const AGENT: &str = "Cursed-UpdateCheck";
/// A release payload is a few kilobytes; anything far larger is not our JSON.
const MAX_JSON: usize = 256 * 1024;
/// The installer is ~11 MB. 64 MB is generous headroom and still a hard ceiling.
const MAX_DOWNLOAD: usize = 64 * 1024 * 1024;
const MAX_SUMS: usize = 64 * 1024;

/// How long WinHTTP may spend on each phase before giving up, in milliseconds.
///
/// Set explicitly rather than left at the defaults, because the failure the
/// defaults produce is the worst kind: a request that never returns leaves
/// `downloading` true in the shared state forever, and the panel then shows a
/// disabled button reading DOWNLOADING that no amount of clicking will move.
/// A timeout is an error, and an error is something the UI can recover from.
const RESOLVE_TIMEOUT_MS: i32 = 15_000;
const CONNECT_TIMEOUT_MS: i32 = 20_000;
const SEND_TIMEOUT_MS: i32 = 30_000;
/// Per read, not for the whole download: a slow line is allowed to take as long
/// as it needs, provided it keeps delivering something.
const RECEIVE_TIMEOUT_MS: i32 = 60_000;

/// How many times a network step is attempted before it is called a failure.
///
/// One transient error used to end the update for the whole session — the
/// background pass runs once at startup and nothing retried it — so a dropped
/// connection at the wrong moment meant "updates are broken" until the user
/// happened to restart the app.
const ATTEMPTS: u32 = 3;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatus {
    pub current: String,
    pub latest: Option<String>,
    pub newer_available: bool,
    /// Present only when a newer release ships an installer we can verify.
    pub installer: Option<String>,
    pub size: Option<u64>,
    pub notes: Option<String>,
    /// Always the project's own releases page — never a URL from the response.
    pub url: &'static str,
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    assets: Vec<Asset>,
}

#[derive(Deserialize, Clone)]
struct Asset {
    name: String,
    #[serde(default)]
    size: u64,
}

/// A handle that closes itself, so no early return can leak a WinHTTP handle.
struct Handle(*mut std::ffi::c_void);

impl Drop for Handle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: the handle came from WinHttp* and is closed exactly once.
            unsafe {
                let _ = WinHttpCloseHandle(self.0);
            }
        }
    }
}

impl Handle {
    fn new(raw: *mut std::ffi::c_void, what: &str) -> AppResult<Self> {
        if raw.is_null() {
            Err(AppError::msg(format!("could not reach GitHub ({what})")))
        } else {
            Ok(Self(raw))
        }
    }
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// UTF-16 **without** a NUL terminator.
///
/// `WinHttpSendRequest` takes headers as a slice and uses its length as the
/// header length. Passing a NUL-terminated buffer therefore declares one
/// character too many, WinHTTP parses the NUL as part of a header, and the whole
/// request is rejected with an invalid-parameter error that surfaces as "the
/// request could not be sent". Every other call here wants the terminator; this
/// one must not have it.
fn wide_unterminated(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}

/// One HTTPS GET, returning the body.
///
/// `follow_redirects` exists because release asset URLs redirect to a CDN, while
/// the API does not need to. Redirects stay within HTTPS — WinHTTP will not
/// downgrade to plaintext under `WINHTTP_FLAG_SECURE`.
fn get(host: &str, path: &str, cap: usize, follow_redirects: bool) -> AppResult<Vec<u8>> {
    get_with_progress(host, path, cap, follow_redirects, &mut |_, _| {})
}

/// The same GET, reporting `(received, total)` as the body arrives.
///
/// **A short read is an error here, not a result.** The loop this replaced
/// treated a mid-stream failure as the end of the response: it broke out and
/// returned `Ok` with whatever had arrived. Every caller then did something
/// confidently wrong with a partial file. An installer cut off at 90% still
/// begins with `MZ` and is comfortably over the truncation floor, so it passed
/// both sanity checks and failed only at the checksum — which told the user
/// their download "did not match its published checksum", the wording reserved
/// for a tampered file, and deleted it. On a connection that drops
/// occasionally, that is an update which can never succeed and accuses the
/// project of shipping a bad binary while failing.
///
/// So: read errors propagate, and the body is checked against `Content-Length`
/// before it is returned.
fn get_with_progress(
    host: &str,
    path: &str,
    cap: usize,
    follow_redirects: bool,
    progress: &mut dyn FnMut(u64, u64),
) -> AppResult<Vec<u8>> {
    let agent = wide(AGENT);
    let host_w = wide(host);
    let path_w = wide(path);
    let verb = wide("GET");
    let headers =
        wide_unterminated("User-Agent: Cursed\r\nAccept: application/vnd.github+json");

    // SAFETY: every wide buffer outlives the call that borrows it, each handle
    // is owned by a `Handle` that closes it once, and the read loop is bounded
    // by both the reported availability and `cap`.
    unsafe {
        let session = Handle::new(
            WinHttpOpen(
                PCWSTR(agent.as_ptr()),
                WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
                PCWSTR::null(),
                PCWSTR::null(),
                0,
            ),
            "session",
        )?;

        // Applies to every handle made from this session.
        let _ = WinHttpSetTimeouts(
            session.0,
            RESOLVE_TIMEOUT_MS,
            CONNECT_TIMEOUT_MS,
            SEND_TIMEOUT_MS,
            RECEIVE_TIMEOUT_MS,
        );

        if follow_redirects {
            let policy: u32 = WINHTTP_OPTION_REDIRECT_POLICY_ALWAYS;
            let _ = WinHttpSetOption(
                Some(session.0),
                WINHTTP_OPTION_REDIRECT_POLICY,
                Some(std::slice::from_raw_parts(
                    (&policy as *const u32).cast::<u8>(),
                    std::mem::size_of::<u32>(),
                )),
            );
        }

        let connection = Handle::new(
            WinHttpConnect(session.0, PCWSTR(host_w.as_ptr()), 443, 0),
            "connection",
        )?;

        let request = Handle::new(
            WinHttpOpenRequest(
                connection.0,
                PCWSTR(verb.as_ptr()),
                PCWSTR(path_w.as_ptr()),
                PCWSTR::null(),
                PCWSTR::null(),
                std::ptr::null(),
                WINHTTP_FLAG_SECURE,
            ),
            "request",
        )?;

        WinHttpSendRequest(request.0, Some(&headers), None, 0, 0, 0)
            .map_err(|_| AppError::msg("the request could not be sent"))?;
        WinHttpReceiveResponse(request.0, std::ptr::null_mut())
            .map_err(|_| AppError::msg("GitHub did not respond"))?;

        // A 404 body is still a body; without this an error page would be
        // treated as a download and hashed into a confusing mismatch.
        let mut status: u32 = 0;
        let mut len = std::mem::size_of::<u32>() as u32;
        let _ = WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            PCWSTR::null(),
            Some((&mut status as *mut u32).cast()),
            &mut len,
            std::ptr::null_mut(),
        );
        if status != 0 && !(200..300).contains(&status) {
            return Err(AppError::msg(format!("GitHub replied with status {status}")));
        }

        // How many bytes the server says it is sending. Absent on a chunked
        // response, in which case there is nothing to check the total against
        // and the checksum remains the only guard.
        let mut declared: u32 = 0;
        let mut len = std::mem::size_of::<u32>() as u32;
        let expected = WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_CONTENT_LENGTH | WINHTTP_QUERY_FLAG_NUMBER,
            PCWSTR::null(),
            Some((&mut declared as *mut u32).cast()),
            &mut len,
            std::ptr::null_mut(),
        )
        .is_ok()
        .then_some(u64::from(declared))
        .filter(|n| *n > 0);

        if let Some(total) = expected {
            if total as usize > cap {
                return Err(AppError::msg("the download was larger than expected"));
            }
        }
        progress(0, expected.unwrap_or(0));

        let mut body = Vec::new();
        loop {
            let mut available: u32 = 0;
            WinHttpQueryDataAvailable(request.0, &mut available)
                .map_err(|_| AppError::msg("the connection dropped part-way through"))?;
            if available == 0 {
                break;
            }
            let take = (available as usize).min(cap.saturating_sub(body.len()));
            if take == 0 {
                return Err(AppError::msg("the download was larger than expected"));
            }
            let mut chunk = vec![0u8; take];
            let mut read: u32 = 0;
            WinHttpReadData(request.0, chunk.as_mut_ptr().cast(), take as u32, &mut read)
                .map_err(|_| AppError::msg("the connection dropped part-way through"))?;
            if read == 0 {
                break;
            }
            chunk.truncate(read as usize);
            body.extend_from_slice(&chunk);
            progress(body.len() as u64, expected.unwrap_or(0));
        }

        // The check the old loop could not make, because it could not tell the
        // end of a response from the end of a connection.
        check_complete(body.len(), expected)?;
        Ok(body)
    }
}

/// Whether everything the server said it would send actually arrived.
///
/// Split out so the rule can be tested without a network. A chunked response
/// declares no length, and then there is nothing to compare — the checksum is
/// the only remaining guard, which is why it is not optional anywhere.
fn check_complete(received: usize, expected: Option<u64>) -> AppResult<()> {
    match expected {
        Some(total) if received as u64 != total => Err(AppError::msg(format!(
            "the download stopped early — {received} of {total} bytes arrived"
        ))),
        _ => Ok(()),
    }
}

/// Runs a network step again if it failed in a way that might not fail twice.
///
/// A refusal is not retried: a 404, a name that is not our installer, a body
/// that is not the JSON we expect — none of those improve on a second attempt,
/// and retrying them just makes the eventual error take three times as long.
fn with_retry<T>(what: &str, mut attempt: impl FnMut() -> AppResult<T>) -> AppResult<T> {
    let mut last = None;
    for tries in 1..=ATTEMPTS {
        match attempt() {
            Ok(value) => {
                if tries > 1 {
                    log::info!("update: {what} succeeded on attempt {tries}");
                }
                return Ok(value);
            }
            Err(e) => {
                let transient = matches!(&e, AppError::Message(m) if is_transient(m));
                log::warn!("update: {what} failed on attempt {tries}: {e}");
                if !transient || tries == ATTEMPTS {
                    return Err(e);
                }
                last = Some(e);
                // 1s, then 3s. Long enough for a flapping link to settle,
                // short enough that nobody watching the panel gives up first.
                std::thread::sleep(std::time::Duration::from_millis(
                    1_000 * u64::from(tries) * 2 - 1_000,
                ));
            }
        }
    }
    Err(last.unwrap_or_else(|| AppError::msg(format!("{what} failed"))))
}

/// Whether an error describes a network that misbehaved rather than a server
/// that answered.
fn is_transient(message: &str) -> bool {
    const SIGNS: [&str; 5] = [
        "could not reach GitHub",
        "the request could not be sent",
        "GitHub did not respond",
        "the connection dropped part-way through",
        "the download stopped early",
    ];
    SIGNS.iter().any(|sign| message.contains(sign))
}

/// Compares dotted numeric versions. Anything unparseable sorts as "not newer",
/// so a strange tag can never nag the user into a download.
fn is_newer(latest: &str, current: &str) -> bool {
    let parse = |text: &str| -> Vec<u64> {
        text.trim_start_matches(['v', 'V'])
            .split(['.', '-', '+'])
            .take(3)
            .map(|part| part.parse::<u64>().unwrap_or(0))
            .collect()
    };
    let (latest, current) = (parse(latest), parse(current));
    for index in 0..3 {
        let l = latest.get(index).copied().unwrap_or(0);
        let c = current.get(index).copied().unwrap_or(0);
        if l != c {
            return l > c;
        }
    }
    false
}

/// The installer suffix Tauri gives a bundle built for *this* binary's
/// architecture.
///
/// A release carries one installer per architecture, and the only one that can
/// correctly replace this install is the one matching the binary now running.
/// Matching on the running binary rather than on the machine is deliberate: an
/// x64 build running under emulation on an ARM64 PC should keep updating to x64
/// builds, because that is what is installed there.
///
/// Getting this wrong is quiet. An ARM64 user handed the x64 installer would
/// see the download succeed, the hash verify, the installer run — and end up
/// with a second, emulated copy of the app.
#[cfg(target_arch = "x86_64")]
const INSTALLER_SUFFIX: &str = "_x64-setup.exe";
#[cfg(target_arch = "aarch64")]
const INSTALLER_SUFFIX: &str = "_arm64-setup.exe";
#[cfg(target_arch = "x86")]
const INSTALLER_SUFFIX: &str = "_x86-setup.exe";

/// An asset name we are willing to download and run.
///
/// Deliberately strict: the name is used to build a URL and a filename, so it
/// must be a plain NSIS installer and nothing else. A release that starts
/// shipping a `.bat` cannot talk this into running it.
fn is_our_installer(name: &str) -> bool {
    name.starts_with("Cursed_")
        && name.ends_with(INSTALLER_SUFFIX)
        && !name.contains('/')
        && !name.contains('\\')
        && !name.contains("..")
        && name.chars().all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))
}

pub fn check() -> AppResult<UpdateStatus> {
    let current = env!("CARGO_PKG_VERSION").to_owned();
    let body = get(API_HOST, RELEASE_PATH, MAX_JSON, false)?;
    let text = String::from_utf8(body)
        .map_err(|_| AppError::msg("GitHub returned something unreadable"))?;

    // The response is data. Nothing in it becomes a path or a command — only a
    // version string we compare and an asset name we re-validate (PRD §13.6).
    let release: Release = serde_json::from_str(&text)
        .map_err(|_| AppError::msg("GitHub returned an unexpected answer"))?;

    if release.draft || release.prerelease {
        return Ok(UpdateStatus {
            current,
            latest: None,
            newer_available: false,
            installer: None,
            size: None,
            notes: None,
            url: RELEASES_URL,
        });
    }

    let tag = release.tag_name.trim_start_matches('v').to_owned();
    let newer = is_newer(&tag, &current);
    let asset = release
        .assets
        .iter()
        .find(|a| is_our_installer(&a.name))
        .cloned();

    Ok(UpdateStatus {
        current,
        latest: Some(tag),
        newer_available: newer && asset.is_some(),
        installer: newer.then(|| asset.as_ref().map(|a| a.name.clone())).flatten(),
        size: asset.as_ref().map(|a| a.size),
        // Trimmed hard: release notes are author-controlled text shown in a UI,
        // so they are treated as a short plain-text blurb, never as markup.
        notes: release.body.map(|b| {
            b.chars()
                .filter(|c| !c.is_control() || *c == '\n')
                .take(600)
                .collect()
        }),
        url: RELEASES_URL,
    })
}

fn download_dir() -> AppResult<PathBuf> {
    let dir = paths::root()?.join("updates");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}


/// Downloads an update and checks it, recording progress in the shared state.
///
/// **Both** the background updater and the manual button go through here, and
/// that is the point. They used to be separate: the background path set
/// `downloading` and `ready` on the shared state, the button did not. The UI
/// polls that state every three seconds, so pressing Download produced an
/// Install button that existed for at most three seconds before the next poll
/// read `ready: false` and replaced it with a Download button again. Pressing it
/// worked exactly as designed and looked exactly like nothing happening.
///
/// The checksum is verified here rather than at install time, so a download that
/// fails it never reaches a button labelled "install".
pub fn download_and_verify(version: &str, installer: &str) -> AppResult<()> {
    update_state(|s| {
        s.downloading = true;
        s.ready = false;
        s.error = None;
        s.downloaded = 0;
        s.total = 0;
    });

    let outcome = download(version, installer).and_then(|_| {
        let expected = with_retry("checksum fetch", || published_hash(version, installer))?;
        let file = download_dir()?.join(installer);
        let actual = sha256_file(&file)?;
        if crate::hash::hex_eq(&actual, &expected) {
            Ok(())
        } else {
            let _ = std::fs::remove_file(&file);
            Err(AppError::msg(
                "the downloaded update did not match its published checksum",
            ))
        }
    });

    update_state(|s| {
        s.downloading = false;
        match &outcome {
            Ok(()) => {
                s.ready = true;
                s.error = None;
            }
            Err(e) => {
                s.ready = false;
                s.error = Some(e.to_string());
            }
        }
    });
    match &outcome {
        Ok(()) => log::info!("update: {installer} downloaded and verified, ready to install"),
        Err(e) => log::warn!("update: {installer} could not be prepared: {e}"),
    }
    outcome
}

/// Downloads the installer for `tag` and returns where it landed.
pub fn download(tag: &str, asset: &str) -> AppResult<PathBuf> {
    if !is_our_installer(asset) {
        return Err(AppError::invalid("that is not an installer Cursed will run"));
    }
    let tag = sanitise_tag(tag)?;

    let path = format!("/notfeylo/cursed/releases/download/v{tag}/{asset}");
    log::info!("update: downloading {asset} from v{tag}");

    // Kept so the log can say whether the server declared a length at all. A
    // zero here means the response was chunked and the size check below could
    // not run, which is worth being able to see from a user's log rather than
    // inferring from a download that failed its checksum.
    let declared = std::sync::atomic::AtomicU64::new(0);
    let bytes = with_retry("download", || {
        get_with_progress(DOWNLOAD_HOST, &path, MAX_DOWNLOAD, true, &mut |got, total| {
            declared.store(total, std::sync::atomic::Ordering::Relaxed);
            update_state(|s| {
                s.downloaded = got;
                s.total = total;
            });
        })
    })?;
    log::info!(
        "update: server declared {} bytes, received {}",
        declared.load(std::sync::atomic::Ordering::Relaxed),
        bytes.len()
    );

    if bytes.len() < 512 * 1024 {
        return Err(AppError::msg("the download looks truncated"));
    }

    // MZ: if this is not a Windows executable, nothing below should touch it.
    if !bytes.starts_with(b"MZ") {
        return Err(AppError::msg("the download is not a Windows installer"));
    }

    // Written beside the target and renamed into place, so an interrupted write
    // cannot leave a half-file sitting where the installer is expected to be —
    // which would then be found by `file.exists()`, fail its checksum, and be
    // reported as a corrupted download rather than an unfinished one.
    let dir = download_dir()?;
    let file = dir.join(asset);
    let staging = dir.join(format!("{asset}.part"));
    std::fs::write(&staging, &bytes)?;
    let _ = std::fs::remove_file(&file);
    std::fs::rename(&staging, &file)?;
    log::info!("update: downloaded {} bytes to {}", bytes.len(), file.display());
    Ok(file)
}

/// A release tag is interpolated into a URL, so it may only be a version.
fn sanitise_tag(tag: &str) -> AppResult<String> {
    let tag = tag.trim().trim_start_matches(['v', 'V']);
    if tag.is_empty()
        || tag.len() > 32
        || !tag.chars().all(|c| c.is_ascii_digit() || c == '.')
    {
        return Err(AppError::invalid("that release tag is not a version number"));
    }
    Ok(tag.to_owned())
}

/// SHA-256 of a file, as lowercase hex.
fn sha256_file(path: &Path) -> AppResult<String> {
    let bytes = std::fs::read(path)?;
    Ok(crate::hash::sha256_hex(&bytes))
}

/// Fetches the release's own `SHA256SUMS.txt` and returns the hash for `asset`.
fn published_hash(tag: &str, asset: &str) -> AppResult<String> {
    let tag = sanitise_tag(tag)?;
    let path = format!("/notfeylo/cursed/releases/download/v{tag}/SHA256SUMS.txt");
    let bytes = get(DOWNLOAD_HOST, &path, MAX_SUMS, true)?;
    let text = String::from_utf8(bytes)
        .map_err(|_| AppError::msg("the checksum file is not readable"))?;

    for line in text.lines() {
        let mut parts = line.split_whitespace();
        let (Some(hash), Some(name)) = (parts.next(), parts.next()) else {
            continue;
        };
        if name.trim_start_matches('*') == asset {
            let hash = hash.to_ascii_lowercase();
            if hash.len() == 64 && hash.chars().all(|c| c.is_ascii_hexdigit()) {
                return Ok(hash);
            }
        }
    }
    Err(AppError::msg(
        "this release does not publish a checksum for that installer, so it will not be run",
    ))
}

/// Verifies a downloaded installer against the published checksum and launches
/// it. **This is the security boundary of the whole update feature.**
///
/// An installer is an executable about to run with the user's privileges. TLS
/// proves who served the bytes, not that the bytes are the ones the author
/// published, and a release asset can be replaced. So the file is hashed and
/// compared against `SHA256SUMS.txt` from the same release; a mismatch deletes
/// the file rather than leaving an unverified executable on disk.
/// Checks a downloaded installer against the published checksum and stops.
///
/// The same comparison `verify_and_launch` makes, without launching anything —
/// so the download and verification half of the updater can be proven on its
/// own rather than only by watching an installer appear.
pub fn verify_only(tag: &str, asset: &str) -> AppResult<String> {
    let file = download_dir()?.join(asset);
    if !is_our_installer(asset) || !file.exists() {
        return Err(AppError::invalid("there is no downloaded installer to check"));
    }
    let expected = published_hash(tag, asset)?;
    let actual = sha256_file(&file)?;
    if !crate::hash::hex_eq(&actual, &expected) {
        return Err(AppError::msg(format!(
            "checksum mismatch: got {actual}, the release publishes {expected}"
        )));
    }
    Ok(actual)
}

/// Checks the downloaded installer and returns where it is, without running it.
///
/// Split from the launch so the caller can verify **before** it starts tearing
/// the app down. A checksum failure has to leave the app exactly as it was —
/// hotkeys registered, watchdog defending, tray icon present — rather than
/// half shut down around an installer that is never going to run.
pub fn verified_installer(tag: &str, asset: &str) -> AppResult<PathBuf> {
    let file = download_dir()?.join(asset);
    if !is_our_installer(asset) || !file.exists() {
        return Err(AppError::invalid("there is no downloaded installer to run"));
    }

    let expected = with_retry("checksum fetch", || published_hash(tag, asset))?;
    let actual = sha256_file(&file)?;
    if !crate::hash::hex_eq(&actual, &expected) {
        let _ = std::fs::remove_file(&file);
        return Err(AppError::msg(
            "the downloaded installer does not match the checksum published with the release, so it was deleted",
        ));
    }
    Ok(file)
}

/// Builds the exact command that [`launch`] runs.
///
/// Split out from the spawn so a test can look at what would be run. Asserting
/// the *constant* contains `/UPDATE` proves nothing on its own — the bug this
/// fixes was a correct constant that no call site passed — so what the test
/// reads is this `Command`'s own argument list, one step from
/// `CreateProcess`.
fn installer_command(file: &std::path::Path) -> std::process::Command {
    let mut command = std::process::Command::new(file);
    command
        .args(INSTALLER_ARGUMENTS)
        .current_dir(file.parent().unwrap_or(file));
    command
}

/// Runs an installer that [`verified_installer`] has already vouched for.
pub fn launch(file: &std::path::Path) -> AppResult<()> {
    // `CreateProcess`, not `ShellExecuteW`.
    //
    // The shell path returns as soon as it has *begun* the operation and tells
    // us nothing about whether a process now exists — and the caller closes the
    // app in the next statement. Microsoft's own guidance is that a process
    // terminating right after `ShellExecute` must go through `ShellExecuteEx`
    // with `SEE_MASK_NOASYNC`, because otherwise the pending operation can be
    // dropped with the process that asked for it. Spawning directly sidesteps
    // the question: the child exists before this returns, or we get an error to
    // show instead of quietly exiting with nothing installed.
    let child = installer_command(file)
        .spawn()
        .map_err(|e| AppError::msg(format!("the installer would not start: {e}")))?;

    log::info!(
        "update: installer {} started as pid {} with {}",
        file.display(),
        child.id(),
        INSTALLER_ARGUMENTS.join(" ")
    );
    Ok(())
}

/// What the installer is told when it is run as an update.
///
/// **These four flags are the difference between an update and a reinstall.**
/// Passing none of them — which is what this did until v1.21.0 — is not a
/// cosmetic problem. The Tauri NSIS template decides everything from the command
/// line, so an installer launched bare takes the *fresh install* path: it shows
/// the reinstall page with "uninstall before installing" pre-selected, runs the
/// previous version's uninstaller **without** `/UPDATE`, and that uninstaller
/// then runs `installer-hooks.nsh` in full uninstall mode — restoring the stock
/// Windows pointer scheme and offering to delete the user's presets, custom
/// cursors and original-scheme snapshot with "delete" as the default answer.
///
/// The guard in those hooks is written correctly and keys on `$UpdateMode`. It
/// could never fire, because `$UpdateMode` is set from the command line and
/// nowhere else. See `docs/UPDATE_PATH_DIAGNOSIS.md` for the full trace.
///
/// | Flag | What it does, and where the template does it |
/// |---|---|
/// | `/UPDATE` | Sets `$UpdateMode`. Skips the reinstall page entirely, so the old uninstaller is never run and the hooks never fire. Also preserves the autostart entry and the shortcuts. |
/// | `/P` | Passive: one progress bar, no pages, and — critically — suppresses the "Cursed is running, close it?" prompt, which is otherwise shown for any non-silent, non-passive run. |
/// | `/R` | Relaunches the app afterwards. **Only honoured when `/P` or `/S` is also set**, so it is inseparable from the flag above. |
/// | `/NS` | Do not recreate shortcuts. Redundant while `/UPDATE` is passed, and kept anyway: it is the flag that directly expresses the intent, and it keeps a duplicate desktop icon from appearing if `/UPDATE` is ever dropped. |
///
/// Deliberately **not** `/S`. Fully silent gives the user no sign that anything is
/// happening during an install they explicitly asked for, and the brief asks for
/// exactly one progress bar rather than none.
///
/// No `/ARGS` either. It would restart the app with extra arguments, and the only
/// one worth passing is `--silent`, which sends it to the tray. An update is
/// always started by someone clicking a button in the window, so the window is
/// what they should get back.
const INSTALLER_ARGUMENTS: &[&str] = &["/UPDATE", "/P", "/R", "/NS"];

/// Where a downloaded installer is staged.
pub fn downloaded_path(installer: &str) -> AppResult<PathBuf> {
    Ok(download_dir()?.join(installer))
}

// ── the pending-install record ───────────────────────────────────
//
// An update replaces the running binary and hands control to an installer that
// may or may not finish. Nothing about that is observable from inside a process
// that is about to exit, so what happened has to be worked out on the *next*
// launch, from a note left behind before the handover.

/// Written before the installer starts, read on the next launch, then deleted.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct PendingInstall {
    /// The version that was running when the update was started.
    from: String,
    /// The version the installer was supposed to produce.
    to: String,
    started_at: String,
    /// A copy of the binary that was running, kept until the update is known to
    /// have worked.
    rollback: Option<PathBuf>,
}

fn pending_file() -> AppResult<PathBuf> {
    Ok(download_dir()?.join("pending.json"))
}

/// How an update attempt turned out, worked out on the launch after it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", tag = "outcome")]
pub enum InstallOutcome {
    /// The version now running is the one the installer was asked to produce.
    Succeeded { from: String, to: String },
    /// An update was started and the version did not change. The installer was
    /// cancelled, aborted, or refused — and the user is owed a plain answer
    /// rather than an app that silently looks exactly as it did before.
    DidNotTake { from: String, to: String },
}

/// Records that an update is about to be attempted, and keeps the current
/// binary.
///
/// The copy is what makes a rollback possible at all: once the installer runs,
/// the previous executable is gone from the install directory and there is
/// nowhere to get it back from short of downloading it again. It is deleted as
/// soon as the new version is confirmed working, so it costs ~12 MB and only
/// between two launches.
pub fn record_pending_install(to: &str) -> AppResult<()> {
    let from = env!("CARGO_PKG_VERSION").to_owned();
    let rollback = match std::env::current_exe() {
        Ok(exe) => {
            let dir = download_dir()?.join("rollback");
            std::fs::create_dir_all(&dir)?;
            let kept = dir.join(format!("Cursed-{from}.exe"));
            match std::fs::copy(&exe, &kept) {
                Ok(_) => Some(kept),
                Err(e) => {
                    // Not fatal. An update without a rollback copy is still an
                    // update; refusing to install because a spare could not be
                    // written would be the worse failure.
                    log::warn!("update: could not keep a copy of the current binary: {e}");
                    None
                }
            }
        }
        Err(e) => {
            log::warn!("update: could not locate the running binary to keep: {e}");
            None
        }
    };

    let record = PendingInstall {
        from,
        to: to.trim_start_matches('v').to_owned(),
        started_at: crate::util::iso_now(),
        rollback,
    };
    // Through the shared store: write, flush, sync_all, rename.
    //
    // This record is written in the last second before the machine is handed to
    // an installer that replaces the binary — which is the single most likely
    // moment in this app's life for the power to go, the machine to be closed,
    // or the process to be killed. A rename that lands before its bytes do
    // leaves a `pending.json` that exists and is empty, and an empty one is
    // indistinguishable from no update having been started at all: the next
    // launch reports nothing, the rollback copy is never cleaned up, and a
    // failed update is silent.
    crate::state::store::write(&pending_file()?, &serde_json::to_string_pretty(&record)?)?;
    log::info!("update: {} -> {} recorded before handover", record.from, record.to);
    Ok(())
}

/// Works out how the last update attempt went, and clears the record.
///
/// Returns `None` when no update was pending, which is every ordinary launch.
///
/// **What this cannot detect:** a new version that installs and then fails to
/// start. Nothing runs to notice, because the thing that would notice is the
/// binary that will not run. Catching that needs a process outside this one —
/// a service or a launcher — which is a larger decision than the update path,
/// and until it is made the rollback copy is a manual recovery rather than an
/// automatic one. Recorded here so the limit is not mistaken for coverage.
pub fn settle_pending_install() -> Option<InstallOutcome> {
    let file = pending_file().ok()?;
    // Through the store, so a record damaged by the crash it was written to
    // survive is recovered from its backup rather than read as "no update was
    // ever started".
    let (record, _) = crate::state::store::read::<Option<PendingInstall>>(&file);
    let record = record?;
    let _ = std::fs::remove_file(&file);
    let mut backup = file.clone().into_os_string();
    backup.push(".bak");
    let _ = std::fs::remove_file(std::path::PathBuf::from(backup));

    let running = env!("CARGO_PKG_VERSION");
    let outcome = if running == record.to {
        // The rollback copy has done its job and is now 12 MB of a version
        // nobody is going to run.
        if let Some(kept) = &record.rollback {
            let _ = std::fs::remove_file(kept);
        }
        log::info!("update: {} -> {} succeeded", record.from, record.to);
        InstallOutcome::Succeeded {
            from: record.from,
            to: record.to,
        }
    } else {
        // Deliberately keeping the rollback copy here. The version did not
        // change, so whatever went wrong may still be going wrong, and the one
        // copy of the previous binary is not worth discarding on a guess.
        log::warn!(
            "update: {} -> {} did not take; still running {running}",
            record.from,
            record.to
        );
        InstallOutcome::DidNotTake {
            from: record.from,
            to: record.to,
        }
    };
    Some(outcome)
}

/// Settles the last update attempt and publishes the answer for the UI.
///
/// Called once from startup. Kept separate from [`settle_pending_install`] so
/// the decision and the reporting of it can be tested apart.
pub fn settle_and_report() {
    if let Some(outcome) = settle_pending_install() {
        update_state(|s| s.installed = Some(outcome.clone()));
    }
}

/// Removes downloaded installers from the update staging directory.
///
/// **Not everything in it.** This used to be `remove_dir_all` on the whole
/// directory, which also took `pending.json` and the `rollback\` copy of the
/// previous binary — so a user who pressed "clear downloads" between starting an
/// update and the next launch destroyed the record of what was being installed
/// *and* the only local copy of the version to go back to. The button is
/// offered as a way to reclaim a few megabytes; it should not be able to end a
/// recovery.
///
/// What it removes is what it says: installers. They are re-downloadable by
/// definition — that is what makes them the disposable thing here.
pub fn clear_downloads() -> AppResult<()> {
    let dir = download_dir()?;
    if !dir.exists() {
        return Ok(());
    }

    for item in std::fs::read_dir(&dir)?.flatten() {
        let path = item.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default();
        if is_disposable(path.is_dir(), name) {
            let _ = std::fs::remove_file(&path);
        }
    }
    Ok(())
}

/// Whether one entry in the staging directory may be deleted to reclaim space.
///
/// Its own function so it can be tested: the alternative is a test that runs
/// `clear_downloads` against the developer's real `%APPDATA%\Cursed\updates`,
/// which is the same class of mistake as testing the update path against the
/// machine holding the data it might destroy.
fn is_disposable(is_dir: bool, name: &str) -> bool {
    // `rollback\` is the only directory here, and the binary in it is the one
    // thing in this tree that cannot be fetched again.
    !is_dir && is_our_installer(name)
}

/// How long the background pass waits before looking again.
///
/// The check used to happen once, at startup, and never again. An app that
/// lives in the tray for a fortnight therefore never saw a release published
/// the day after it launched, and — worse — a single failed attempt at startup,
/// on a laptop opened before its Wi-Fi had associated, meant no update for the
/// whole session. Both are the same bug: nothing ever tried a second time.
const RECHECK_INTERVAL: std::time::Duration = std::time::Duration::from_secs(6 * 60 * 60);
/// The first retry comes quickly, because the usual reason a startup check
/// fails is that the network was not up yet.
const RETRY_AFTER_FAILURE: std::time::Duration = std::time::Duration::from_secs(5 * 60);

/// Runs a check in the background at startup, if the user left it enabled, and
/// keeps looking for as long as the app is running.
/// Failure is silent: a missing network is not an error worth a banner.
pub fn check_in_background() {
    static STARTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if STARTED.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return;
    }

    std::thread::Builder::new()
        .name("cursed-update-poll".into())
        .spawn(|| loop {
            let succeeded = run_update_pass();
            std::thread::sleep(if succeeded {
                RECHECK_INTERVAL
            } else {
                RETRY_AFTER_FAILURE
            });
        })
        .ok();
}

/// What the background updater has found so far, for the UI to read.
///
/// Held in memory rather than pushed at the frontend: the window is usually
/// hidden, and an event fired at a webview that does not exist yet is lost.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateState {
    pub checking: bool,
    pub downloading: bool,
    /// Downloaded and verified against the release checksum — ready to run.
    pub ready: bool,
    /// Bytes received so far, and how many are expected. Both zero when there
    /// is nothing in flight.
    ///
    /// An eleven-megabyte download used to report nothing but `downloading:
    /// true`, so a slow connection showed a one-pixel shimmer and no other sign
    /// of life for minutes. There is no way to tell that from a hang, and
    /// people reasonably concluded the button had not worked.
    pub downloaded: u64,
    pub total: u64,
    pub status: Option<UpdateStatus>,
    pub error: Option<String>,
    /// How the last update attempt turned out, worked out on this launch.
    ///
    /// `None` on every ordinary start. Set once, at startup, and left alone —
    /// an update that succeeded is worth saying once, and an update that did
    /// not take is worth saying plainly rather than leaving the user to notice
    /// the version never changed.
    pub installed: Option<InstallOutcome>,
}

fn state_slot() -> &'static std::sync::Mutex<UpdateState> {
    static STATE: std::sync::OnceLock<std::sync::Mutex<UpdateState>> = std::sync::OnceLock::new();
    STATE.get_or_init(|| std::sync::Mutex::new(UpdateState::default()))
}

/// The state the UI reads, with `ready` checked against the disk rather than
/// trusted.
///
/// `ready` means "there is a verified installer waiting". It was set once and
/// never revisited, so anything that removed the staged file — Disk Cleanup, an
/// antivirus quarantine, the user emptying the folder — left the panel offering
/// INSTALL & RESTART for a file that was gone. Pressing it produced "there is
/// no downloaded installer to run" and no way forward. Now a missing file puts
/// the panel back to offering the download.
pub fn state() -> UpdateState {
    let mut current = state_slot().lock().map(|s| s.clone()).unwrap_or_default();
    if current.ready {
        let staged = current
            .status
            .as_ref()
            .and_then(|s| s.installer.as_ref())
            .and_then(|name| downloaded_path(name).ok());
        if !staged.is_some_and(|path| path.exists()) {
            log::warn!("update: the staged installer is gone; offering the download again");
            current.ready = false;
            update_state(|s| s.ready = false);
        }
    }
    current
}

fn update_state(f: impl FnOnce(&mut UpdateState)) {
    if let Ok(mut guard) = state_slot().lock() {
        f(&mut guard);
    }
}

/// Checks, and if a newer version exists, downloads and verifies it — unasked.
///
/// Installing stays the user's decision, because it closes the app and runs an
/// installer. Everything *before* that decision is work they would otherwise sit
/// and wait through, so it happens up front and the button already says
/// "install" by the time anyone looks at it.
pub fn auto_update_in_background() {
    std::thread::Builder::new()
        .name("cursed-auto-update".into())
        .spawn(|| {
            run_update_pass();
        })
        .ok();
}

/// One check, and the download that follows if there is something to fetch.
///
/// Returns whether the pass got as far as it needed to, so the caller can
/// decide how soon to try again. "Up to date" counts as success; a network that
/// would not answer does not.
fn run_update_pass() -> bool {
    // The timer thread and the Check-now button both land here, and two passes
    // downloading the same installer at once is a waste at best.
    static IN_FLIGHT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if IN_FLIGHT.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return true;
    }
    struct Done;
    impl Drop for Done {
        fn drop(&mut self) {
            IN_FLIGHT.store(false, std::sync::atomic::Ordering::SeqCst);
        }
    }
    let _done = Done;

    update_state(|s| {
        s.checking = true;
        s.error = None;
    });

    let found = with_retry("check", check);
    update_state(|s| s.checking = false);

    let status = match found {
        Ok(status) => status,
        Err(e) => {
            // A missing network is not worth shouting about, but it is
            // recorded so Settings can say something honest — and logged, so
            // that "updates don't work" is answerable from a log file instead
            // of guessed at. The updater used to write nothing at all.
            log::warn!("update: check failed: {e}");
            update_state(|s| s.error = Some(e.to_string()));
            return false;
        }
    };
    log::info!(
        "update: running {}, latest {}, newer={}",
        status.current,
        status.latest.as_deref().unwrap_or("(none)"),
        status.newer_available
    );
    update_state(|s| s.status = Some(status.clone()));

    if !status.newer_available {
        return true;
    }
    let (Some(version), Some(installer)) = (status.latest.clone(), status.installer.clone()) else {
        return true;
    };

    // Already fetched and verified on an earlier pass — do not spend the
    // bandwidth again just because six hours went by.
    if state().ready {
        return true;
    }

    download_and_verify(&version, &installer).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The three files that carry a version must agree.
    ///
    /// `Cargo.toml` is what the updater reports as the running version;
    /// `tauri.conf.json` is what the installer stamps onto the `.exe`. When they
    /// drift the app compares a version it is not actually running against the
    /// newest release, so it either offers an update already installed or stays
    /// quiet about one that is not.
    #[test]
    fn every_file_that_carries_a_version_agrees() {
        let cargo = env!("CARGO_PKG_VERSION");

        let tauri: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).expect("tauri.conf.json");
        let package: serde_json::Value =
            serde_json::from_str(include_str!("../../package.json")).expect("package.json");

        assert_eq!(
            tauri["version"].as_str(),
            Some(cargo),
            "tauri.conf.json disagrees with Cargo.toml"
        );
        assert_eq!(
            package["version"].as_str(),
            Some(cargo),
            "package.json disagrees with Cargo.toml"
        );
    }

    /// The release asset is matched by name, and the uninstaller hook calls the
    /// binary by name, so both have to stay what the code expects.
    #[test]
    fn the_bundle_is_named_what_the_rest_of_the_code_expects() {
        let tauri: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).expect("tauri.conf.json");
        assert_eq!(tauri["productName"].as_str(), Some("Cursed"));
        assert_eq!(tauri["mainBinaryName"].as_str(), Some("Cursed"));
        // Per-user, so an update never needs an administrator prompt.
        assert_eq!(
            tauri["bundle"]["windows"]["nsis"]["installMode"].as_str(),
            Some("currentUser")
        );
    }

    #[test]
    fn version_comparison_handles_the_usual_shapes() {
        assert!(is_newer("1.1.0", "1.0.0"));
        assert!(is_newer("v1.0.1", "1.0.0"));
        assert!(is_newer("2.0.0", "1.9.9"));
        assert!(!is_newer("1.0.0", "1.0.0"));
        assert!(!is_newer("1.0.0", "1.0.1"));
        assert!(!is_newer("0.9.0", "1.0.0"));
    }

    #[test]
    fn an_unparseable_tag_never_claims_to_be_newer() {
        assert!(!is_newer("nightly", "1.0.0"));
        assert!(!is_newer("", "1.0.0"));
        assert!(!is_newer("../../etc/passwd", "1.0.0"));
    }

    #[test]
    fn only_our_own_installer_name_is_accepted() {
        assert!(is_our_installer(&format!("Cursed_1.0.0{INSTALLER_SUFFIX}")));
        assert!(is_our_installer(&format!("Cursed_10.2.34{INSTALLER_SUFFIX}")));

        // Anything that is not exactly our installer must be refused, because
        // the name becomes both a URL path segment and a file we execute.
        for bad in [
            "Cursed_1.0.0_x64-setup.bat",
            "evil.exe",
            "../../../windows/system32/cmd.exe",
            "Cursed_1.0.0_x64-setup.exe.bat",
            "Cursed_/../setup.exe",
            // A space is not in the allowed character set, so this is not our
            // installer even though it reads like one.
            "Cursed_1.0.0_x64-setup exe",
            // The previous product's installer is not this product's installer.
            "CursorForge_1.0.0_x64-setup.exe",
            "",
        ] {
            assert!(!is_our_installer(bad), "should refuse {bad:?}");
        }
    }

    /// A release carries one installer per architecture and exactly one of them
    /// can replace this install.
    ///
    /// The failure this guards is silent rather than loud: a wrong-architecture
    /// installer downloads, verifies against its published hash and runs — it is
    /// a genuine, correctly-signed Cursed installer. It just installs the wrong
    /// build, and the first sign of it is an ARM64 machine quietly running an
    /// emulated app.
    #[test]
    fn an_installer_built_for_another_architecture_is_not_ours() {
        for suffix in ["_x64-setup.exe", "_arm64-setup.exe", "_x86-setup.exe"] {
            let name = format!("Cursed_1.0.0{suffix}");
            assert_eq!(
                is_our_installer(&name),
                suffix == INSTALLER_SUFFIX,
                "{name} on a {} build",
                std::env::consts::ARCH
            );
        }
    }

    #[test]
    fn a_release_tag_may_only_be_a_version() {
        assert_eq!(sanitise_tag("v1.2.3").unwrap(), "1.2.3");
        assert_eq!(sanitise_tag("1.2.3").unwrap(), "1.2.3");

        for bad in [
            "1.2.3/../../evil",
            "latest",
            "1.2.3?x=1",
            "../../../",
            "",
            "1.2.3 && calc",
        ] {
            assert!(sanitise_tag(bad).is_err(), "should refuse {bad:?}");
        }
    }

    #[test]
    fn drafts_and_prereleases_are_not_offered() {
        let draft: Release = serde_json::from_str(r#"{"tag_name":"v9.9.9","draft":true}"#).unwrap();
        assert!(draft.draft);
        let pre: Release =
            serde_json::from_str(r#"{"tag_name":"v9.9.9","prerelease":true}"#).unwrap();
        assert!(pre.prerelease);
    }

    /// The bug this release is mostly about: a body that stops short is a
    /// failure, and the message says so in bytes rather than accusing the file
    /// of failing its checksum.
    #[test]
    fn a_body_that_stops_short_of_its_declared_length_is_a_failure() {
        assert!(check_complete(11_586_368, Some(11_586_368)).is_ok());

        let cut = check_complete(4_194_304, Some(11_586_368)).unwrap_err().to_string();
        assert!(cut.contains("stopped early"), "{cut}");
        assert!(cut.contains("4194304"), "says how much arrived: {cut}");
        assert!(cut.contains("11586368"), "says how much was expected: {cut}");
        // It must read as transient, or it will not be retried and the fix is
        // only half of one.
        assert!(is_transient(&cut));

        // More than declared is equally wrong.
        assert!(check_complete(11_586_369, Some(11_586_368)).is_err());

        // Chunked: no declared length, nothing to compare, checksum decides.
        assert!(check_complete(4_194_304, None).is_ok());
    }

    /// A network that wobbled is worth another go; a server that answered is
    /// not. Getting this backwards is expensive in both directions — retrying a
    /// 404 triples the time before the user is told, and *not* retrying a
    /// dropped connection is the bug this whole path was rewritten for.
    #[test]
    fn only_a_misbehaving_network_is_worth_retrying() {
        for transient in [
            "could not reach GitHub (session)",
            "the request could not be sent",
            "GitHub did not respond",
            "the connection dropped part-way through",
            "the download stopped early — 4194304 of 11586368 bytes arrived",
        ] {
            assert!(is_transient(transient), "should retry {transient:?}");
        }

        for final_ in [
            "GitHub replied with status 404",
            "the download is not a Windows installer",
            "the downloaded update did not match its published checksum",
            "the download was larger than expected",
            "GitHub returned an unexpected answer",
        ] {
            assert!(!is_transient(final_), "should not retry {final_:?}");
        }
    }

    #[test]
    fn a_transient_failure_is_retried_and_a_refusal_is_not() {
        let tries = std::cell::Cell::new(0);
        let recovered = with_retry("test", || {
            tries.set(tries.get() + 1);
            if tries.get() < 3 {
                Err(AppError::msg("the connection dropped part-way through"))
            } else {
                Ok(7)
            }
        });
        assert_eq!(recovered.unwrap(), 7);
        assert_eq!(tries.get(), 3, "gave up too early");

        // Never more than ATTEMPTS, however transient the failure looks.
        let forever = std::cell::Cell::new(0);
        let gave_up = with_retry("test", || {
            forever.set(forever.get() + 1);
            Err::<(), _>(AppError::msg("GitHub did not respond"))
        });
        assert!(gave_up.is_err());
        assert_eq!(forever.get(), ATTEMPTS);

        // A refusal costs exactly one attempt.
        let once = std::cell::Cell::new(0);
        let refused = with_retry("test", || {
            once.set(once.get() + 1);
            Err::<(), _>(AppError::msg("GitHub replied with status 404"))
        });
        assert!(refused.is_err());
        assert_eq!(once.get(), 1, "a 404 does not improve on a second attempt");
    }

    #[test]
    fn launching_without_a_downloaded_file_is_refused() {
        assert!(verified_installer("1.0.0", "Cursed_9.9.9_x64-setup.exe").is_err());
        assert!(verified_installer("1.0.0", "evil.exe").is_err());
    }

    /// The four flags are the whole difference between an update and a
    /// reinstall, and three of them have a specific job that nothing else does.
    ///
    /// Passing none of them — which is what shipped through v1.20.0 — made the
    /// installer take the fresh-install path: it ran the previous version's
    /// uninstaller, which ran `installer-hooks.nsh` in full uninstall mode,
    /// which restored the stock Windows pointer scheme and offered to delete
    /// the user's presets with "delete" as the default. `docs/
    /// UPDATE_PATH_DIAGNOSIS.md` traces it line by line.
    #[test]
    fn the_installer_is_told_that_this_is_an_update() {
        // Without this the old uninstaller runs and the hooks fire.
        assert!(
            INSTALLER_ARGUMENTS.contains(&"/UPDATE"),
            "an update that does not say it is an update is an uninstall"
        );
        // Without this the user is asked to close a running app mid-update.
        assert!(INSTALLER_ARGUMENTS.contains(&"/P"));
        // Without this the app never comes back.
        assert!(INSTALLER_ARGUMENTS.contains(&"/R"));
        assert!(INSTALLER_ARGUMENTS.contains(&"/NS"));

        // `/R` is only honoured in silent or passive mode, so it is meaningless
        // on its own — the template checks `$PassiveMode = 1 ${OrIf} ${Silent}`
        // before even looking for it.
        assert!(
            !INSTALLER_ARGUMENTS.contains(&"/R") || INSTALLER_ARGUMENTS.contains(&"/P"),
            "/R does nothing without /P"
        );

        // Not silent. An install the user asked for should show them something.
        assert!(!INSTALLER_ARGUMENTS.contains(&"/S"));
    }

    /// The constant being right is not the property that was broken.
    ///
    /// Through v1.20.0 there was no constant at all, but there could have been:
    /// a correct list of flags sitting beside a `spawn()` that passed none of
    /// them would have looked exactly as fixed as this does, and destroyed data
    /// exactly as thoroughly. So the assertion is on the command that is one
    /// call from `CreateProcess`, not on the list it was built from.
    #[test]
    fn the_flags_reach_the_command_that_is_actually_run() {
        let installer = std::path::Path::new(r"C:\x\updates\Cursed_1.21.0_x64-setup.exe");
        let command = installer_command(installer);

        let passed: Vec<String> = command
            .get_args()
            .map(|a| a.to_string_lossy().to_string())
            .collect();
        assert_eq!(passed, INSTALLER_ARGUMENTS, "the spawn must carry the flags");

        // And it runs the file it was handed, from beside it — a relative
        // working directory is how an installer ends up looking for its own
        // payload in the wrong place.
        assert_eq!(command.get_program(), installer.as_os_str());
        assert_eq!(
            command.get_current_dir(),
            Some(std::path::Path::new(r"C:\x\updates"))
        );
    }

    /// Clearing downloads must not be able to end a recovery.
    ///
    /// It used to `remove_dir_all` the staging directory, which held three
    /// different things: installers, which are re-downloadable; `pending.json`,
    /// which is the only record that an update was started; and `rollback\`,
    /// which is the only local copy of the version to go back to. Pressing a
    /// button labelled "clear downloads" took all three.
    #[test]
    fn clearing_downloads_keeps_the_things_that_cannot_be_downloaded_again() {
        let installer = format!("Cursed_1.20.0{INSTALLER_SUFFIX}");
        assert!(is_disposable(false, &installer), "an installer is disposable");

        assert!(!is_disposable(true, "rollback"), "the previous binary stays");
        assert!(!is_disposable(false, "pending.json"), "the record stays");
        assert!(!is_disposable(false, "pending.json.bak"));
        // And nothing that is not ours, whatever it is doing there.
        assert!(!is_disposable(false, "notes.txt"));
        assert!(!is_disposable(false, "Cursed-1.20.0.exe"));
    }

    /// Only NSIS ships, and the config has to agree.
    ///
    /// The updater cannot choose an MSI — `is_our_installer` accepts one shape
    /// of name and it is the NSIS one — so an MSI built beside it is not a
    /// second update path. It is a second *installer*, one hand-download away
    /// from a user who now has two copies of Cursed in two directories with two
    /// uninstall entries, because NSIS and the MSI record the install location
    /// under different registry values and neither looks for the other's.
    #[test]
    fn the_bundle_ships_nsis_only() {
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).expect("tauri.conf.json");
        let targets = config["bundle"]["targets"]
            .as_array()
            .expect("bundle.targets");
        assert_eq!(
            targets.iter().filter_map(|t| t.as_str()).collect::<Vec<_>>(),
            vec!["nsis"],
            "anything but nsis alone reintroduces the duplicate-install failure"
        );
    }

    /// Per-user, so an update never needs elevation.
    ///
    /// `perMachine` or `both` make the installer request admin, and an updater
    /// that cannot elevate silently fails with OS error 740 — the app quits and
    /// the update simply does not happen.
    #[test]
    fn the_installer_never_needs_elevation() {
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).expect("tauri.conf.json");
        assert_eq!(
            config["bundle"]["windows"]["nsis"]["installMode"].as_str(),
            Some("currentUser")
        );
    }

    /// The guard that makes `/UPDATE` matter has to still be in the hooks. If
    /// somebody removes it, the flag above is protecting nothing.
    #[test]
    fn the_uninstall_hooks_still_check_update_mode() {
        let hooks = include_str!("../installer-hooks.nsh");
        // Both hooks run during an upgrade, and both would otherwise destroy
        // something: the cursor scheme, and everything in %APPDATA%\Cursed.
        let guards = hooks.matches("$UpdateMode = 1").count();
        assert!(
            guards >= 2,
            "PREUNINSTALL and POSTUNINSTALL must each refuse to run during an update; found {guards}"
        );
        assert!(
            hooks.contains(r#"RMDir /r "$APPDATA\Cursed""#),
            "this test is guarding the deletion below; if it moved, re-point the test"
        );
    }

    /// Extracts one `!macro NAME ... !macroend` block, as lines.
    fn hook_body<'a>(source: &'a str, name: &str) -> Vec<&'a str> {
        let opener = format!("!macro {name}");
        let mut lines = source.lines().skip_while(|l| !l.trim_start().starts_with(&opener));
        // Drop the `!macro` line itself so index 0 is the first statement.
        lines.next();
        lines
            .take_while(|l| !l.trim_start().starts_with("!macroend"))
            .collect()
    }

    /// Anything in an uninstall hook that a user would notice happening to them.
    ///
    /// `nsExec` is on the list because the thing it executes is
    /// `--restore-defaults`, which puts the machine back on the stock Windows
    /// arrow — the most visible of the lot, and the one least likely to be read
    /// as destructive from the line itself.
    fn destructive(line: &str) -> bool {
        let line = line.trim_start();
        if line.starts_with(';') {
            return false;
        }
        ["RMDir", "Delete", "DeleteRegValue", "MessageBox", "nsExec::"]
            .iter()
            .any(|marker| line.starts_with(marker))
    }

    /// **The regression test for the data-loss bug.**
    ///
    /// Not "is there a guard" — there was a guard, it was correct, and it ran on
    /// every update for three releases without firing, because the flag it keys
    /// on was decided a process and a half away in a `spawn()` call with no
    /// arguments. What has to hold is *reachability*: with `$UpdateMode = 1`,
    /// control must leave each hook before it reaches anything destructive, and
    /// come back after all of it.
    ///
    /// So the guard is traced. Find where it jumps to, find where that label
    /// sits, and assert that every destructive statement in the macro lies
    /// strictly between the two — i.e. that the jump goes over all of them and
    /// lands past the last.
    #[test]
    fn an_update_cannot_reach_anything_the_uninstaller_destroys() {
        let hooks = include_str!("../installer-hooks.nsh");

        for name in ["NSIS_HOOK_PREUNINSTALL", "NSIS_HOOK_POSTUNINSTALL"] {
            let body = hook_body(hooks, name);
            assert!(!body.is_empty(), "{name} not found in installer-hooks.nsh");

            let guard = body
                .iter()
                .position(|l| l.contains("$UpdateMode = 1"))
                .unwrap_or_else(|| panic!("{name} has no $UpdateMode guard at all"));

            // The `Goto` inside the guarded block is what actually skips the
            // work. A guard whose body only prints a line changes nothing.
            let (jump_offset, target) = body[guard..]
                .iter()
                .take_while(|l| !l.trim_start().starts_with("${EndIf}"))
                .enumerate()
                .find_map(|(offset, line)| {
                    line.trim_start()
                        .strip_prefix("Goto ")
                        .map(|label| (offset, label.trim().to_owned()))
                })
                .unwrap_or_else(|| panic!("{name}'s update guard does not skip anything"));
            let jump = guard + jump_offset;

            let landing = body
                .iter()
                .position(|l| l.trim_start() == format!("{target}:"))
                .unwrap_or_else(|| panic!("{name} jumps to {target}, which does not exist"));

            assert!(
                landing > jump,
                "{name}'s guard jumps backwards, which is a loop rather than a skip"
            );

            let mut checked = 0;
            for (index, line) in body.iter().enumerate() {
                if !destructive(line) {
                    continue;
                }
                checked += 1;
                assert!(
                    index > guard,
                    "{name}: `{}` runs before the update guard is even read",
                    line.trim()
                );
                assert!(
                    index < landing,
                    "{name}: `{}` sits past `{target}:`, so an update reaches it",
                    line.trim()
                );
            }

            assert!(
                checked > 0,
                "{name} has nothing destructive in it; this test is asserting nothing. \
                 Either the hook was emptied or `destructive` stopped recognising its lines."
            );
        }
    }

    /// The order in `install_update`, asserted against the source.
    ///
    /// It cannot be asserted any other way — the function takes an `AppHandle`,
    /// tears down a live Tauri app and launches an installer, none of which a
    /// test may do. But the order is the thing that was wrong, and it is worth
    /// pinning: verify the download **before** anything is torn down, record the
    /// pending install **before** the process that would write it is gone, and
    /// only launch once both have happened.
    ///
    /// The old order launched the installer first and then began shutting down,
    /// on a one-second timer, which is a race dressed as a sequence.
    #[test]
    fn nothing_is_torn_down_before_the_installer_is_verified() {
        let source = include_str!("commands.rs");
        let body = source
            .split("pub fn install_update")
            .nth(1)
            .expect("install_update")
            .split("\n#[tauri::command]")
            .next()
            .expect("the body of install_update");

        let at = |needle: &str| {
            body.find(needle)
                .unwrap_or_else(|| panic!("install_update no longer calls {needle}"))
        };

        let verify = at("verified_installer");
        let record = at("record_pending_install");
        let teardown = at("prepare_for_shutdown");
        let launch = at("updates::launch");
        let exit = at("app.exit(0)");

        assert!(verify < teardown, "a checksum failure must leave the app intact");
        assert!(record < teardown, "the record is written by the process being replaced");
        assert!(teardown < launch, "the installer must not race a live app");
        assert!(launch < exit, "exiting first would abandon the update");
    }
}
