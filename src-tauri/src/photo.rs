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
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use windows::core::{HRESULT, PCWSTR};
use windows::Win32::Foundation::ERROR_MOD_NOT_FOUND;
use windows::Win32::System::LibraryLoader::LoadLibraryW;

fn wide(path: &Path) -> Vec<u16> {
    path.as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

/// The release these artifacts come from.
///
/// A pinned tag, never `latest`. A copy of the app compiled today must keep
/// fetching the artifact it was tested against, even after a later release
/// publishes a different runtime.
const ARTIFACT_TAG: &str = "photo-v1";

/// One file photo mode needs on disk.
#[derive(Debug, Clone, Copy)]
pub struct Artifact {
    /// **The filename on disk, which for the C++ runtime is not a free choice.**
    /// Windows satisfies an import from the list of modules already loaded in
    /// the process, matched by base name, so a downloaded `msvcp140.dll`
    /// answers the runtime's import only while it is called exactly that.
    pub name: &'static str,
    /// The asset name in the release. The same as `name` wherever one file
    /// serves every machine, and architecture-tagged where it cannot be: three
    /// different `msvcp140.dll` cannot share one name in one release.
    pub asset: &'static str,
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
    asset: "u2netp.onnx",
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
            asset: "onnxruntime-x64.dll",
            sha256: "69d8e6d3879a3b4001cdc74c8ed9ccc7e7f799a5b847059738323404519ec471",
            bytes: 16_149_344,
        })
    }
    #[cfg(target_arch = "aarch64")]
    {
        Some(Artifact {
            name: "onnxruntime-arm64.dll",
            asset: "onnxruntime-arm64.dll",
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
            asset: "onnxruntime-x86.dll",
            sha256: "f898b430bb6130b8c1394f98ea1c6f4134752919cf96601da27537a8b9458fdb",
            bytes: 10_884_640,
        })
    }
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64", target_arch = "x86")))]
    {
        None
    }
}

/// The Microsoft C++ runtime the ONNX Runtime was built against.
///
/// **This is what shipped broken in 1.22.0.** `onnxruntime-x64.dll` imports
/// `MSVCP140.dll`, `MSVCP140_1.dll`, `VCRUNTIME140.dll` and
/// `VCRUNTIME140_1.dll` — the Visual C++ redistributable, which is not part of
/// Windows. This app itself never needed it: Rust links the MSVC C runtime
/// statically, so `Cursed.exe` imports only the OS and the UCRT and runs on a
/// bare install of Windows. The result was the worst shape a dependency bug
/// comes in — an app that starts perfectly and one feature inside it that
/// answers `LoadLibraryExW failed` for a reason nothing on screen explains.
///
/// It survived every test because installing Visual Studio, the build tools, or
/// almost any other developer runtime installs these files. **A machine that has
/// never built anything is the only machine that can find this**, which is why
/// it took until a clean VM.
///
/// Per architecture, because the set genuinely differs. `VCRUNTIME140_1.dll` is
/// the x64 C++ exception helper: the 32-bit redistributable does not contain it
/// at all, and the ARM64 runtime does not import it. Each list is the transitive
/// closure of the imports, checked against the published artifact rather than
/// assumed — everything else these files reach for is the UCRT, which has been
/// part of Windows since long before the 1803 floor.
///
/// **Order is load order**, and it matters: each file is loaded by absolute
/// path, and a file loaded that way still resolves *its own* imports through
/// the normal search. `msvcp140.dll` needs `vcruntime140.dll` to already be
/// in the process.
pub const fn crt() -> &'static [Artifact] {
    #[cfg(target_arch = "x86_64")]
    {
        &[
            Artifact {
                name: "vcruntime140.dll",
                asset: "vcruntime140-x64.dll",
                sha256: "d1f4225df2cd877dbf130d5668a021dce3f94118455ff5ec952061c30afc9ce7",
                bytes: 178_616,
            },
            Artifact {
                name: "vcruntime140_1.dll",
                asset: "vcruntime140_1-x64.dll",
                sha256: "a7146c08f89fe5b04541ab507cdb59ff7b44534d4ba3c668a426c6450a03434e",
                bytes: 50_112,
            },
            Artifact {
                name: "msvcp140.dll",
                asset: "msvcp140-x64.dll",
                sha256: "7c26614e1d733892c2deac7e245ce115504b1d80592dd0a01b08e3e5a55f89ca",
                bytes: 643_512,
            },
            Artifact {
                name: "msvcp140_1.dll",
                asset: "msvcp140_1-x64.dll",
                sha256: "206c931bf90fdad8816de3b5e2ef80b2bcaa9406c89ecc05fe6fddffe251e982",
                bytes: 35_768,
            },
        ]
    }
    #[cfg(target_arch = "aarch64")]
    {
        &[
            Artifact {
                name: "vcruntime140.dll",
                asset: "vcruntime140-arm64.dll",
                sha256: "3c56f4167e2b3d8e6338497731e6aae8cd7ec46bd6789f9423a8d9cf9a630310",
                bytes: 246_112,
            },
            Artifact {
                name: "msvcp140.dll",
                asset: "msvcp140-arm64.dll",
                sha256: "167ceac85c2d726c4cd9e39b8881fafdc6de1520c0e67c4f6b271235f9d2a6c5",
                bytes: 1_588_064,
            },
            Artifact {
                name: "msvcp140_1.dll",
                asset: "msvcp140_1-arm64.dll",
                sha256: "c7713c2061b29d24de4874923522ada0815cfca80d09265c181e81d16c7c42d6",
                bytes: 50_528,
            },
        ]
    }
    #[cfg(target_arch = "x86")]
    {
        &[
            Artifact {
                name: "vcruntime140.dll",
                asset: "vcruntime140-x86.dll",
                sha256: "2fa6efc053203460a23d3a25158f227d895d2dadc63acc1a372da97c3a4281c3",
                bytes: 123_328,
            },
            Artifact {
                name: "msvcp140.dll",
                asset: "msvcp140-x86.dll",
                sha256: "f0cda2a0cf1fe6fbbf579b9098462329d3aaa7513a207af2e0f33b01456e388a",
                bytes: 618_944,
            },
            Artifact {
                name: "msvcp140_1.dll",
                asset: "msvcp140_1-x86.dll",
                sha256: "b08edd7954ae9954d24eb344a8b0116a980c60179ff1b92d5219639e21028cc2",
                bytes: 33_728,
            },
        ]
    }
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64", target_arch = "x86")))]
    {
        &[]
    }
}

