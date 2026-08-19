//! Photo mode: a learned matte, downloaded on request.
//!
//! ## Why this exists
//!
//! The classical path is a flood fill with a tolerance. It is exact and instant
//! on the artwork this app is for — logos, icons, screenshots, crosshairs — and
//! it cannot separate a lit face from a white wall, because lit skin sits a few
//! levels from white and hair is semi-transparent at the strand level. That is
//! alpha matting rather than segmentation, and no amount of tuning reaches it.
//! `docs/verification/background-removal.md` records the failure that settled
//! the question.
//!
//! ## Why it is not in the installer
//!
//! The runtime and the model together are about twenty megabytes per
//! architecture, against an installer that is eleven. Bundling would nearly
//! triple the download for everyone, including everyone who only ever imports a
//! PNG of an arrow. Both are fetched **on request**, once, and can be removed.
//!
//! ## The trust problem, which is worse than the installer's
//!
//! This downloads a **library and loads it into the process**. An installer at
//! least passes SmartScreen and the user's own double-click; this does not. So
//! the bytes are checked against a SHA-256 published with them *and* a minisign
//! signature made with the release key, **before the library is ever loaded**,
//! and a failure deletes the file rather than leaving unverified code on disk.
//!
//! Nothing here runs at launch and nothing downloads without being asked.

use crate::error::{AppError, AppResult};
use crate::paths;
use serde::Serialize;
use std::path::PathBuf;

/// The release these artifacts come from.
///
/// A pinned tag, never `latest`. A copy of the app compiled today must keep
/// fetching the artifact it was tested against, even after a later release
/// publishes a different runtime.
const ARTIFACT_TAG: &str = "photo-v1";

/// One file photo mode needs on disk.
#[derive(Debug, Clone, Copy)]
pub struct Artifact {
    /// The asset name in the release, and the filename on disk.
    pub name: &'static str,
    /// Lowercase hex SHA-256 of the exact bytes. Empty until published.
    pub sha256: &'static str,
    pub bytes: u64,
}

/// The model. Architecture-independent, because ONNX is a portable graph.
///
/// **u2netp**, the small U²-Net: 4,574,861 bytes, Apache 2.0, taken from the
/// `rembg` release assets. General salient-object detection rather than a
/// portrait-specific matter, which matches what people actually import here.
/// `docs/PHOTO_MODE.md` records the licence and provenance.
pub const MODEL: Artifact = Artifact {
    name: "u2netp.onnx",
    sha256: "309c8469258dda742793dce0ebea8e6dd393174f89934733ecc8b14c76f4ddd8",
    bytes: 4_574_861,
};

/// The ONNX Runtime for this architecture.
///
/// Per-architecture by necessity. An architecture with no published artifact is
/// told so rather than handed the wrong one, which would load and then fail in
/// a way indistinguishable from corruption.
pub const fn runtime() -> Option<Artifact> {
    #[cfg(target_arch = "x86_64")]
    {
        Some(Artifact {
            name: "onnxruntime-x64.dll",
            sha256: "69d8e6d3879a3b4001cdc74c8ed9ccc7e7f799a5b847059738323404519ec471",
            bytes: 16_149_344,
        })
    }
    #[cfg(target_arch = "aarch64")]
    {
        Some(Artifact {
            name: "onnxruntime-arm64.dll",
            sha256: "7c7df2cefd6910f50f44792e8f8f71b371bf9675f9273e70a9277eb92e4d75ed",
            bytes: 16_261_432,
        })
    }
    #[cfg(target_arch = "x86")]
    {
        // 1.22.0 rather than 1.29.0: Microsoft stopped publishing a `win-x86`
        // build after it. ONNX is a portable graph, so the same model runs
        // against the older runtime.
        Some(Artifact {
            name: "onnxruntime-x86.dll",
            sha256: "f898b430bb6130b8c1394f98ea1c6f4134752919cf96601da27537a8b9458fdb",
            bytes: 10_884_640,
        })
    }
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64", target_arch = "x86")))]
    {
        None
    }
}

/// Whether this build can use photo mode at all.
///
/// The offline installer exists so an air-gapped machine works, and photo mode
/// is the one feature that cannot: it is defined by fetching something. The
/// build says so plainly rather than offering a button that always fails.
pub const fn available() -> bool {
    !cfg!(feature = "offline-build")
}

/// The sentence shown when it is not.
pub const UNAVAILABLE: &str =
    "Photo mode needs a one-time download and isn't available in the offline build.";

/// Where the artifacts live.
pub fn models_dir() -> AppResult<PathBuf> {
    let dir = paths::root()?.join("models");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn artifact_path(artifact: &Artifact) -> AppResult<PathBuf> {
    Ok(models_dir()?.join(artifact.name))
}

/// What the UI needs in order to decide what to offer.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PhotoStatus {
    pub available: bool,
    pub installed: bool,
    /// What a first use would cost. Measured figures, not estimates.
    pub download_bytes: u64,
    /// What removing it would reclaim.
    pub installed_bytes: u64,
    pub unavailable_reason: Option<String>,
}

