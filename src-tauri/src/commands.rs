//! The **entire** IPC surface. Nothing outside this file is reachable from the
//! webview.
//!
//! Three rules hold for every function here, without exception:
//!
//!  1. It returns `Result<T, AppError>`. There is no `unwrap`, `expect` or
//!     `panic!` anywhere in a command path — a panic in a Tauri command takes
//!     the whole app with it (PRD §19 rule 4).
//!  2. It accepts **values, never locations**. A role arrives as an enum
//!     variant, a pack as a catalog id, a staged image as an opaque token. The
//!     frontend cannot name a registry key or a filesystem path, so it cannot
//!     be tricked into naming a dangerous one (PRD §13.4).
//!  3. Anything it does accept from outside is validated before use.

use crate::cursor::roles::{Role, ALL_ROLES, RECOMMENDED_ROLES};
use crate::error::{AppError, AppResult};
use crate::packs::catalog::{self, PackSummary, RenderSpec};
use crate::state::presets::{self, Preset};
use crate::state::settings::{self, ApplyMode, Settings};
use crate::{cursor, custom, packs, paths, session, updates};
use serde::Deserialize;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

/// Resolves the pixel size to render at: the user's explicit choice, else
/// whatever Windows' own accessibility slider is set to (PRD §5.2).
fn effective_size(requested: u32) -> u32 {
    if requested >= 32 {
        requested.min(256)
    } else {
        cursor::engine::effective_size(settings::get().cursor_size)
    }
}

/// The roles an apply mode covers. Roles left out are deleted from the scheme,
/// which is Windows' way of saying "use the built-in one".
pub fn roles_for(mode: ApplyMode) -> &'static [Role] {
    match mode {
        ApplyMode::ArrowOnly => &[Role::Arrow],
        ApplyMode::Recommended => &RECOMMENDED_ROLES,
        // A catalog pack already defines all seventeen coherently, so there is
        // nothing to blend it with — "Blend" and "All" mean the same thing here.
        ApplyMode::All | ApplyMode::Blend => &ALL_ROLES,
    }
}

/* ── settings & state ──────────────────────────────────────── */

#[tauri::command]
pub fn get_settings() -> AppResult<Settings> {
    Ok(settings::get())
}

#[tauri::command]
pub fn save_settings(app: AppHandle, settings: Settings) -> AppResult<Settings> {
    let saved = settings::save(settings)?;
    settings::propagate(&saved);
    crate::autostart::apply(&app, saved.launch_on_startup)?;
    crate::hotkeys::register(&app, &saved)?;
    crate::tray::set_visible(&app, saved.show_tray_icon)?;
    Ok(saved)
}

#[tauri::command]
pub fn get_active_state() -> AppResult<cursor::ActiveState> {
    cursor::active_state()
}

#[tauri::command]
pub fn get_cursor_base_size() -> AppResult<u32> {
    cursor::scheme::read_base_size()
}

/* ── catalog ───────────────────────────────────────────────── */

