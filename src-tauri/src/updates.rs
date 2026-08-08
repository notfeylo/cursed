//! The one network request CursorForge ever makes (PRD §15.2).
//!
//! Implemented on WinHTTP rather than an HTTP crate. That is not austerity for
//! its own sake: WinHTTP uses the OS certificate store and proxy configuration,
//! it is already resident, and it keeps roughly two megabytes of TLS stack out
//! of a binary whose entire budget is twelve.
//!
//! It sends nothing but the request. No identifiers, no telemetry, no body.

use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use windows::core::PCWSTR;
use windows::Win32::Networking::WinHttp::{
    WinHttpCloseHandle, WinHttpConnect, WinHttpOpen, WinHttpOpenRequest, WinHttpQueryDataAvailable,
    WinHttpReadData, WinHttpReceiveResponse, WinHttpSendRequest,
    WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, WINHTTP_FLAG_SECURE,
};

const HOST: &str = "api.github.com";
const PATH: &str = "/repos/notfeylo/cursorforge/releases/latest";
/// GitHub requires a User-Agent and rejects requests without one.
const AGENT: &str = "CursorForge-UpdateCheck";
/// A release payload is a few kilobytes; anything far larger is not our JSON.
const MAX_RESPONSE: usize = 256 * 1024;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatus {
    pub current: String,
    pub latest: Option<String>,
    pub newer_available: bool,
    /// Always the project's releases page — never a URL taken from the response.
    pub url: &'static str,
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
}

/// A handle that closes itself, so no early return can leak a WinHTTP handle.
struct Handle(*mut std::ffi::c_void);

impl Drop for Handle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: the handle was returned by WinHttp* and is closed once.
            unsafe {
                let _ = WinHttpCloseHandle(self.0);
            }
        }
    }
}

impl Handle {
    fn new(raw: *mut std::ffi::c_void, what: &str) -> AppResult<Self> {
        if raw.is_null() {
            Err(AppError::msg(format!("could not reach the update service ({what})")))
        } else {
            Ok(Self(raw))
        }
    }
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

fn fetch_latest_release() -> AppResult<String> {
    let agent = wide(AGENT);
    let host = wide(HOST);
    let path = wide(PATH);
    let verb = wide("GET");
    let headers = wide("User-Agent: CursorForge\r\nAccept: application/vnd.github+json\r\n");

    // SAFETY: every wide buffer outlives the call that borrows it, each handle
    // is owned by a `Handle` that closes it exactly once, and the read loop is
    // bounded by both `available` and MAX_RESPONSE.
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

        let connection = Handle::new(
            WinHttpConnect(session.0, PCWSTR(host.as_ptr()), 443, 0),
            "connection",
        )?;

        let request = Handle::new(
            WinHttpOpenRequest(
                connection.0,
                PCWSTR(verb.as_ptr()),
                PCWSTR(path.as_ptr()),
                PCWSTR::null(),
                PCWSTR::null(),
                std::ptr::null(),
                WINHTTP_FLAG_SECURE,
            ),
            "request",
        )?;

        WinHttpSendRequest(request.0, Some(&headers), None, 0, 0, 0)
            .map_err(|_| AppError::msg("the update check could not be sent"))?;
        WinHttpReceiveResponse(request.0, std::ptr::null_mut())
            .map_err(|_| AppError::msg("the update service did not respond"))?;

        let mut body = Vec::new();
        loop {
            let mut available: u32 = 0;
            if WinHttpQueryDataAvailable(request.0, &mut available).is_err() || available == 0 {
                break;
            }
            let take = (available as usize).min(MAX_RESPONSE - body.len());
            if take == 0 {
                break;
            }
            let mut chunk = vec![0u8; take];
            let mut read: u32 = 0;
            if WinHttpReadData(
                request.0,
                chunk.as_mut_ptr().cast(),
                take as u32,
                &mut read,
            )
            .is_err()
                || read == 0
            {
                break;
            }
            chunk.truncate(read as usize);
            body.extend_from_slice(&chunk);
            if body.len() >= MAX_RESPONSE {
                break;
            }
        }

        String::from_utf8(body)
            .map_err(|_| AppError::msg("the update service returned something unreadable"))
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

pub fn check() -> AppResult<UpdateStatus> {
    let current = env!("CARGO_PKG_VERSION").to_owned();
    let body = fetch_latest_release()?;

    // The response is data. Nothing in it becomes a path, a command, or a URL
    // we open — only a version string we compare (PRD §13.6).
    let release: Release = serde_json::from_str(&body)
        .map_err(|_| AppError::msg("the update service returned an unexpected answer"))?;

    let usable = !release.draft && !release.prerelease;
    let latest = usable.then(|| release.tag_name.trim_start_matches('v').to_owned());
    let newer_available = latest
        .as_deref()
        .is_some_and(|tag| is_newer(tag, &current));

    Ok(UpdateStatus {
        current,
        latest,
        newer_available,
        url: "https://github.com/notfeylo/cursorforge/releases",
    })
}

/// Runs a check in the background at startup, if the user left it enabled.
/// Failure is silent: a missing network is not an error worth a banner.
pub fn check_in_background() {
    std::thread::Builder::new()
        .name("cursorforge-update-check".into())
        .spawn(|| {
            let _ = check();
        })
        .ok();
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn drafts_and_prereleases_are_not_offered() {
        let draft: Release = serde_json::from_str(r#"{"tag_name":"v9.9.9","draft":true}"#).unwrap();
        assert!(draft.draft);
        let pre: Release =
            serde_json::from_str(r#"{"tag_name":"v9.9.9","prerelease":true}"#).unwrap();
        assert!(pre.prerelease);
    }
}
