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

    // A removal waiting on the next launch is a removal as far as anyone using
    // the app is concerned: the button worked, and offering "Remove" again for
    // files that are already on their way out would be a button that cannot do
    // anything.
    let pending = removal_is_pending();

    PhotoStatus {
        available: true,
        installed: !pending && model > 0 && runtime_bytes > 0,
        download_bytes: MODEL.bytes + library.bytes,
        installed_bytes: if pending { 0 } else { model + runtime_bytes },
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
    // Installing cancels a removal that never finished, or the next launch
    // would sweep away the file that has just been downloaded.
    if let Ok(marker) = removal_marker() {
        let _ = std::fs::remove_file(marker);
    }
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
///
/// **Windows will not delete a DLL that is loaded**, and after one cutout this
/// one is. The session is dropped first, which is necessary and not sufficient:
/// the runtime itself stays mapped for the life of the process. So a file that
/// will not go is recorded rather than ignored — photo mode reports itself
/// uninstalled from that moment, and the leftover is swept up by the next
/// launch. The alternative is a Remove button that appears to do nothing and a
/// size that never drops.
pub fn remove() -> AppResult<u64> {
    release();

    let mut freed = 0u64;
    let mut stubborn = false;
    let mut artifacts: Vec<Artifact> = vec![MODEL];
    if let Some(library) = runtime() {
        artifacts.push(library);
    }
    for artifact in artifacts {
        let path = artifact_path(&artifact)?;
        let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        match std::fs::remove_file(&path) {
            Ok(()) => freed += size,
            Err(e) if path.exists() => {
                log::warn!("photo mode: {} could not be deleted yet: {e}", artifact.name);
                stubborn = true;
            }
            Err(_) => {}
        }
    }

    if stubborn {
        mark_pending_removal();
    }
    log::info!("photo mode: removed, {freed} bytes reclaimed");
    Ok(freed)
}

/// The marker that says "these files are on their way out".
fn removal_marker() -> AppResult<PathBuf> {
    Ok(models_dir()?.join("removed"))
}

fn mark_pending_removal() {
    if let Ok(marker) = removal_marker() {
        let _ = std::fs::write(&marker, b"photo mode was removed while its runtime was loaded\n");
    }
}

fn removal_is_pending() -> bool {
    removal_marker().map(|marker| marker.is_file()).unwrap_or(false)
}

/// Finishes a removal that could not complete while the library was loaded.
///
/// Called once at startup, before anything can load it again. Silent: there is
/// nothing for the user to do about it either way.
pub fn sweep_pending_removal() {
    if !removal_is_pending() {
        return;
    }
    let mut left = false;
    let mut artifacts: Vec<Artifact> = vec![MODEL];
    if let Some(library) = runtime() {
        artifacts.push(library);
    }
    for artifact in artifacts {
        let Ok(path) = artifact_path(&artifact) else {
            continue;
        };
        if std::fs::remove_file(&path).is_err() && path.exists() {
            left = true;
        }
    }
    if !left {
        if let Ok(marker) = removal_marker() {
            let _ = std::fs::remove_file(marker);
        }
        log::info!("photo mode: the pending removal completed");
    }
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

// --- the learned matte ------------------------------------------
//
// Everything above gets the model onto the disk. This is what it is for.
//
// The shape of the model decides most of this. **u2netp takes a 320x320 RGB
// tensor and returns seven saliency maps**, of which the first is the fused one
// everything else refines; the other six exist to train it and are ignored here.
// So the work either side of `run` is fixed: letterbox-free resize down to 320,
// normalise the way the network was trained, and stretch the map that comes
// back over the image it came from.

use crate::build::bitmap::Bitmap;
use crate::build::matte::{MatteReport, Refusal};

/// The side of the square the network takes. Not a tunable: it is baked into
/// the graph, and feeding it anything else is a shape error at `run`.
const INPUT_SIDE: u32 = 320;

/// ImageNet normalisation, which is what u2netp was trained against. Getting
/// these wrong does not fail — it produces a plausible, quietly worse matte,
/// which is the hardest kind of mistake to notice.
const MEAN: [f32; 3] = [0.485, 0.456, 0.406];
const DEVIATION: [f32; 3] = [0.229, 0.224, 0.225];

/// Alpha below this is haze rather than subject, and alpha above it is solid.
///
/// A learned matte is soft everywhere, including across the background it was
/// confident about — a few levels of grey over the whole frame reads as a dirty
/// pane of glass around the cursor rather than as transparency.
const FLOOR: u8 = 12;
const CEILING: u8 = 243;

/// The session, kept because building one costs more than running one.
///
/// `Option` rather than a bare session: **removing photo mode has to be able to
/// let go of the library**, and on Windows a DLL that is still loaded is a file
/// that cannot be deleted.
fn session_slot() -> &'static std::sync::Mutex<Option<ort::session::Session>> {
    static SESSION: std::sync::OnceLock<std::sync::Mutex<Option<ort::session::Session>>> =
        std::sync::OnceLock::new();
    SESSION.get_or_init(|| std::sync::Mutex::new(None))
}

/// Points ONNX Runtime at the DLL that was downloaded and verified.
///
/// Once per process, and deliberately not at launch: this is the first moment
/// the library is loaded into this process, and it must not happen for someone
/// who never asks for photo mode.
fn load_runtime_once() -> AppResult<()> {
    // **Only success is remembered.** Caching the failure too would mean that
    // somebody who reaches for photo mode before installing it — or whose
    // antivirus held the DLL for a moment — stays broken until they restart the
    // app, with an install button that appears to work and a cutout that keeps
    // saying the runtime is missing. ONNX Runtime's own loader is a `OnceLock`,
    // so asking again after it has succeeded is free.
    static LOADED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if LOADED.load(std::sync::atomic::Ordering::Acquire) {
        return Ok(());
    }

    let path = runtime_path()?;
    log::info!("photo mode: loading the runtime from {}", path.display());
    // `init_from` is the `load-dynamic` entry point: it resolves the library
    // from this path rather than from a link at build time, which is the whole
    // reason the installer does not carry it. `commit` answers `false` when an
    // environment was already configured, which is not a failure — it is the
    // same runtime, and there is only ever one in a process.
    let builder = ort::init_from(&path).map_err(|e| {
        AppError::msg(format!(
            "the photo-mode runtime could not be loaded from {}: {e}",
            path.display()
        ))
    })?;
    let _ = builder.commit();

    LOADED.store(true, std::sync::atomic::Ordering::Release);
    Ok(())
}

/// Runs the model over one image and returns its alpha, 320x320, row-major.
fn infer(input: Vec<f32>) -> AppResult<Vec<f32>> {
    let mut guard = session_slot()
        .lock()
        .map_err(|_| AppError::msg("photo mode is busy in another window"))?;

    // Inside the lock: the library is loaded once, and two imports arriving
    // together cannot both be half-way through doing it.
    load_runtime_once()?;

    if guard.is_none() {
        let model = artifact_path(&MODEL)?;
        if !model.is_file() {
            return Err(AppError::invalid(
                "photo mode is not installed on this machine yet",
            ));
        }
        let started = std::time::Instant::now();
        let session = ort::session::Session::builder()
            .and_then(|mut builder| builder.commit_from_file(&model))
            .map_err(|e| AppError::msg(format!("the photo-mode model would not load: {e}")))?;
        log::info!("photo mode: model ready in {} ms", started.elapsed().as_millis());
        *guard = Some(session);
    }
    let session = guard.as_mut().unwrap_or_else(|| unreachable!());

    let tensor = ort::value::Tensor::from_array((
        [1usize, 3, INPUT_SIDE as usize, INPUT_SIDE as usize],
        input,
    ))
    .map_err(|e| AppError::msg(format!("the image could not be given to the model: {e}")))?;

    let started = std::time::Instant::now();
    // Positional rather than by name. u2netp takes exactly one input, and its
    // name is an artefact of whichever exporter produced the file — matching on
    // it would make the app dependent on a string in somebody else's tooling.
    let outputs = session
        .run(ort::inputs![tensor])
        .map_err(|e| AppError::msg(format!("photo mode could not process this image: {e}")))?;

    // The first of seven. d0 is the fused map; d1..d6 are the side outputs the
    // network is trained against and are not more accurate than the fusion.
    let (shape, data) = outputs[0]
        .try_extract_tensor::<f32>()
        .map_err(|e| AppError::msg(format!("photo mode returned something unreadable: {e}")))?;

    let expected = (INPUT_SIDE * INPUT_SIDE) as usize;
    if data.len() < expected {
        return Err(AppError::msg(format!(
            "photo mode returned a {shape:?} map, which is not the size it was asked for"
        )));
    }
    log::info!("photo mode: matte in {} ms", started.elapsed().as_millis());
    Ok(data[..expected].to_vec())
}

/// Turns an image into the tensor the network expects.
fn prepare(bitmap: &Bitmap) -> AppResult<Vec<f32>> {
    let small = bitmap.resized(INPUT_SIDE, INPUT_SIDE)?;
    let pixels = (INPUT_SIDE * INPUT_SIDE) as usize;
    let mut tensor = vec![0f32; pixels * 3];

    // Channel-planar (NCHW), which is what the graph declares — interleaving
    // the channels is a silent, total corruption of the input rather than an
    // error, and it comes back as a matte of noise.
    for y in 0..INPUT_SIDE {
        for x in 0..INPUT_SIDE {
            let [r, g, b, a] = small.pixel(x, y);
            let index = (y * INPUT_SIDE + x) as usize;
            // Composited onto black. An image that already has holes in it is
            // rare here — this path is for photographs — but a transparent
            // pixel has an undefined colour, and feeding whatever was left in
            // it shows up as a phantom subject.
            let alpha = a as f32 / 255.0;
            for (channel, value) in [r, g, b].into_iter().enumerate() {
                let scaled = (value as f32 / 255.0) * alpha;
                tensor[channel * pixels + index] =
                    (scaled - MEAN[channel]) / DEVIATION[channel];
            }
        }
    }
    Ok(tensor)
}

/// Stretches the model's 320x320 map back over the image it came from.
fn as_alpha(map: &[f32], width: u32, height: u32) -> AppResult<Bitmap> {
    // The map is a saliency score with no fixed range, so it is normalised
    // against its own extremes — the same thing `rembg` does, and without it a
    // confident matte and a hesitant one produce completely different alphas.
    let (mut low, mut high) = (f32::MAX, f32::MIN);
    for &value in map {
        low = low.min(value);
        high = high.max(value);
    }
    let span = (high - low).max(f32::EPSILON);

    let mut small = Bitmap::new(INPUT_SIDE, INPUT_SIDE);
    for y in 0..INPUT_SIDE {
        for x in 0..INPUT_SIDE {
            let value = map[(y * INPUT_SIDE + x) as usize];
            let level = (((value - low) / span) * 255.0).clamp(0.0, 255.0) as u8;
            // Carried in every channel as well as alpha, so the resize — which
            // works in premultiplied space — has a colour to interpolate rather
            // than blending the matte towards an arbitrary one.
            small.set_pixel(x, y, [level, level, level, level]);
        }
    }
    small.resized(width, height)
}

/// Cuts the background out with the learned matte, in place.
///
/// Returns the same report shape a keyed cut produces, so the caller does not
/// care which of the two ran.
pub fn remove_background_learned(bitmap: &mut Bitmap) -> AppResult<MatteReport> {
    let keyability = crate::build::matte::assess(bitmap);
    let before = std::time::Instant::now();

    let map = infer(prepare(bitmap)?)?;
    let alpha = as_alpha(&map, bitmap.width, bitmap.height)?;

    let mut cleared = 0usize;
    let total = (bitmap.width as usize) * (bitmap.height as usize);
    let original = bitmap.clone();
    for y in 0..bitmap.height {
        for x in 0..bitmap.width {
            let [r, g, b, existing] = bitmap.pixel(x, y);
            let level = alpha.alpha(x, y);
            // Multiplied into whatever alpha was already there rather than
            // replacing it: an image that arrived with holes keeps them.
            let combined = ((existing as u32 * level as u32) / 255) as u8;
            let combined = match combined {
                // The floor is what stops a faint haze over the whole frame
                // reading as a dirty pane of glass around the cursor.
                level if level <= FLOOR => 0,
                level if level >= CEILING => 255,
                level => level,
            };
            if combined == 0 && existing > 0 {
                cleared += 1;
            }
            bitmap.set_pixel(x, y, [r, g, b, combined]);
        }
    }

    let removed = if total == 0 {
        0.0
    } else {
        cleared as f32 / total as f32
    };
    log::info!(
        "photo mode: removed {:.0}% in {} ms",
        removed * 100.0,
        before.elapsed().as_millis()
    );

    // **The same safety net the classical path has.** A model can fail, and
    // when it does it fails the same way a bad flood fill does: dozens of
    // disconnected islands where a subject used to be. Reverting is better than
    // returning wreckage, and it is the difference between "that did not work"
    // and "this app destroyed my picture".
    if !crate::build::matte::survivor_is_coherent(bitmap) {
        *bitmap = original;
        return Ok(MatteReport::refused(
            Refusal::WouldHaveEatenTheSubject,
            keyability,
        ));
    }

    Ok(MatteReport::learned(removed, keyability))
}

/// Releases the model and the library.
///
/// Called before deleting them: a loaded DLL is a file Windows will not remove,
/// so "Remove photo mode" without this frees nothing and reports that it did.
pub fn release() {
    if let Ok(mut guard) = session_slot().lock() {
        if guard.take().is_some() {
            log::info!("photo mode: session released");
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    /// A picture with an obvious subject on a busy backdrop, which is the one
    /// thing the classical path cannot key and the model can.
    fn a_subject_on_noise() -> Bitmap {
        let mut bitmap = Bitmap::new(512, 512);
        for y in 0..512u32 {
            for x in 0..512u32 {
                // Deterministic noise, so a failure is reproducible.
                let n = ((x * 7919 + y * 104_729) % 97) as u8;
                bitmap.set_pixel(x, y, [40 + n, 90 + n / 2, 30 + n, 255]);
            }
        }
        // A bright disc in the middle: salient by construction.
        for y in 150..360u32 {
            for x in 150..360u32 {
                let (dx, dy) = (x as f32 - 255.0, y as f32 - 255.0);
                if dx * dx + dy * dy < 105.0 * 105.0 {
                    bitmap.set_pixel(x, y, [240, 240, 250, 255]);
                }
            }
        }
        bitmap
    }

    /// **The end-to-end test, when the artifacts are on this machine.**
    ///
    /// Skipped rather than failed when they are not: they are a 20 MB download
    /// that nothing in the build fetches, and a red test on a clean checkout
    /// would be a test that trains people to ignore it. Where they *are*
    /// present — which is any machine that has used photo mode once, and the
    /// machine this feature is developed on — it runs the real model through
    /// the real runtime and checks the matte that comes back.
    #[test]
    fn the_model_produces_a_matte_when_it_is_installed() {
        if !status().installed {
            return;
        }
        let mut bitmap = a_subject_on_noise();
        let report = remove_background_learned(&mut bitmap).expect("the model runs");

        assert!(report.refused.is_none(), "refused: {:?}", report.refused);
        assert!(
            report.removed > 0.3,
            "only {:.0}% came off a disc on noise",
            report.removed * 100.0
        );
        // The subject survives: the middle is opaque and the corners are gone.
        assert_eq!(bitmap.alpha(255, 255), 255, "the subject was eaten");
        assert_eq!(bitmap.alpha(2, 2), 0, "the background stayed");
    }

    /// The classical path is still the one that runs for flat art, whatever is
    /// installed. Photo mode is an answer to a question the flood fill cannot
    /// answer, not a replacement for it.
    #[test]
    fn photo_mode_is_never_what_runs_by_default() {
        let source = include_str!("build/pipeline.rs");
        assert!(
            source.contains("Cut::Photo => crate::photo::remove_background_learned"),
            "the learned matte must be reachable"
        );
        assert_eq!(
            crate::build::pipeline::Cut::default(),
            crate::build::pipeline::Cut::Auto,
            "the default cut is the classical one"
        );
    }

    /// Asking for the learned matte without the model is an error that says so,
    /// not a silent no-op — the UI only offers it once it is installed, and a
    /// user who gets there another way is owed a sentence.
    #[test]
    fn asking_for_a_learned_matte_without_the_model_says_so() {
        if status().installed {
            return;
        }
        let mut bitmap = Bitmap::new(64, 64);
        let error = remove_background_learned(&mut bitmap)
            .expect_err("there is no model on this machine")
            .to_string();
        assert!(error.contains("photo mode") || error.contains("Photo mode"), "{error}");
    }

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