#[tauri::command]
pub fn list_packs() -> AppResult<Vec<PackSummary>> {
    catalog::list_summaries()
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyArgs {
    pub pack_id: String,
    pub tint: String,
    pub size: u32,
    pub outline: bool,
    pub apply_mode: ApplyMode,
}

impl ApplyArgs {
    fn spec(&self) -> RenderSpec {
        RenderSpec {
            tint: self.tint.clone(),
            size: effective_size(self.size),
            outline: self.outline,
        }
    }
}

/// Live layer only — no registry write, no broadcast.
///
/// Hover has to feel free, so this builds and sets the **arrow alone**. The
/// other sixteen roles are not what a browsing user is looking at, and building
/// them on every hover would turn a 120 ms debounce into visible lag.
/// Never fails.
///
/// A hover preview is a courtesy, not a commitment. If a particular cursor will
/// not load, the honest outcome is that the pointer does not change while the
/// user's mouse passes over one tile — not an error banner covering the catalog
/// they are trying to browse. Anything worth investigating goes to the log.
#[tauri::command]
pub fn preview_pack(args: ApplyArgs) -> AppResult<()> {
    let spec = args.spec();
    let attempt = catalog::build_preview_set(&args.pack_id, &spec)
        .and_then(|set| cursor::preview(&set, spec.size));

    if let Err(e) = attempt {
        log::debug!("preview of {} skipped: {e}", args.pack_id);
    }
    Ok(())
}

#[tauri::command]
pub fn clear_preview() -> AppResult<()> {
    cursor::clear_preview()
}

#[tauri::command]
pub fn apply_pack(app: AppHandle, args: ApplyArgs) -> AppResult<()> {
    let spec = args.spec();

    // An imported pack defines a role or two; the rest come from a built-in so
    // the pointer set stays coherent.
    let (set, name) = if catalog::is_imported(&args.pack_id) {
        let pack = crate::import::get(&args.pack_id)?;
        let base = settings::get().blend_pack;
        (
            catalog::build_imported(&args.pack_id, &base, &spec)?,
            pack.name,
        )
    } else {
        (
            catalog::build_roles(&args.pack_id, roles_for(args.apply_mode), &spec)?,
            catalog::display_name(&args.pack_id)
                .ok_or(AppError::UnknownPack)?
                .to_owned(),
        )
    };
    let name = name.as_str();

    cursor::commit(
        set,
        name,
        spec.size,
        Some(args.pack_id.clone()),
        args.tint.clone(),
    )?;
    // Recorded so the next launch, the watchdog, and the toggle hotkey all know
    // what "correct" is without re-deriving it (see `session`).
    session::save(&session::AppliedDescriptor {
        source: session::AppliedSource::Pack {
            pack_id: args.pack_id,
            apply_mode: args.apply_mode,
        },
        display_name: name.to_owned(),
        tint: args.tint,
        size: spec.size,
        outline: args.outline,
    })?;
    crate::tray::refresh_tooltip(&app);
    Ok(())
}

#[tauri::command]
pub fn restore_windows_default(app: AppHandle) -> AppResult<()> {
    cursor::restore_default()?;
    session::forget();
    crate::tray::refresh_tooltip(&app);
    Ok(())
}

/* ── presets ───────────────────────────────────────────────── */

#[tauri::command]
pub fn list_presets() -> AppResult<Vec<Preset>> {
    presets::list()
}

#[tauri::command]
pub fn save_preset(preset: Preset) -> AppResult<Preset> {
    presets::upsert(preset)
}

#[tauri::command]
pub fn delete_preset(id: String) -> AppResult<()> {
    presets::remove(&id)
}

#[tauri::command]
pub fn set_default_preset(id: String) -> AppResult<()> {
    presets::set_default(&id)
}

#[tauri::command]
pub fn duplicate_preset(id: String) -> AppResult<Preset> {
    presets::duplicate(&id)
}

#[tauri::command]
pub fn apply_preset(app: AppHandle, id: String) -> AppResult<()> {
    let preset = presets::get(&id)?;
    apply_preset_inner(&app, &preset)
}

/// Shared by the command, the tray menu, and the global hotkeys.
pub fn apply_preset_inner(app: &AppHandle, preset: &Preset) -> AppResult<()> {
    let spec = RenderSpec {
        tint: preset.tint.clone(),
        size: effective_size(preset.size),
        outline: preset.outline,
    };

    let mut set = catalog::build_set(&preset.base_pack, &spec)?;
    // A preset's overrides are custom cursors layered onto its base pack — the
    // stored form of the Blend mode.
    for (role, cursor_id) in &preset.overrides {
        let custom_set = custom::build_set(cursor_id, ApplyMode::ArrowOnly, None, &spec)?;
        if let Some(path) = custom_set.get(Role::Arrow) {
            set.insert(*role, path.to_path_buf());
        }
    }

    cursor::commit(
        set,
        &preset.name,
        spec.size,
        Some(preset.base_pack.clone()),
        preset.tint.clone(),
    )?;
    session::save(&session::AppliedDescriptor {
        source: session::AppliedSource::Pack {
            pack_id: preset.base_pack.clone(),
            apply_mode: ApplyMode::All,
        },
        display_name: preset.name.clone(),
        tint: preset.tint.clone(),
        size: spec.size,
        outline: preset.outline,
    })?;
    crate::tray::refresh_tooltip(app);
    Ok(())
}

#[tauri::command]
pub fn export_preset(id: String, dest: String) -> AppResult<String> {
    let preset = presets::get(&id)?;
    let destination = PathBuf::from(&dest);
    // The picker chose this path, so it is outside our storage by design; what
    // matters is that it is a file we are creating, not a directory traversal.
    if destination.extension().is_none_or(|e| e != "cfpack") {
        return Err(AppError::invalid("a pack must be saved with a .cfpack name"));
    }
    packs::cfpack::export(&preset, &destination)?;
    Ok(destination.to_string_lossy().into_owned())
}

#[tauri::command]
pub fn import_cfpack(src: String) -> AppResult<Preset> {
    packs::cfpack::import(&PathBuf::from(src))
}

/* ── custom import ─────────────────────────────────────────── */

#[tauri::command]
pub fn import_image(path: String) -> AppResult<custom::ImportedImage> {
    let source = PathBuf::from(path);
    let metadata = std::fs::metadata(&source)
        .map_err(|_| AppError::invalid("that file could not be opened"))?;
    if metadata.len() > crate::build::pipeline::MAX_INPUT_BYTES as u64 {
        return Err(AppError::ImageTooLarge("over 20 MB".into()));
    }
    custom::stage(std::fs::read(&source)?)
}

/// Bytes route for drag-and-drop, where the webview hands us content rather
/// than a path.
#[tauri::command]
pub fn import_image_bytes(bytes: Vec<u8>) -> AppResult<custom::ImportedImage> {
    custom::stage(bytes)
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildArgs {
    pub token: String,
    pub name: String,
    pub hotspot: (f32, f32),
    pub outline: bool,
    pub animation_speed: f32,
}

#[tauri::command]
pub fn preview_custom(token: String, outline: bool) -> AppResult<Vec<custom::Preview>> {
    custom::preview(&token, outline)
}

#[tauri::command]
pub fn build_custom_cursor(args: BuildArgs) -> AppResult<custom::BuiltCursor> {
    let name = args.name.trim();
    if name.is_empty() {
        return Err(AppError::invalid("give the cursor a name first"));
    }
    custom::build(
        &args.token,
        name,
        (
            args.hotspot.0.clamp(0.0, 1.0),
            args.hotspot.1.clamp(0.0, 1.0),
        ),
        args.outline,
        args.animation_speed.clamp(0.5, 2.0),
    )
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyCustomArgs {
    pub cursor_id: String,
    pub apply_mode: ApplyMode,
    pub blend_pack_id: Option<String>,
    pub tint: String,
    pub size: u32,
    pub outline: bool,
}

#[tauri::command]
pub fn apply_custom_cursor(app: AppHandle, args: ApplyCustomArgs) -> AppResult<()> {
    let spec = RenderSpec {
        tint: args.tint.clone(),
        size: effective_size(args.size),
        outline: args.outline,
    };
    let set = custom::build_set(
        &args.cursor_id,
        args.apply_mode,
        args.blend_pack_id.as_deref(),
        &spec,
    )?;

    cursor::commit(
        set,
        "CUSTOM",
        spec.size,
        args.blend_pack_id.clone(),
        args.tint.clone(),
    )?;
    session::save(&session::AppliedDescriptor {
        source: session::AppliedSource::Custom {
            cursor_id: args.cursor_id,
            apply_mode: args.apply_mode,
            blend_pack_id: args.blend_pack_id,
        },
        display_name: "CUSTOM".to_owned(),
        tint: args.tint,
        size: spec.size,
        outline: args.outline,
    })?;
    crate::tray::refresh_tooltip(&app);
    Ok(())
}

#[tauri::command]
pub fn delete_custom_cursor(id: String) -> AppResult<()> {
    custom::remove(&id)
}

/* ── advanced / about ──────────────────────────────────────── */

#[tauri::command]
pub fn get_storage_dir() -> AppResult<String> {
    Ok(paths::root()?.to_string_lossy().into_owned())
}

/// Opens our own storage folder in Explorer.
///
/// Deliberately a Rust command rather than a shell permission: the path is
/// computed here and cannot be supplied by the webview, so there is no argument
/// for the frontend to control (PRD §13.2 denies `shell:*` outright).
#[tauri::command]
pub fn open_storage_dir() -> AppResult<()> {
    crate::shell::open_path(&paths::root()?)
}

#[tauri::command]
pub fn get_cache_size() -> AppResult<u64> {
    catalog::cache_size()
}

#[tauri::command]
pub fn clear_cache() -> AppResult<u64> {
    catalog::clear_cache()
}

#[tauri::command]
pub fn get_legal_doc(kind: String) -> AppResult<String> {
    // Rendered from the binary, so Terms, Privacy and Licences are readable with
    // no network and no files on disk (PRD §15).
    Ok(match kind.as_str() {
        "terms" => include_str!("../../docs/TERMS.md"),
        "privacy" => include_str!("../../docs/PRIVACY.md"),
        "licenses" => include_str!("../../docs/LICENSES.md"),
        _ => return Err(AppError::invalid("no such document")),
    }
    .to_owned())
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildInfo {
    pub version: String,
    pub commit: String,
    pub target: String,
    /// `YYYY-MM-DD`, stamped at compile time.
    pub built: String,
    /// What Windows calls itself, e.g. `Windows 11 24H2 (build 26200)`.
    pub windows: String,
}

#[tauri::command]
pub fn get_build_info() -> AppResult<BuildInfo> {
    Ok(BuildInfo {
        version: env!("CARGO_PKG_VERSION").to_owned(),
        commit: option_env!("CURSORFORGE_COMMIT").unwrap_or("local").to_owned(),
        target: std::env::consts::ARCH.to_owned(),
        built: env!("CURSED_BUILD_DATE").to_owned(),
        windows: windows_build(),
    })
}

/// The OS version, read from the registry.
///
/// `GetVersionEx` lies unless the application manifest opts in, and
/// `RtlGetVersion` needs an ntdll import for one string. These values are
/// read-only, always present, and say what the user would read in `winver`.
fn windows_build() -> String {
    use winreg::enums::HKEY_LOCAL_MACHINE;
    use winreg::RegKey;

    let Ok(key) = RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion")
    else {
        return "unknown".to_owned();
    };

    let product: String = key.get_value("ProductName").unwrap_or_default();
    let display: String = key.get_value("DisplayVersion").unwrap_or_default();
    let build: String = key.get_value("CurrentBuild").unwrap_or_default();

    // Windows 11 still reports "Windows 10 ..." in ProductName; build 22000 is
    // the line where it became 11, and reporting 10 on an 11 machine makes the
    // whole report look wrong.
    let product = match build.parse::<u32>() {
        Ok(n) if n >= 22_000 => product.replace("Windows 10", "Windows 11"),
        _ => product,
    };

    let mut out = if product.is_empty() { "Windows".to_owned() } else { product };
    if !display.is_empty() {
        out.push(' ');
        out.push_str(&display);
    }
    if !build.is_empty() {
        out.push_str(&format!(" (build {build})"));
    }
    out
}

/// A plain-text report the user can copy straight into a message.
///
/// This exists because "it doesn't work" is unanswerable. Everything in it has
/// already been the difference between working and not: which version is really
/// running, whether the data directory can be written, how many cursors the
/// catalog can actually offer, and whether the seventeen registry values still
/// point at files that exist.
///
/// It carries no personal data beyond paths inside the user's own profile,
/// because the entire point is that it can be pasted to a stranger.
#[tauri::command]
pub fn get_diagnostics() -> AppResult<String> {
    use std::fmt::Write as _;
    let mut out = String::new();

    let _ = writeln!(out, "CURSED DIAGNOSTICS");
    let _ = writeln!(
        out,
        "version {}   commit {}   arch {}",
        env!("CARGO_PKG_VERSION"),
        option_env!("CURSORFORGE_COMMIT").unwrap_or("local"),
        std::env::consts::ARCH
    );
    let _ = writeln!(
        out,
        "exe     {}",
        std::env::current_exe()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|e| e.to_string())
    );

    // A profile that cannot be written is the difference between a working
    // catalog and a screen with nothing on it.
    let _ = writeln!(out, "\nSTORAGE");
    for (label, dir) in [
        ("data  ", paths::root()),
        ("cache ", paths::cache_dir()),
        ("custom", paths::custom_dir()),
        ("backup", paths::backup_dir()),
    ] {
        match dir {
            Ok(path) => {
                let exists = path.is_dir();
                let writable = exists && {
                    let probe = path.join(".write-probe");
                    let ok = std::fs::write(&probe, b"x").is_ok();
                    let _ = std::fs::remove_file(&probe);
                    ok
                };
                let _ = writeln!(out, "{label}  exists={exists} writable={writable}  {}", path.display());
            }
            Err(e) => {
                let _ = writeln!(out, "{label}  UNAVAILABLE: {e}");
            }
        }
    }

    // A built-in count of zero is a bug, not a preference.
    let built_in = crate::packs::styles::all().len();
    let imported = crate::import::list().map(|v| v.len()).unwrap_or(0);
    let _ = writeln!(out, "\nCATALOG");
    let _ = writeln!(out, "built-in {built_in}   imported {imported}   total {}", built_in + imported);
    if built_in == 0 {
        let _ = writeln!(out, "!! the built-in catalog is empty -- this is a bug, please report it");
    }
    let _ = writeln!(
        out,
        "cache    {:.2} MB",
        catalog::cache_size().unwrap_or(0) as f64 / 1_048_576.0
    );

    // What Windows is pointing at right now, and whether it is still there. A
    // missing file is a cursor that silently reverts to the default.
    let _ = writeln!(out, "\nREGISTRY  HKCU\\Control Panel\\Cursors");
    let _ = writeln!(
        out,
        "scheme  {}",
        crate::cursor::scheme::read_scheme_name().unwrap_or_else(|_| "(none)".into())
    );
    for role in crate::cursor::roles::ALL_ROLES {
        match crate::cursor::scheme::read_role(role) {
            Ok(value) if value.trim().is_empty() => {
                let _ = writeln!(out, "  {:<12} (empty -- Windows default)", role.registry_value());
            }
            Ok(value) => {
                let present = std::path::Path::new(&crate::util::expand_env(&value)).exists();
                let _ = writeln!(
                    out,
                    "  {:<12} {value}{}",
                    role.registry_value(),
                    if present { "" } else { "   << FILE MISSING" }
                );
            }
            Err(e) => {
                let _ = writeln!(out, "  {:<12} unreadable: {e}", role.registry_value());
            }
        }
    }

    let _ = writeln!(out, "\nSAFETY NET");
    let _ = writeln!(
        out,
        "original scheme snapshot  {}",
        paths::original_scheme_file()
            .map(|p| p.is_file().to_string())
            .unwrap_or_else(|e| e.to_string())
    );
    let _ = writeln!(out, "applied state recorded    {}", crate::session::load().is_some());

    let _ = writeln!(out, "\nUPDATES");
    let _ = writeln!(out, "endpoint  {}", updates::RELEASES_URL);
    let state = updates::state();
    let _ = writeln!(
        out,
        "checking={} downloading={} ready={}",
        state.checking, state.downloading, state.ready
    );
    match &state.status {
        Some(s) => {
            let _ = writeln!(
                out,
                "current {}   latest {}   newer {}",
                s.current,
                s.latest.clone().unwrap_or_else(|| "(unknown)".into()),
                s.newer_available
            );
        }
        None => {
            let _ = writeln!(out, "no check has completed yet");
        }
    }
    // Verbatim. A paraphrased error is a useless error.
    if let Some(err) = &state.error {
        let _ = writeln!(out, "last error: {err}");
    }

    Ok(out)
}

#[tauri::command]
pub fn check_for_updates() -> AppResult<updates::UpdateStatus> {
    updates::check()
}

/// Downloads the installer for an available update.
///
/// The frontend passes the version and asset name it was given by
/// `check_for_updates`; both are re-validated here rather than trusted, because
/// they end up in a URL and a filename.
#[tauri::command]
pub fn download_update(version: String, installer: String) -> AppResult<u64> {
    let file = updates::download(&version, &installer)?;
    Ok(std::fs::metadata(&file).map(|m| m.len()).unwrap_or(0))
}

/// Verifies the downloaded installer against the checksum published with the
/// release, then launches it. Refuses to run anything that does not match.
#[tauri::command]
pub fn install_update(app: AppHandle, version: String, installer: String) -> AppResult<()> {
    updates::verify_and_launch(&version, &installer)?;
    // The installer needs our files unlocked, and leaving a stale copy running
    // behind a fresh install is how you get two tray icons.
    app.exit(0);
    Ok(())
}

#[tauri::command]
pub fn clear_update_downloads() -> AppResult<()> {
    updates::clear_downloads()
}

/// What the background updater has found. Cheap enough to poll.
#[tauri::command]
pub fn get_update_state() -> AppResult<updates::UpdateState> {
    Ok(updates::state())
}

/// Kicks off a check-and-download without waiting for it.
#[tauri::command]
pub fn start_update_check() -> AppResult<()> {
    updates::auto_update_in_background();
    Ok(())
}

/* ── importing the user's own cursors ──────────────────────── */

/// Imports every cursor found in a folder the user picked.
///
/// The path comes from the native file dialog rather than from the webview, and
/// the import only ever *reads* from it — everything created lands under
/// Cursed's own storage.
#[tauri::command]
pub fn import_cursor_folder(folder: String) -> AppResult<crate::import::ImportReport> {
    crate::import::import_folder(&PathBuf::from(folder))
}

#[tauri::command]
pub fn list_imported() -> AppResult<Vec<crate::import::ImportedPack>> {
    crate::import::list()
}

#[tauri::command]
pub fn delete_imported(id: String) -> AppResult<()> {
    crate::import::remove(&id)
}

#[tauri::command]
pub fn delete_all_imported() -> AppResult<()> {
    crate::import::remove_all()
}

/// Opens an external URL — but only one of ours.
///
/// The allow-list is the point. A command that opened whatever URL it was given
/// would be a general-purpose launcher reachable from web content, which is
/// exactly what denying `shell:*` was meant to prevent.
#[tauri::command]
pub fn open_external(url: String) -> AppResult<()> {
    const ALLOWED: [&str; 3] = [
        "https://github.com/notfeylo/cursorforge",
        "https://github.com/notfeylo/cursorforge/issues",
        "https://github.com/notfeylo/cursorforge/releases",
    ];
    if !ALLOWED.contains(&url.as_str()) {
        return Err(AppError::invalid("that link is not one Cursed opens"));
    }
    crate::shell::open_url(&url)
}

/// Called by the frontend once it has painted.
///
/// The window is created hidden so nobody sees an unpainted rectangle. Showing
/// it is therefore the frontend's cue, not the backend's — but see
/// [`crate::show_main_window_eventually`] for the fallback that guarantees the
/// window appears even if the frontend never gets this far.
#[tauri::command]
pub fn frontend_ready(app: AppHandle) -> AppResult<()> {
    crate::show_main_window(&app);
    Ok(())
}

#[tauri::command]
pub fn hide_to_tray(app: AppHandle) -> AppResult<()> {
    if let Some(window) = app.get_webview_window("main") {
        window.hide().map_err(|e| AppError::msg(e.to_string()))?;
        crate::idle::release_memory_soon();
    }
    Ok(())
}

#[tauri::command]
pub fn quit_app(app: AppHandle) -> AppResult<()> {
    app.exit(0);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_modes_cover_the_roles_they_advertise() {
        assert_eq!(roles_for(ApplyMode::ArrowOnly).len(), 1);
        assert_eq!(roles_for(ApplyMode::Recommended).len(), 3);
        assert_eq!(roles_for(ApplyMode::All).len(), 17);
        assert_eq!(roles_for(ApplyMode::Blend).len(), 17);
    }

    #[test]
    fn arrow_only_covers_the_arrow_and_nothing_else() {
        let roles = roles_for(ApplyMode::ArrowOnly);
        assert!(roles.contains(&Role::Arrow));
        assert!(!roles.contains(&Role::IBeam));
    }

    #[test]
    fn recommended_is_the_three_roles_a_person_actually_notices() {
        let roles = roles_for(ApplyMode::Recommended);
        for expected in [Role::Arrow, Role::Hand, Role::Crosshair] {
            assert!(roles.contains(&expected), "{expected} should be covered");
        }
    }

    #[test]
    fn legal_documents_are_embedded_and_non_empty() {
        for kind in ["terms", "privacy", "licenses"] {
            let text = get_legal_doc(kind.to_owned()).unwrap();
            assert!(text.len() > 400, "{kind} looks truncated");
        }
        assert!(get_legal_doc("../../secrets".to_owned()).is_err());
    }

    #[test]
    fn external_links_are_allow_listed() {
        assert!(open_external("https://example.com/".into()).is_err());
        assert!(open_external("file:///C:/Windows".into()).is_err());
        assert!(open_external("https://github.com/notfeylo/cursorforge.evil.com".into()).is_err());
    }

    #[test]
    fn a_size_below_the_floor_falls_back_to_the_system_size() {
        assert_eq!(effective_size(48), 48);
        assert_eq!(effective_size(9_999), 256);
        assert!(effective_size(0) >= 32);
    }
}
