use std::time::{SystemTime, UNIX_EPOCH};

fn main() {
    // Stamped so About can answer "which build is this, exactly" without needing
    // a commit hash — local builds have no hash, and that is precisely when the
    // question gets asked.
    println!("cargo:rustc-env=CURSED_BUILD_DATE={}", build_date());
    stage_photo_runtime();
    tauri_build::build();
}

/// Collects this architecture's photo-mode files into one directory for the
/// bundler to ship.
///
/// Photo mode used to download these on request, which meant a feature most
/// people never found: it lived behind a button in Settings, and the ones who
/// would have wanted it were exactly the ones not reading Settings. It ships in
/// the installer now, so it is simply there.
///
/// Staged rather than listed directly in `tauri.conf.json` because the bundler's
/// resource list is fixed and the files are not: the C++ runtime and the ONNX
/// Runtime differ per architecture, and naming all three in the config would put
/// 50 MB of the wrong processor's DLLs into every installer. This runs before
/// the bundler and leaves exactly one architecture's worth behind.
///
/// The C++ runtime is **renamed on the way in** — `msvcp140-x64.dll` becomes
/// `msvcp140.dll`. That is not tidying: Windows satisfies an import from the
/// modules already loaded in the process, matched by base name, so a file
/// preloaded under any other name answers nothing.
fn stage_photo_runtime() {
    use std::path::{Path, PathBuf};

    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let source = manifest.join("..").join("assets").join("photo");
    let stage = manifest.join("photo-runtime");

    let arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    let arch_dir = match arch.as_str() {
        "x86_64" => Some("x64"),
        "aarch64" => Some("arm64"),
        "x86" => Some("x86"),
        _ => None,
    };

    // Rebuilt from scratch, so switching target architecture in the same tree
    // cannot leave the previous one's runtime behind to be shipped.
    let _ = std::fs::remove_dir_all(&stage);
    std::fs::create_dir_all(&stage).expect("photo staging directory");

    println!("cargo:rerun-if-changed={}", source.display());
    println!("cargo:rustc-env=CURSED_PHOTO_STAGE={}", stage.display());

    let Some(arch_dir) = arch_dir else {
        // An architecture with no runtime gets an empty directory rather than a
        // failed build: everything else about the app works there.
        println!("cargo:warning=photo mode has no runtime for target arch {arch}");
        return;
    };

    let copy = |from: PathBuf| {
        let Some(name) = from.file_name() else { return };
        if let Err(e) = std::fs::copy(&from, stage.join(name)) {
            panic!("could not stage {}: {e}", from.display());
        }
    };

    copy(source.join("u2netp.onnx"));
    let arch_source: &Path = &source.join(arch_dir);
    let entries = std::fs::read_dir(arch_source)
        .unwrap_or_else(|e| panic!("could not read {}: {e}", arch_source.display()));
    for entry in entries.filter_map(Result::ok) {
        if entry.path().is_file() {
            copy(entry.path());
        }
    }
}

/// `YYYY-MM-DD` in UTC, computed without pulling in a date crate.
///
/// Howard Hinnant's civil-from-days, which is exact for any date this will ever
/// see and short enough to read.
fn build_date() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    let days = secs.div_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };

    format!("{y:04}-{m:02}-{d:02}")
}