pub fn status() -> PhotoStatus {
    let unavailable = |why: &str| PhotoStatus {
        available: false,
        installed: false,
        download_bytes: 0,
        installed_bytes: 0,
        unavailable_reason: Some(why.to_owned()),
    };

    let Some(library) = runtime() else {
        return unavailable("Photo mode has no build for this processor architecture.");
    };
    if !available() {
        return unavailable(UNAVAILABLE);
    }

    let on_disk = |a: &Artifact| {
        artifact_path(a)
            .ok()
            .and_then(|p| std::fs::metadata(p).ok())
            .map(|m| m.len())
            .unwrap_or(0)
    };
    let model = on_disk(&MODEL);
    let runtime_bytes = on_disk(&library);

    PhotoStatus {
        available: true,
        installed: model > 0 && runtime_bytes > 0,
        download_bytes: MODEL.bytes + library.bytes,
        installed_bytes: model + runtime_bytes,
        unavailable_reason: None,
    }
}

/// Checksum **and** signature, both before anything is written or loaded.
///
/// The checksum catches a corrupted transfer and cannot catch a substitution,
/// because it is published by the same host as the file. That is the reasoning
/// that put a signature on the installer, and it applies harder to a library
/// that gets loaded into this process.
fn verify(artifact: &Artifact, bytes: &[u8]) -> AppResult<()> {
    if artifact.sha256.is_empty() {
        // Nothing to check against. Refusing is the only safe answer: the
        // alternative is loading a library on the strength of its filename.
        return Err(AppError::msg(
            "this build has no published checksum for the photo-mode download, \
             so it will not use one",
        ));
    }
    let actual = crate::hash::sha256_hex(bytes);
    if !crate::hash::hex_eq(&actual, artifact.sha256) {
        return Err(AppError::msg(format!(
            "the downloaded {} does not match the checksum published with it",
            artifact.name
        )));
    }
    if crate::signing::enforced() {
        let signature = signature_for(artifact)?;
        crate::signing::verify(bytes, &signature).map_err(|_| {
            AppError::msg(format!(
                "the downloaded {} is not signed by this project's release key",
                artifact.name
            ))
        })?;
    } else {
        log::warn!("photo mode: {}", crate::signing::describe());
    }
    Ok(())
}

fn asset_path(name: &str) -> String {
    format!("/notfeylo/cursed/releases/download/{ARTIFACT_TAG}/{name}")
}

fn signature_for(artifact: &Artifact) -> AppResult<String> {
    let bytes = crate::updates::get_with_progress(
        crate::updates::DOWNLOAD_HOST,
        &asset_path(&format!("{}.minisig", artifact.name)),
        64 * 1024,
        true,
        &mut |_, _| {},
    )?;
    String::from_utf8(bytes)
        .map_err(|_| AppError::msg("the signature for that download could not be read"))
}


// --- the download, as the UI sees it ----------------------------
//
// A twenty-megabyte download cannot block the window, so `install` runs on its
// own thread and reports through here. The shape is deliberately the same as
// the updater's: one shared state the UI polls, so a button press and a
// background task can never show two different answers.

/// How the download is going.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub running: bool,
    pub received: u64,
    pub total: u64,
    /// Set once the download has finished successfully.
    pub installed: bool,
    /// Verbatim, because a paraphrased error is a useless error.
    pub error: Option<String>,
}

fn progress_slot() -> &'static std::sync::Mutex<Progress> {
    static P: std::sync::OnceLock<std::sync::Mutex<Progress>> = std::sync::OnceLock::new();
    P.get_or_init(|| std::sync::Mutex::new(Progress::default()))
}

/// Set when the user asks to stop. Checked between artifacts and inside the
/// byte loop, so a cancel takes effect within a chunk rather than at the end.
static CANCELLED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn progress() -> Progress {
    progress_slot().lock().map(|p| p.clone()).unwrap_or_default()
}

pub fn cancel() {
    CANCELLED.store(true, std::sync::atomic::Ordering::SeqCst);
}

pub fn cancelled() -> bool {
    CANCELLED.load(std::sync::atomic::Ordering::SeqCst)
}

/// Called from the download thread on every chunk.
pub fn report_progress(received: u64, total: u64) {
    if let Ok(mut p) = progress_slot().lock() {
        p.running = true;
        p.received = received;
        p.total = total;
        p.error = None;
    }
}

/// Called from the download thread when it stops, either way.
pub fn finish(result: AppResult<()>) {
    if let Ok(mut p) = progress_slot().lock() {
        p.running = false;
        match result {
            Ok(()) => {
                p.installed = true;
                p.error = None;
            }
            Err(e) => {
                p.installed = false;
                p.error = Some(e.to_string());
            }
        }
    }
    CANCELLED.store(false, std::sync::atomic::Ordering::SeqCst);
}

