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
    WinHttpSetOption, WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, WINHTTP_FLAG_SECURE,
    WINHTTP_OPTION_REDIRECT_POLICY, WINHTTP_OPTION_REDIRECT_POLICY_ALWAYS,
    WINHTTP_QUERY_FLAG_NUMBER, WINHTTP_QUERY_STATUS_CODE,
};

const API_HOST: &str = "api.github.com";
const RELEASE_PATH: &str = "/repos/notfeylo/cursorforge/releases/latest";
const DOWNLOAD_HOST: &str = "github.com";
pub const RELEASES_URL: &str = "https://github.com/notfeylo/cursorforge/releases";

/// GitHub requires a User-Agent and rejects requests without one.
const AGENT: &str = "Cursed-UpdateCheck";
/// A release payload is a few kilobytes; anything far larger is not our JSON.
const MAX_JSON: usize = 256 * 1024;
/// The installer is ~2 MB. 64 MB is generous headroom and still a hard ceiling.
const MAX_DOWNLOAD: usize = 64 * 1024 * 1024;
const MAX_SUMS: usize = 64 * 1024;

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

        let mut body = Vec::new();
        loop {
            let mut available: u32 = 0;
            if WinHttpQueryDataAvailable(request.0, &mut available).is_err() || available == 0 {
                break;
            }
            let take = (available as usize).min(cap.saturating_sub(body.len()));
            if take == 0 {
                return Err(AppError::msg("the download was larger than expected"));
            }
            let mut chunk = vec![0u8; take];
            let mut read: u32 = 0;
            if WinHttpReadData(request.0, chunk.as_mut_ptr().cast(), take as u32, &mut read).is_err()
                || read == 0
            {
                break;
            }
            chunk.truncate(read as usize);
            body.extend_from_slice(&chunk);
        }
        Ok(body)
    }
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

/// An asset name we are willing to download and run.
///
/// Deliberately strict: the name is used to build a URL and a filename, so it
/// must be a plain NSIS installer and nothing else. A release that starts
/// shipping a `.bat` cannot talk this into running it.
fn is_our_installer(name: &str) -> bool {
    name.starts_with("Cursed_")
        && name.ends_with("_x64-setup.exe")
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

/// Downloads the installer for `tag` and returns where it landed.
pub fn download(tag: &str, asset: &str) -> AppResult<PathBuf> {
    if !is_our_installer(asset) {
        return Err(AppError::invalid("that is not an installer Cursed will run"));
    }
    let tag = sanitise_tag(tag)?;

    let path = format!("/notfeylo/cursorforge/releases/download/v{tag}/{asset}");
    let bytes = get(DOWNLOAD_HOST, &path, MAX_DOWNLOAD, true)?;
    if bytes.len() < 512 * 1024 {
        return Err(AppError::msg("the download looks truncated"));
    }

    // MZ: if this is not a Windows executable, nothing below should touch it.
    if !bytes.starts_with(b"MZ") {
        return Err(AppError::msg("the download is not a Windows installer"));
    }

    let file = download_dir()?.join(asset);
    std::fs::write(&file, &bytes)?;
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
    let path = format!("/notfeylo/cursorforge/releases/download/v{tag}/SHA256SUMS.txt");
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

    let expected = published_hash(tag, asset)?;
    let actual = sha256_file(&file)?;
    if !crate::hash::hex_eq(&actual, &expected) {
        let _ = std::fs::remove_file(&file);
        return Err(AppError::msg(
            "the downloaded installer does not match the checksum published with the release, so it was deleted",
        ));
    }

    crate::shell::open_path(&file)?;
    Ok(())
}

/// Removes anything left in the update staging directory.
pub fn clear_downloads() -> AppResult<()> {
    let dir = download_dir()?;
    if dir.exists() {
        std::fs::remove_dir_all(&dir)?;
    }
    Ok(())
}

/// Runs a check in the background at startup, if the user left it enabled.
/// Failure is silent: a missing network is not an error worth a banner.
pub fn check_in_background() {
    auto_update_in_background();
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
    pub status: Option<UpdateStatus>,
    pub error: Option<String>,
}

fn state_slot() -> &'static std::sync::Mutex<UpdateState> {
    static STATE: std::sync::OnceLock<std::sync::Mutex<UpdateState>> = std::sync::OnceLock::new();
    STATE.get_or_init(|| std::sync::Mutex::new(UpdateState::default()))
}

pub fn state() -> UpdateState {
    state_slot().lock().map(|s| s.clone()).unwrap_or_default()
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
            update_state(|s| {
                s.checking = true;
                s.error = None;
            });

            let found = check();
            update_state(|s| s.checking = false);

            let status = match found {
                Ok(status) => status,
                Err(e) => {
                    // A missing network is not worth shouting about, but it is
                    // recorded so Settings can say something honest.
                    update_state(|s| s.error = Some(e.to_string()));
                    return;
                }
            };
            update_state(|s| s.status = Some(status.clone()));

            let (Some(version), Some(installer)) = (status.latest.clone(), status.installer.clone())
            else {
                return;
            };
            if !status.newer_available {
                return;
            }

            update_state(|s| s.downloading = true);
            let outcome = download(&version, &installer).and_then(|_| {
                // Verified now, not at install time: a download that fails its
                // checksum must never reach a button labelled "install".
                let expected = published_hash(&version, &installer)?;
                let file = download_dir()?.join(&installer);
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
                    Ok(()) => s.ready = true,
                    Err(e) => s.error = Some(e.to_string()),
                }
            });
        })
        .ok();
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
        assert!(is_our_installer("Cursed_1.0.0_x64-setup.exe"));
        assert!(is_our_installer("Cursed_10.2.34_x64-setup.exe"));

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

    #[test]
    fn launching_without_a_downloaded_file_is_refused() {
        assert!(verify_and_launch("1.0.0", "Cursed_9.9.9_x64-setup.exe").is_err());
        assert!(verify_and_launch("1.0.0", "evil.exe").is_err());
    }
}
