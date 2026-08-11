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
//! this module is allowed to exist — see [`verify_and_launch`].
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

pub fn verify_and_launch(tag: &str, asset: &str) -> AppResult<()> {
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
    let child = std::process::Command::new(&file)
        .current_dir(file.parent().unwrap_or(&file))
        .spawn()
        .map_err(|e| AppError::msg(format!("the installer would not start: {e}")))?;

    log::info!("update: installer {} started as pid {}", file.display(), child.id());
    Ok(())
}

/// Where a downloaded installer is staged.
pub fn downloaded_path(installer: &str) -> AppResult<PathBuf> {
    Ok(download_dir()?.join(installer))
}

/// Removes anything left in the update staging directory.
pub fn clear_downloads() -> AppResult<()> {
    let dir = download_dir()?;
    if dir.exists() {
        std::fs::remove_dir_all(&dir)?;
    }
    Ok(())
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
        assert!(verify_and_launch("1.0.0", "Cursed_9.9.9_x64-setup.exe").is_err());
        assert!(verify_and_launch("1.0.0", "evil.exe").is_err());
    }
}