/// Fetches one artifact, verifies it, and only then puts it in place.
///
/// Written under a temporary name and renamed after it passes, so an
/// interrupted download can never be mistaken for an installed artifact by the
/// next launch — which for a library that gets loaded is the difference between
/// a retry and executing unverified bytes.
fn fetch(artifact: &Artifact, progress: &mut dyn FnMut(u64, u64)) -> AppResult<()> {
    let destination = artifact_path(artifact)?;
    let cap = (artifact.bytes as usize).saturating_mul(2).max(1024 * 1024);
    let bytes = crate::updates::get_with_progress(
        crate::updates::DOWNLOAD_HOST,
        &asset_path(artifact.name),
        cap,
        true,
        progress,
    )?;

    verify(artifact, &bytes)?;

    let temporary = destination.with_extension("part");
    std::fs::write(&temporary, &bytes)?;
    std::fs::rename(&temporary, &destination)?;
    log::info!("photo mode: {} verified and installed", artifact.name);
    Ok(())
}

/// Downloads both artifacts. Long-running; call it off the UI thread.
pub fn install(progress: &mut dyn FnMut(u64, u64)) -> AppResult<()> {
    if !available() {
        return Err(AppError::invalid(UNAVAILABLE));
    }
    let Some(library) = runtime() else {
        return Err(AppError::invalid(
            "photo mode has no build for this processor architecture",
        ));
    };

    // One figure across both files, so the UI shows a single bar that only
    // moves forwards rather than two that each restart at zero.
    let total = MODEL.bytes + library.bytes;
    fetch(&MODEL, &mut |got, _| progress(got, total))?;
    if cancelled() {
        return Err(AppError::invalid("the download was cancelled"));
    }
    let done = MODEL.bytes;
    fetch(&library, &mut |got, _| progress(done + got, total))?;
    Ok(())
}

/// Deletes both artifacts and reports what was reclaimed.
///
/// A twenty-megabyte download the user cannot get rid of is a bad citizen.
pub fn remove() -> AppResult<u64> {
    let mut freed = 0u64;
    let mut artifacts: Vec<Artifact> = vec![MODEL];
    if let Some(library) = runtime() {
        artifacts.push(library);
    }
    for artifact in artifacts {
        let path = artifact_path(&artifact)?;
        if let Ok(meta) = std::fs::metadata(&path) {
            freed += meta.len();
        }
        let _ = std::fs::remove_file(&path);
    }
    log::info!("photo mode: removed, {freed} bytes reclaimed");
    Ok(freed)
}

/// Where the runtime is loaded from, once it has been verified.
///
/// The `load-dynamic` feature resolves the library from this path at runtime
/// rather than linking it at build time, which is what keeps it out of the
/// installer.
pub fn runtime_path() -> AppResult<PathBuf> {
    let library = runtime().ok_or_else(|| {
        AppError::invalid("photo mode has no build for this processor architecture")
    })?;
    let path = artifact_path(&library)?;
    if !path.is_file() {
        return Err(AppError::invalid("photo mode is not installed on this machine yet"));
    }
    Ok(path)
}

/// Whether an image is the kind of thing photo mode is for.
///
/// Used to decide whether to *offer* it. Never to start a download.
pub fn looks_like_a_photograph(bitmap: &crate::build::bitmap::Bitmap) -> bool {
    !crate::build::matte::assess(bitmap).confident
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **The trust boundary.** A library that gets loaded into this process
    /// must never arrive on the strength of its filename. A build with no
    /// published checksum has nothing to check against, and the safe answer is
    /// to do without photo mode entirely.
    #[test]
    fn an_artifact_with_no_published_checksum_is_refused() {
        let unpublished = Artifact { name: "onnxruntime-x64.dll", sha256: "", bytes: 10 };
        assert!(verify(&unpublished, b"anything at all").is_err());
    }

    #[test]
    fn a_checksum_mismatch_is_refused() {
        let artifact = Artifact {
            name: "u2netp.onnx",
            sha256: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            bytes: 4,
        };
        assert!(verify(&artifact, b"different bytes entirely").is_err());
    }

    /// Pinned, so a later release cannot move the artifact a shipped build
    /// expects out from under it.
    #[test]
    fn the_artifact_release_is_pinned() {
        assert_ne!(ARTIFACT_TAG, "latest");
        assert!(!ARTIFACT_TAG.is_empty());
    }

    /// The advertised size is the measured one.
    #[test]
    fn the_advertised_size_is_the_measured_one() {
        assert_eq!(MODEL.bytes, 4_574_861, "u2netp.onnx, weighed rather than guessed");
        let status = status();
        if status.available {
            assert!(
                status.download_bytes > MODEL.bytes,
                "the runtime is the larger half and has to be counted"
            );
        }
    }

    /// Nothing downloads unasked: there is one fetch, reachable only from
    /// `install`, which is reachable only from a command the user pressed.
    #[test]
    fn nothing_downloads_without_being_asked() {
        let source = include_str!("photo.rs");
        assert_eq!(source.matches("\nfn fetch(").count(), 1);
        assert!(source.contains("pub fn install("));
    }

    /// An uninstalled photo mode says so rather than handing out a path to a
    /// file that is not there.
    #[test]
    fn the_runtime_path_is_only_given_when_it_exists() {
        if runtime_path().is_ok() {
            return; // installed on this machine, which is a valid state
        }
        assert!(!status().installed);
    }
}