/// Everything a working photo mode needs on disk, in the order it is fetched.
///
/// One list, so that installing, removing, sweeping a failed removal and
/// deciding whether the feature is installed can never disagree about what the
/// feature *is*. Adding the C++ runtime to this list is what makes an existing
/// install notice that it is now missing something.
fn artifacts() -> Vec<Artifact> {
    let mut all = required();
    all.extend_from_slice(crt());
    all
}

/// The two without which there is no photo mode at all.
///
/// **The C++ runtime is deliberately not in here.** It is a copy of something
/// most machines already have in System32, carried for the ones that do not, so
/// a machine that cannot fetch it is not a machine that has lost the feature —
/// it is the machine every release before this one ran on. Treating it as
/// required would mean an unreachable asset, or a release published in the
/// wrong order, breaking photo mode for everybody rather than for nobody.
fn required() -> Vec<Artifact> {
    let mut all = vec![MODEL];
    if let Some(library) = runtime() {
        all.push(library);
    }
    all
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

    let Some(_library) = runtime() else {
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
    let all = artifacts();
    let installed_bytes: u64 = all.iter().map(on_disk).sum();
    // Judged on what photo mode cannot work without. A machine that has the
    // model and the runtime *is* installed, whether or not the C++ runtime came
    // down beside them — see `required`.
    let complete = required().iter().all(|a| on_disk(a) > 0);

    // A removal waiting on the next launch is a removal as far as anyone using
    // the app is concerned: the button worked, and offering "Remove" again for
    // files that are already on their way out would be a button that cannot do
    // anything.
    let pending = removal_is_pending();

    PhotoStatus {
        available: true,
        installed: !pending && complete,
        download_bytes: all.iter().map(|a| a.bytes).sum(),
        installed_bytes: if pending { 0 } else { installed_bytes },
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
        &asset_path(&format!("{}.minisig", artifact.asset)),
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
        &asset_path(artifact.asset),
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
    if runtime().is_none() {
        return Err(AppError::invalid(
            "photo mode has no build for this processor architecture",
        ));
    }

    // One figure across every file, so the UI shows a single bar that only
    // moves forwards rather than one that restarts at zero per artifact.
    let all = artifacts();
    let total: u64 = all.iter().map(|a| a.bytes).sum();
    let optional: Vec<&'static str> = crt().iter().map(|a| a.name).collect();
    let mut done = 0u64;
    for artifact in &all {
        if cancelled() {
            return Err(AppError::invalid("the download was cancelled"));
        }
        // **Skipped when it is already here and already right.** Adding the C++
        // runtime to the list would otherwise make everybody who already has
        // photo mode fetch twenty megabytes again to be handed the eight
        // hundred kilobytes they were actually missing.
        if already_installed(artifact) {
            log::info!("photo mode: {} is already installed", artifact.name);
            done += artifact.bytes;
            progress(done, total);
            continue;
        }
        match fetch(artifact, &mut |got, _| progress(done + got, total)) {
            Ok(()) => {}
            // **A missing C++ runtime is not a failed install.** Most machines
            // resolve those from System32 and never touch these copies, so
            // refusing the whole install because one of them could not be
            // fetched would break photo mode for everyone in order to fix it
            // for the few. What it costs is one honest sentence later:
            // `diagnose` names the file if the runtime then will not load.
            Err(e) if optional.contains(&artifact.name) => {
                log::warn!(
                    "photo mode: {} could not be downloaded ({e}); \
                     carrying on, because this PC may already have it",
                    artifact.name
                );
            }
            Err(e) => return Err(e),
        }
        done += artifact.bytes;
    }
    Ok(())
}

/// Whether this artifact is on disk *and* is the artifact it claims to be.
///
/// By hash rather than by size, because the point of it is to skip a download
/// safely: a file that hashes to the constant compiled into this build is
/// byte-for-byte the file that passed both the checksum and the signature when
/// it was written. A size match alone would wave through a truncated or
/// substituted file, which for a library this process is about to load is the
/// one mistake worth never making.
fn already_installed(artifact: &Artifact) -> bool {
    if artifact.sha256.is_empty() {
        return false;
    }
    let Ok(path) = artifact_path(artifact) else {
        return false;
    };
    let Ok(bytes) = std::fs::read(&path) else {
        return false;
    };
    bytes.len() as u64 == artifact.bytes
        && crate::hash::hex_eq(&crate::hash::sha256_hex(&bytes), artifact.sha256)
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
    for artifact in artifacts() {
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
    for artifact in artifacts() {
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
    // Before the runtime, the runtime's own dependencies. See `preload_crt`.
    preload_crt();
    // `init_from` is the `load-dynamic` entry point: it resolves the library
    // from this path rather than from a link at build time, which is the whole
    // reason the installer does not carry it. `commit` answers `false` when an
    // environment was already configured, which is not a failure — it is the
    // same runtime, and there is only ever one in a process.
    let builder = ort::init_from(&path).map_err(|e| {
        AppError::msg(format!(
            "the photo-mode runtime could not be loaded from {}: {e}{}",
            path.display(),
            diagnose(&path)
        ))
    })?;
    let _ = builder.commit();

    LOADED.store(true, std::sync::atomic::Ordering::Release);
    Ok(())
}

/// Loads the C++ runtime into this process before anything asks for it.
///
/// **Why a preload, and not simply a file in the same folder.** `ort` opens the
/// runtime through `libloading::Library::new`, which is
/// `LoadLibraryExW(path, NULL, 0)` — no `LOAD_WITH_ALTERED_SEARCH_PATH`.
/// Windows therefore resolves `onnxruntime-x64.dll`'s own imports through the
/// standard search order, and the standard search order contains this
/// executable's directory and System32 and pointedly **not** the directory the
/// library itself was loaded from. Dropping `msvcp140.dll` next to it in
/// `models` and expecting that to work is the obvious fix and it does nothing.
///
/// Loading each dependency first, by absolute path, does work — for a reason
/// worth writing down. An import is satisfied from the list of modules already
/// loaded in the process, matched **by base name**, before any search of the
/// disk happens. Once `msvcp140.dll` is in that list, the runtime's import of
/// it resolves with no search at all, wherever the file came from.
///
/// Best-effort, file by file. A missing copy is not an error here: most
/// machines have the redistributable in System32 and never needed this, and an
/// install made before these files joined the list must still be able to load.
/// If it genuinely cannot be resolved, `diagnose` says so afterwards in a
/// sentence that names the file.
fn preload_crt() {
    for artifact in crt() {
        let Ok(path) = artifact_path(artifact) else {
            continue;
        };
        if !path.is_file() {
            continue;
        }
        let wide_path = wide(&path);
        match unsafe { LoadLibraryW(PCWSTR(wide_path.as_ptr())) } {
            // **Never freed, deliberately.** Staying in the loaded-module list
            // for the life of the process is the whole mechanism; unloading it
            // again would leave the import with nothing to match.
            Ok(_) => log::debug!("photo mode: preloaded {}", artifact.name),
            Err(e) => log::warn!(
                "photo mode: {} could not be preloaded: {}",
                artifact.name,
                crate::error::describe_win32(&e)
            ),
        }
    }
}

/// Why the load really failed, asked of Windows instead of of `ort`.
///
/// **The layer that knows will not say.** `ort` formats a load failure as
/// "failed to load from `…`: {e}", where `{e}` is a `libloading::Error` whose
/// entire `Display` is the four words "LoadLibraryExW failed"; the real error
/// lives in that type's `source`, and `ort`'s own error implements `source`
/// as `None`, so the chain is cut before it reaches anything useful. A missing
/// dependency (126), a file that is not a library of this architecture (193)
/// and one an antivirus is holding open (5) all arrive as the same four words.
/// That is exactly what a user on a clean machine was shown.
///
/// So the same file is opened again here, and this time the answer is kept.
/// Only ever on the failing path, where one more `LoadLibraryW` costs nothing.
fn diagnose(path: &Path) -> String {
    let wide_path = wide(path);
    let Err(error) = (unsafe { LoadLibraryW(PCWSTR(wide_path.as_ptr())) }) else {
        // It loaded this time, so the load was not what failed — `ort` also
        // refuses a runtime older than the one it was built against. Adding a
        // guess here would be worse than adding nothing.
        return String::new();
    };
    let mut said = format!(" — Windows says: {}", crate::error::describe_win32(&error));
    if error.code() == HRESULT::from_win32(ERROR_MOD_NOT_FOUND.0) {
        let missing: Vec<&str> = crt()
            .iter()
            .map(|a| a.name)
            .filter(|name| !module_resolves(name))
            .collect();
        if !missing.is_empty() {
            said.push_str(&format!(
                ". {} could not be found on this PC — that is the Microsoft Visual C++ Runtime,                  which photo mode's library is built against and Windows does not include.                  Installing photo mode again downloads a copy of it",
                missing.join(", ")
            ));
        }
    }
    said
}

/// Whether Windows can resolve a module by bare name, through the search order
/// it would use for an import.
fn module_resolves(name: &str) -> bool {
    let wide_name: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe { LoadLibraryW(PCWSTR(wide_name.as_ptr())) }.is_ok()
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
        let failed = |e: &dyn std::fmt::Display| {
            AppError::msg(format!("the photo-mode model would not load: {e}"))
        };
        let mut builder = ort::session::Session::builder().map_err(|e| failed(&e))?;
        // **Without the arena.** ONNX Runtime's CPU allocator is a growing
        // arena by default: it takes memory from the OS as the graph runs and
        // then keeps it for the life of the session, on the assumption that the
        // next run wants it back. For a server answering requests all day that
        // is exactly right. For a cursor app it is not — measured on this
        // model, one cutout committed **554 MB and held it**, no matter how
        // small the picture was, because u2netp always runs at 320x320 and the
        // arena sizes itself to the graph rather than to the image.
        //
        // A tray application sitting on half a gigabyte after somebody removed
        // a background once is wrong on any machine. On a small one it is fatal:
        // Rust aborts on a failed allocation, and an abort takes the whole app
        // with it without reaching the panic hook, which is precisely the
        // "it just crashes after a few goes" this was reported as.
        //
        // Turning the arena off gives the memory back after every run — 554 MB
        // becomes 26 MB, and 27 MB at rest rather than 557 — for about 11 ms on
        // a 91 ms inference. That trade is not close. `docs/PHOTO_MODE.md` has
        // the table.
        builder = builder
            .with_execution_providers([ort::ep::CPU::default()
                .with_arena_allocator(false)
                .build()])
            .map_err(|e| failed(&e))?;
        let session = builder.commit_from_file(&model).map_err(|e| failed(&e))?;
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

    /// **One at a time.** Every test below drives the real model through the
    /// one shared session, and one of them measures this process's memory while
    /// it does. Left to run in parallel they measure each other: a concurrent
    /// cutout, or a session that another test has just released and is
    /// rebuilding, lands in the same reading — which is how the memory test
    /// first reported 277 MB for a 64x64 picture that costs 26.
    fn one_at_a_time() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// The two files a learned matte actually opens.
    ///
    /// **Not `status().installed`**, which asks a stricter question: it also
    /// wants the C++ runtime downloaded into `models`, and is therefore false
    /// on every machine that resolves `msvcp140.dll` from System32 — which is
    /// every machine with a compiler on it, including this one. Guarding on the
    /// stricter question does not fail the tests below, it *skips* them, which
    /// is the failure mode worth spending a helper to avoid.
    fn the_model_and_runtime_are_here() -> bool {
        let present = |a: &Artifact| artifact_path(a).map(|p| p.is_file()).unwrap_or(false);
        present(&MODEL) && runtime().is_some_and(|r| present(&r))
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
        if !the_model_and_runtime_are_here() {
            return;
        }
        let _serialised = one_at_a_time();
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

    /// **The mechanism, checked instead of assumed.**
    ///
    /// The whole fix rests on one claim: loading our verified copy by absolute
    /// path first puts it in this process's module list under the bare name the
    /// runtime imports, so the runtime binds to *that* file and never searches
    /// the disk. Asking Windows where `msvcp140.dll` was actually loaded from
    /// is the only way to know the claim holds — and it is worth knowing,
    /// because the failure it guards against, quietly binding to System32
    /// instead, is invisible on a machine that has both.
    ///
    /// Skipped where the copies have not been downloaded, like the end-to-end
    /// test above.
    #[test]
    fn a_preloaded_runtime_is_the_copy_that_gets_used() {
        use windows::Win32::System::LibraryLoader::{GetModuleFileNameW, GetModuleHandleW};

        let Ok(models) = models_dir() else {
            return;
        };
        let ours = models.join("msvcp140.dll");
        if !ours.is_file() {
            return;
        }
        let _serialised = one_at_a_time();
        preload_crt();

        let name: Vec<u16> = "msvcp140.dll"
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let handle = unsafe { GetModuleHandleW(PCWSTR(name.as_ptr())) }
            .expect("msvcp140.dll is in the process after a preload");
        let mut buffer = [0u16; 512];
        let written = unsafe { GetModuleFileNameW(Some(handle), &mut buffer) } as usize;
        let loaded = String::from_utf16_lossy(&buffer[..written]);

        assert_eq!(
            Path::new(&loaded),
            ours.as_path(),
            "the C++ runtime bound to {loaded} rather than to the verified copy"
        );
    }

    /// **The guard for a session that held 554 MB**, and the one that runs in
    /// the suite.
    ///
    /// The measurement it stands in for is `a_learned_matte_gives_its_memory_back`
    /// below, which reads this process's committed bytes — and therefore cannot
    /// run beside three hundred other tests that are all allocating. Reading the
    /// source instead is exact, free, and catches the thing actually worth
    /// catching: somebody simplifying the session options back to the default
    /// and quietly restoring half a gigabyte.
    #[test]
    fn the_learned_matte_runs_without_the_memory_arena() {
        let source = include_str!("photo.rs");
        assert!(
            source.contains("with_arena_allocator(false)"),
            "the CPU memory arena has to stay off: with it on, one cutout of a              64x64 picture commits 554 MB and keeps it for the life of the              session. See a_learned_matte_gives_its_memory_back for the figures."
        );
    }

    /// **The measurement behind that guard. Run it deliberately:**
    ///
    /// ```text
    /// cargo test --release --lib a_learned_matte_gives_its_memory_back -- --ignored --nocapture --test-threads=1
    /// ```
    ///
    /// `#[ignore]` because it reads **process-wide** committed bytes, and the
    /// rest of the suite allocates freely in parallel: run alongside everything
    /// else it reported 277 MB for a picture that costs 26. Serialising the
    /// tests in this file was not enough, because the noise is in the other
    /// three hundred. A number that is only true when nothing else is running
    /// belongs behind a flag rather than in a gate.
    ///
    /// Measured on 24 logical processors, release build:
    ///
    /// | | arena on | arena off |
    /// | --- | --- | --- |
    /// | committed after one cutout | **554 MB** | **26 MB** |
    /// | still committed afterwards | 554 MB | ~0 MB |
    /// | inference | 91 ms | 102 ms |
    #[test]
    #[ignore]
    fn a_learned_matte_gives_its_memory_back() {
    ///
    /// ONNX Runtime's CPU allocator is an arena by default: it takes memory as
    /// the graph runs and keeps it for the life of the session, which is right
    /// for a server and wrong for a tray application. Measured on this model it
    /// committed **554 MB on the first cutout and never gave it back** — and
    /// not in proportion to the picture, because u2netp always runs at 320x320.
    /// A 64x64 image cost the same 554 MB as a 19-megapixel one.
    ///
    /// That is what made photo mode fatal on a small machine rather than merely
    /// heavy. Rust aborts on a failed allocation, an abort never reaches the
    /// panic hook, and so it arrived as "the app just closes after a few goes"
    /// with nothing in the log to say why.
    ///
    /// The budget below is deliberately loose. The measured figure with the
    /// arena disabled is about 26 MB and with it enabled about 554 MB, so
    /// anything between the two catches the regression while leaving room for
    /// whatever else this process is doing — the reading is process-wide, and
    /// the rest of the suite is running alongside it.
        use windows::Win32::System::ProcessStatus::{
            GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX,
        };
        use windows::Win32::System::Threading::GetCurrentProcess;

        if !the_model_and_runtime_are_here() {
            return;
        }
        let _serialised = one_at_a_time();

        /// Committed private bytes. Working set is trimmed by Windows whenever
        /// it likes and would measure memory pressure rather than this code.
        fn committed() -> u64 {
            let mut ex = PROCESS_MEMORY_COUNTERS_EX::default();
            let size = std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32;
            unsafe {
                GetProcessMemoryInfo(
                    GetCurrentProcess(),
                    &mut ex as *mut _ as *mut PROCESS_MEMORY_COUNTERS,
                    size,
                )
            }
            .ok();
            ex.PrivateUsage as u64
        }

        const BUDGET: u64 = 200 * 1024 * 1024;

        // Start from no session, so what is measured is one session's whole
        // cost rather than one that some other test already paid for.
        release();
        let before = committed();

        // Small on purpose. The arena sizes itself to the graph, so a tiny
        // picture that costs half a gigabyte is the clearest possible statement
        // of what is wrong.
        let mut bitmap = Bitmap::new(64, 64);
        for y in 0..64u32 {
            for x in 0..64u32 {
                bitmap.set_pixel(x, y, [(x * 4) as u8, (y * 4) as u8, 120, 255]);
            }
        }
        let _ = remove_background_learned(&mut bitmap);

        let held = committed().saturating_sub(before);
        release();

        assert!(
            held < BUDGET,
            "one cutout of a 64x64 picture is holding {} MB. The CPU memory              arena is the usual reason — see the session options in `infer`.",
            held / (1024 * 1024)
        );
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
        if the_model_and_runtime_are_here() {
            return;
        }
        let mut bitmap = Bitmap::new(64, 64);
        let error = remove_background_learned(&mut bitmap)
            .expect_err("there is no model on this machine")
            .to_string();
        assert!(error.contains("photo mode") || error.contains("Photo mode"), "{error}");
    }

    /// **The regression test for what shipped in 1.22.0.**
    ///
    /// These filenames are not a labelling choice. Windows satisfies an import
    /// from the modules already loaded in the process, matched by base name, so
    /// a file downloaded as anything but `msvcp140.dll` answers nothing and the
    /// runtime fails to load exactly as it did before. The sets are per
    /// architecture and were read out of each published runtime's import table:
    /// `VCRUNTIME140_1.dll` is the x64 C++ exception helper, absent from the
    /// 32-bit redistributable entirely and not imported by the ARM64 build.
    #[test]
    fn the_cpp_runtime_is_carried_under_the_names_windows_matches() {
        let names: Vec<&str> = crt().iter().map(|a| a.name).collect();
        #[cfg(target_arch = "x86_64")]
        assert_eq!(
            names,
            [
                "vcruntime140.dll",
                "vcruntime140_1.dll",
                "msvcp140.dll",
                "msvcp140_1.dll"
            ]
        );
        #[cfg(any(target_arch = "aarch64", target_arch = "x86"))]
        assert_eq!(names, ["vcruntime140.dll", "msvcp140.dll", "msvcp140_1.dll"]);
    }

    /// **The C++ runtime is carried, not required.**
    ///
    /// Were it required, a release published before its assets went up — or an
    /// asset that later went missing — would break photo mode on every machine,
    /// including the great majority that already have those files in System32
    /// and never touch the copies. This is the assertion that stops the order
    /// of a release from being load-bearing.
    #[test]
    fn photo_mode_does_not_depend_on_the_carried_cpp_runtime() {
        let names: Vec<&str> = required().iter().map(|a| a.name).collect();
        for artifact in crt() {
            assert!(
                !names.contains(&artifact.name),
                "{} is being treated as required",
                artifact.name
            );
        }
        assert!(names.contains(&MODEL.name), "the model is required");
        assert_eq!(names.len(), if runtime().is_some() { 2 } else { 1 });
    }

    /// The list is a load order, and the order is a dependency order.
    ///
    /// A library loaded by absolute path still resolves **its own** imports
    /// through the ordinary search, so `msvcp140.dll` finds `vcruntime140.dll`
    /// only if that one is already in the process. Getting this backwards fails
    /// on precisely the machines the whole change exists for, and nowhere else.
    #[test]
    fn the_cpp_runtime_loads_in_dependency_order() {
        let names: Vec<&str> = crt().iter().map(|a| a.name).collect();
        let at = |n: &str| names.iter().position(|x| *x == n);
        if let (Some(runtime), Some(cpp)) = (at("vcruntime140.dll"), at("msvcp140.dll")) {
            assert!(runtime < cpp, "msvcp140.dll imports vcruntime140.dll");
        }
        if let (Some(cpp), Some(part)) = (at("msvcp140.dll"), at("msvcp140_1.dll")) {
            assert!(part > cpp, "msvcp140_1.dll imports msvcp140.dll");
        }
    }

    /// Every artifact is published under a name of its own.
    ///
    /// Three architectures need three different `msvcp140.dll` and one release
    /// cannot hold three assets called that, which is why the asset name and the
    /// filename are separate fields. A collision here is an architecture quietly
    /// downloading another architecture's library — which loads, and then fails
    /// in a way indistinguishable from a corrupted file.
    #[test]
    fn every_artifact_is_published_under_its_own_name() {
        let mut seen = std::collections::BTreeSet::new();
        for artifact in artifacts() {
            assert!(
                !artifact.sha256.is_empty(),
                "{} has no published checksum",
                artifact.name
            );
            assert!(artifact.bytes > 0, "{} has no published size", artifact.name);
            assert!(
                seen.insert(artifact.asset),
                "two artifacts are published as {}",
                artifact.asset
            );
        }
    }

    /// The trust boundary again, on the other road into it.
    ///
    /// `already_installed` exists to *skip* a download, so a build with no
    /// published checksum must be unable to skip one — otherwise the checksum
    /// that `verify` refuses to do without could be sidestepped by the file
    /// simply being there already.
    #[test]
    fn an_artifact_with_no_published_checksum_is_never_taken_from_disk() {
        let unpublished = Artifact {
            name: MODEL.name,
            asset: MODEL.asset,
            sha256: "",
            bytes: MODEL.bytes,
        };
        assert!(!already_installed(&unpublished));
    }

    /// **The trust boundary.** A library that gets loaded into this process
    /// must never arrive on the strength of its filename. A build with no
    /// published checksum has nothing to check against, and the safe answer is
    /// to do without photo mode entirely.
    #[test]
    fn an_artifact_with_no_published_checksum_is_refused() {
        let unpublished = Artifact {
            name: "onnxruntime-x64.dll",
            asset: "onnxruntime-x64.dll",
            sha256: "",
            bytes: 10,
        };
        assert!(verify(&unpublished, b"anything at all").is_err());
    }

    #[test]
    fn a_checksum_mismatch_is_refused() {
        let artifact = Artifact {
            name: "u2netp.onnx",
            asset: "u2netp.onnx",
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
