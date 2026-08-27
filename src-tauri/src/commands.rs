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
    // Zero means "not specified", and nothing else does.
    //
    // This used to treat anything under 32 as unspecified, which worked only
    // while 32 was the smallest size on offer. Once the range opened to 10 px
    // that sentinel started swallowing real requests: asking for 10 or 16
    // silently fell back to the settings value, so the size control appeared to
    // do nothing at the small end. A magic threshold that happens to sit at the
    // bottom of the old range is not a sentinel, it is a collision waiting for
    // the range to change.
    if requested == 0 {
        return cursor::engine::effective_size(settings::get().cursor_size);
    }
    // Clamped **and snapped**, the same as the other two ways a size reaches the
    // renderer.
    //
    // 1.25.0 snapped `Settings::sanitised` and `engine::effective_size` and
    // missed this one, which is the path the UI actually uses — every apply and
    // every hover arrives here with an explicit number. A size between two rungs
    // is one Windows has to stretch a cursor into, and it did not stop being
    // that because it came from the front end rather than from disk.
    crate::build::pipeline::nearest_size(requested.clamp(
        crate::state::settings::MIN_CURSOR_PX,
        crate::state::settings::MAX_CURSOR_PX,
    ))
}

#[cfg(test)]
mod size_tests {
    use super::effective_size;
    use crate::state::settings::{MAX_CURSOR_PX, MIN_CURSOR_PX};

    /// The regression: every size below 32 was discarded, so the small end of
    /// the slider did nothing at all.
    ///
    /// Still the point, now stated against the ladder: a small request must come
    /// back small. It no longer has to come back *identical*, because a size
    /// between two rungs is one Windows would have to stretch a cursor into and
    /// is snapped to the nearest one — but snapping to 10 or 16 is honouring the
    /// request, and falling back to the settings value is not.
    #[test]
    fn a_small_size_is_honoured_rather_than_swallowed() {
        for requested in [MIN_CURSOR_PX, 12, 16, 24, 31] {
            let got = effective_size(requested);
            assert!(
                got <= 32,
                "{requested} px came back as {got} px, which is the settings value, not the request"
            );
            assert!(
                crate::build::cur_writer::TARGET_SIZES.contains(&got),
                "{requested} px snapped to {got} px, which no cursor file carries"
            );
        }
    }

    /// Every size the front end can send lands on a rung. This is the entry
    /// point 1.25.0 missed: it snapped the settings file and the value inherited
    /// from Windows, and left the one the UI actually calls.
    #[test]
    fn every_size_the_front_end_can_send_lands_on_a_rung() {
        for requested in MIN_CURSOR_PX..=MAX_CURSOR_PX {
            let got = effective_size(requested);
            assert!(
                crate::build::cur_writer::TARGET_SIZES.contains(&got),
                "{requested} px became {got} px, which no cursor file carries"
            );
        }
    }

    #[test]
    fn the_range_is_enforced_at_both_ends() {
        assert_eq!(effective_size(1), MIN_CURSOR_PX);
        assert_eq!(effective_size(9_999), MAX_CURSOR_PX);
        assert_eq!(effective_size(64), 64);
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
    let before = settings::get();
    let saved = settings::save(settings)?;
    settings::propagate(&saved);
    crate::autostart::apply(&app, saved.launch_on_startup)?;
    crate::hotkeys::register(&app, &saved)?;
    crate::tray::set_visible(&app, saved.show_tray_icon)?;

    // Appearance settings describe the cursor that is already on screen, so
    // changing one has to change it. Otherwise the setting saves, nothing looks
    // different, and the only way to see the new colour is to go and pick the
    // same cursor again — which is the app asking the user to finish the job.
    //
    // Only these three, and only when they actually moved: rebuilding a
    // seventeen-role scheme on every toggle of "start minimised" would be work
    // nobody asked for.
    let appearance_changed = before.tint != saved.tint
        || before.outline != saved.outline
        || before.cursor_size != saved.cursor_size
        // This one changes the hand and the I-beam rather than the pointer, so
        // it is easy to leave out and impossible to notice missing: the toggle
        // moves, the setting saves, and the cursor on screen does not change
        // until something else happens to trigger a rebuild.
        || before.scale_all_roles != saved.scale_all_roles;

    if appearance_changed {
        let size = effective_size(saved.cursor_size.unwrap_or(0));
        match session::reapply_with_appearance(&saved.tint, size, saved.outline) {
            Ok(true) => crate::tray::refresh_tooltip(&app),
            Ok(false) => {}
            // A failure here must not lose the setting the user just saved.
            Err(e) => log::warn!("settings saved, but the cursor could not be redrawn: {e}"),
        }
    }

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
    let attempt = build_set_for(&args, &spec).and_then(|(set, _)| cursor::preview(&set, spec.size));

    if let Err(e) = attempt {
        log::debug!("preview of {} skipped: {e}", args.pack_id);
    }
    Ok(())
}

#[tauri::command]
pub fn clear_preview() -> AppResult<()> {
    cursor::clear_preview()
}

/// The set a pack applies to, built once and used by both apply and preview.
///
/// **Shared on purpose.** Preview used to build a cut-down set — the arrow
/// alone — on the theory that hovering had to be cheap. The result was that what
/// you saw while browsing was not what you would get, and on one machine the
/// difference was visible as a blurry pointer that lasted exactly as long as the
/// app held focus and corrected itself the moment it did not.
///
/// A preview that looks worse than the result is worse than no preview, and the
/// speed it was buying is not there: a full seventeen-role pack renders in about
/// 50 ms at all ten rungs, measured off the cache directory's own timestamps,
/// against a 120 ms hover debounce. There was nothing to save.
fn build_set_for(
    args: &ApplyArgs,
    spec: &RenderSpec,
) -> AppResult<(crate::cursor::scheme::CursorSet, String)> {
    // An imported pack defines a role or two; the rest come from a built-in so
    // the pointer set stays coherent. That blend is also what fills in an
    // imported pack with no arrow of its own — which is what the preview path
    // used to paper over by installing whichever role happened to sort first as
    // the pointer.
    if catalog::is_imported(&args.pack_id) {
        let pack = crate::import::get(&args.pack_id)?;
        let base = settings::get().blend_pack;
        return Ok((catalog::build_imported(&args.pack_id, &base, spec)?, pack.name));
    }
    Ok((
        catalog::build_roles(&args.pack_id, roles_for(args.apply_mode), spec)?,
        catalog::display_name(&args.pack_id)
            .ok_or(AppError::UnknownPack)?
            .to_owned(),
    ))
}

#[tauri::command]
pub fn apply_pack(app: AppHandle, args: ApplyArgs) -> AppResult<()> {
    let spec = args.spec();
    let (set, name) = build_set_for(&args, &spec)?;
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

/// What "restore" is actually able to give back on this machine.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemeStatus {
    /// True when this machine's pre-Cursed pointers were destroyed before they
    /// could be recorded — see `cursor::restore::Provenance::Lost`.
    pub original_lost: bool,
    /// Whether the user has already been told.
    pub acknowledged: bool,
}

/// Whether the machine's original pointer scheme survived.
///
/// Cheap: one file read, already cached by the OS, and the Settings screen asks
/// for it once when it opens.
#[tauri::command]
pub fn get_scheme_status() -> AppResult<SchemeStatus> {
    Ok(SchemeStatus {
        original_lost: !cursor::restore::provenance().is_real(),
        acknowledged: settings::get().scheme_loss_acknowledged,
    })
}

/// Dismisses the lost-scheme notice, permanently.
///
/// Its own command rather than a `save_settings` round trip: the frontend would
/// otherwise have to send back the entire settings object to change one flag,
/// and a stale copy of that object — one loaded before the user changed
/// something in another panel — would quietly revert their change.
#[tauri::command]
pub fn acknowledge_scheme_loss() -> AppResult<()> {
    let mut current = settings::get();
    if current.scheme_loss_acknowledged {
        return Ok(());
    }
    current.scheme_loss_acknowledged = true;
    settings::save(current)?;
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

/* ── the matte editor ──────────────────────────────────────── */

/// What the editor needs to open: the image as it arrived, and where to start.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MatteSession {
    /// The staged image with **no background removal applied**, so the editor
    /// has something to reset to and something to key from.
    pub original_data_uri: String,
    pub width: u32,
    pub height: u32,
    /// The tolerance the automatic path would have chosen for this image.
    pub suggested_tolerance: i32,
    pub min_tolerance: i32,
    pub max_tolerance: i32,
}

/// Opens the editor on a staged image.
#[tauri::command]
pub fn open_matte_editor(token: String) -> AppResult<MatteSession> {
    let original = custom::staged_original(&token)?;
    let (min_tolerance, max_tolerance) = crate::build::matte::tolerance_range();
    Ok(MatteSession {
        suggested_tolerance: crate::build::matte::suggested_tolerance(&original),
        width: original.width,
        height: original.height,
        original_data_uri: original.to_png_data_uri()?,
        min_tolerance,
        max_tolerance,
    })
}

/// Keys the staged image at one tolerance and hands back a preview.
///
/// The live half of the slider. Runs the real matte rather than anything the
/// frontend approximates, so what the user is dragging against is what they
/// will get — an editor that previews with a different algorithm than it
/// applies is worse than no preview.
#[tauri::command]
pub fn preview_matte(token: String, tolerance: i32) -> AppResult<String> {
    let mut image = custom::staged_original(&token)?;
    crate::build::matte::remove_background_at(&mut image, tolerance);
    image.to_png_data_uri()
}


/* ── photo mode ────────────────────────────────────────────── */

/// What photo mode would cost and whether it is installed.
///
/// Cheap and side-effect free: it reads two file sizes. Nothing here downloads,
/// and nothing here runs at launch.
#[tauri::command]
pub fn get_photo_status() -> AppResult<crate::photo::PhotoStatus> {
    Ok(crate::photo::status())
}

/// Downloads the model and runtime, after the user has been told the size.
///
/// Runs on its own thread and reports progress through the shared state, so the
/// window keeps painting through a twenty-megabyte download.
#[tauri::command]
pub fn install_photo_mode() -> AppResult<()> {
    if !crate::photo::available() {
        return Err(AppError::invalid(crate::photo::UNAVAILABLE));
    }
    std::thread::Builder::new()
        .name("cursed-photo-install".into())
        .spawn(|| {
            let result = crate::photo::install(&mut |got, total| {
                crate::photo::report_progress(got, total);
            });
            crate::photo::finish(result);
        })
        .map_err(|e| AppError::msg(format!("the download could not be started: {e}")))?;
    Ok(())
}

/// How the download is going. Cheap enough to poll.
#[tauri::command]
pub fn get_photo_progress() -> AppResult<crate::photo::Progress> {
    Ok(crate::photo::progress())
}

/// Stops a download in flight. The partial file is discarded.
#[tauri::command]
pub fn cancel_photo_install() -> AppResult<()> {
    crate::photo::cancel();
    Ok(())
}

/// Deletes the model and runtime, and reports the space reclaimed.
#[tauri::command]
pub fn remove_photo_mode() -> AppResult<u64> {
    crate::photo::remove()
}

/* ── backup and restore ────────────────────────────────────── */

/// The filename to offer in the save dialog. Dated, so a second backup does not
/// land on top of the first.
#[tauri::command]
pub fn suggested_backup_name() -> AppResult<String> {
    Ok(crate::backup::suggested_name())
}

/// Everything the user has made, in one zip they can put somewhere else.
#[tauri::command]
pub fn export_all_data(dest: String) -> AppResult<crate::backup::BackupReport> {
    crate::backup::export(&PathBuf::from(dest))
}

/// Restores a backup over the data directory, merging rather than replacing.
#[tauri::command]
pub fn import_all_data(src: String) -> AppResult<crate::backup::RestoreReport> {
    crate::backup::import(&PathBuf::from(src))
}

/* ── custom import ─────────────────────────────────────────── */

#[tauri::command]
pub fn import_image(
    path: String,
    cut: Option<crate::build::pipeline::Cut>,
) -> AppResult<custom::ImportedImage> {
    let source = PathBuf::from(path);
    let metadata = std::fs::metadata(&source)
        .map_err(|_| AppError::invalid("that file could not be opened"))?;
    if metadata.len() > crate::build::pipeline::MAX_INPUT_BYTES as u64 {
        return Err(AppError::ImageTooLarge("over 20 MB".into()));
    }
    custom::stage_with(std::fs::read(&source)?, cut.unwrap_or_default())
}

/// Bytes route for drag-and-drop, where the webview hands us content rather
/// than a path.
#[tauri::command]
pub fn import_image_bytes(
    bytes: Vec<u8>,
    cut: Option<crate::build::pipeline::Cut>,
) -> AppResult<custom::ImportedImage> {
    custom::stage_with(bytes, cut.unwrap_or_default())
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildArgs {
    pub token: String,
    pub name: String,
    pub hotspot: (f32, f32),
    pub outline: bool,
    pub animation_speed: f32,
    /// Flip, rotate, invert and crop, chosen on the preview.
    #[serde(default)]
    pub transform: crate::build::pipeline::Transform,
    /// A second staged image for the link/hover cursor, if the user added one.
    #[serde(default)]
    pub hand_token: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdjustArgs {
    pub token: String,
    /// What the artwork has already been turned by. Round-tripped rather than
    /// held here: two windows on one staged image would otherwise fight.
    #[serde(default)]
    pub transform: crate::build::pipeline::Transform,
    pub turn: crate::build::pipeline::Turn,
    pub hotspot: (f32, f32),
    pub outline: bool,
}

/// One press of rotate or flip on a staged image.
#[tauri::command]
pub fn adjust_custom(args: AdjustArgs) -> AppResult<custom::Adjusted> {
    custom::adjust(
        &args.token,
        &args.transform,
        args.turn,
        (
            args.hotspot.0.clamp(0.0, 1.0),
            args.hotspot.1.clamp(0.0, 1.0),
        ),
        args.outline,
    )
}

#[tauri::command]
pub fn preview_custom(
    token: String,
    outline: bool,
    transform: Option<crate::build::pipeline::Transform>,
) -> AppResult<Vec<custom::Preview>> {
    custom::preview(&token, outline, &transform.unwrap_or_default())
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
        &args.transform,
        args.hand_token.as_deref(),
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

    // The name the user typed, not the word "CUSTOM".
    //
    // Every custom cursor was committed under the same label, so the home
    // screen said "USING CUSTOM" whether you had made one cursor or thirty —
    // which is exactly the case where knowing which one is on is worth most.
    let display_name = custom::list()
        .unwrap_or_default()
        .into_iter()
        .find(|c| c.id == args.cursor_id)
        .map(|c| c.name)
        .unwrap_or_else(|| "CUSTOM".to_owned());

    cursor::commit(
        set,
        &display_name,
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
        display_name: display_name.clone(),
        tint: args.tint,
        size: spec.size,
        outline: args.outline,
    })?;
    crate::tray::refresh_tooltip(&app);
    Ok(())
}

/// Every custom cursor the user has kept, with a tile image for each.
///
/// Custom used to be a one-shot builder: make a cursor, apply it, and it was
/// gone from view even though the files were still on disk. Listing them turns
/// that screen into a library of the user's own work, which is what it always
/// should have been.
#[tauri::command]
pub fn list_custom_cursors() -> AppResult<Vec<CustomEntry>> {
    Ok(custom::list()?
        .into_iter()
        .map(|c| {
            let preview = custom::thumbnail(&c.id).unwrap_or_default();
            CustomEntry {
                id: c.id,
                name: c.name,
                animated: c.animated,
                created: c.created,
                preview,
            }
        })
        .collect())
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomEntry {
    pub id: String,
    pub name: String,
    pub animated: bool,
    pub created: String,
    /// Empty when the artwork could not be read; the UI shows a placeholder.
    pub preview: String,
}

/// Deletes a custom cursor, and puts the pointer back if that was the one in use.
///
/// Deleting the applied cursor used to leave `HKCU\Control Panel\Cursors` naming
/// files that no longer existed. Windows does not complain about that — it
/// quietly falls back to its own arrow — so nothing looked wrong until the user
/// tried to change something.
///
/// The failure it produced was baffling on purpose-built evidence: the pointer
/// stopped responding to the size control, while the hand and the I-beam kept
/// resizing, because those still pointed at real files. That reads as "sizing is
/// broken for the main cursor" and sends you looking at the sizing code, which
/// is fine. The stored descriptor was the thing that had gone stale.
#[tauri::command]
pub fn delete_custom_cursor(id: String) -> AppResult<()> {
    let was_applied = session::applied_custom_id().is_some_and(|applied| applied == id);
    custom::remove(&id)?;

    if was_applied {
        // Restore rather than re-apply: the artwork this pointer was made from
        // is the thing that was just deleted, so there is nothing to go back to.
        log::info!("the applied custom cursor was deleted, so the pointer goes back to Windows");
        cursor::restore_default()?;
        session::forget();
    }
    Ok(())
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

/// The changelog, compiled in.
///
/// From the binary rather than from GitHub, and that is the point: this is read
/// on the launch *after* an update, to tell the user what the version they are
/// now running changed. Fetching it would make "what's new" depend on the
/// network being up at that moment, and on the release notes not having been
/// edited since — neither of which is true of the thing they just installed.
const CHANGELOG: &str = include_str!("../../CHANGELOG.md");

/// The changelog section for one version, without its heading.
///
/// Returns `Ok(None)` rather than an error when there is no entry: a build made
/// between releases has a version no section names, and that is an ordinary
/// state, not a failure. The panel simply shows nothing.
#[tauri::command]
pub fn get_release_notes(version: String) -> AppResult<Option<String>> {
    Ok(release_notes_for(CHANGELOG, &version))
}

/// Pulls one `## <version> — <date>` section out of the changelog.
///
/// Matched on the version *token* rather than on the whole heading, because the
/// heading carries a date for a released version and the word "unreleased" for
/// the one being worked on, and both should find their section.
fn release_notes_for(changelog: &str, version: &str) -> Option<String> {
    let wanted = version.trim().trim_start_matches('v');
    if wanted.is_empty() {
        return None;
    }

    let mut lines = changelog.lines();
    // Find the heading whose first word after `## ` is the version.
    lines.find(|line| {
        line.strip_prefix("## ")
            .and_then(|rest| rest.split_whitespace().next())
            .is_some_and(|token| token == wanted)
    })?;

    let body: Vec<&str> = lines
        .take_while(|line| !line.starts_with("## "))
        // A `---` rule separates entries and is furniture, not content.
        .filter(|line| line.trim() != "---")
        .collect();

    let text = body.join("\n").trim().to_owned();
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
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
        commit: option_env!("CURSED_COMMIT").unwrap_or("local").to_owned(),
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
        option_env!("CURSED_COMMIT").unwrap_or("local"),
        std::env::consts::ARCH
    );
    let _ = writeln!(
        out,
        "exe     {}",
        std::env::current_exe()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|e| e.to_string())
    );

    // What this build will and will not accept as an update. An unsigned build
    // and a signed one look identical from the outside right up until it
    // matters, and "my update was refused" is unanswerable without this line.
    let _ = writeln!(
        out,
        "updates {}",
        if crate::signing::enforced() {
            "signature + checksum"
        } else {
            "checksum only (no signing key in this build)"
        }
    );

    // Which of the two installs this is, and whether it is the one defending
    // the scheme. On almost every machine there is one channel and these are two
    // dull lines; on the machines where there are two, they are the difference
    // between a bug and two apps disagreeing.
    let _ = writeln!(
        out,
        "channel {} ({})",
        crate::channel::PRODUCT_NAME,
        crate::channel::NAME
    );
    let _ = writeln!(
        out,
        "pointer {}",
        if crate::cursor::crosschannel::owns_pointer() {
            "held by this process".to_owned()
        } else {
            match crate::cursor::crosschannel::holder() {
                Some(other) => format!("held by {} (pid {})", other.product_name, other.pid),
                None => "not claimed".to_owned(),
            }
        }
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
    // Through the audit rather than reading the values directly, so the report
    // says what each file *is* and not only whether it exists. A `.cur` with the
    // wrong `idType`, or an icon saved under a cursor's name, exists perfectly
    // and loads as nothing — and Windows falls back to its own pointer for it
    // silently. That is the state behind almost every "the cursor doesn't work
    // in <application>", and it is invisible in a listing that only checks for
    // presence.
    let audit = crate::cursor::audit_roles();
    for role in &audit {
        if !role.set {
            let _ = writeln!(out, "  {:<12} (empty -- Windows default)", role.role);
            continue;
        }
        let note = if !role.exists {
            "   << FILE MISSING"
        } else if !role.ok {
            "   << NOT A LOADABLE CURSOR"
        } else {
            ""
        };
        let _ = writeln!(out, "  {:<12} {}{note}", role.role, role.value);
    }
    let faults = audit.iter().filter(|r| !r.ok).count();
    let _ = writeln!(
        out,
        "  {} of 17 set, {faults} fault(s)",
        audit.iter().filter(|r| r.set).count()
    );

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

/// Checks now, because someone asked.
///
/// Through `check_and_record`, never `check`. The panel takes its phase from the
/// shared update state on a timer, so a check that does not write that state is
/// overwritten by whatever was there before — which is how pressing this button
/// showed an available update for one and a half seconds and then went back to
/// claiming the app was up to date.
#[tauri::command]
pub fn check_for_updates() -> AppResult<updates::UpdateStatus> {
    updates::check_and_record()
}

/// Downloads the installer for an available update.
///
/// The frontend passes the version and asset name it was given by
/// `check_for_updates`; both are re-validated here rather than trusted, because
/// they end up in a URL and a filename.
#[tauri::command]
pub fn download_update(version: String, installer: String) -> AppResult<u64> {
    // Through the shared path, so the state the UI polls reflects what the
    // button just did. Downloading here without recording it was why pressing
    // Download appeared to do nothing: the next poll, at most three seconds
    // later, read a state that had never heard of it.
    updates::download_and_verify(&version, &installer)?;
    let file = updates::downloaded_path(&installer)?;
    Ok(std::fs::metadata(&file).map(|m| m.len()).unwrap_or(0))
}

/// Verifies the downloaded installer against the checksum published with the
/// release, then launches it. Refuses to run anything that does not match.
#[tauri::command]
pub fn install_update(app: AppHandle, version: String, installer: String) -> AppResult<()> {
    // Verified first, and nothing is torn down until it passes. A checksum
    // failure has to leave the app exactly as it was rather than half closed
    // around an installer that will never run.
    let file = updates::verified_installer(&version, &installer)?;

    // Recorded before the handover, because after it there is no "before" left
    // to record: the binary is replaced and this process is gone. The next
    // launch reads this to work out whether the update actually happened.
    if let Err(e) = updates::record_pending_install(&version) {
        // Not fatal. Losing the ability to *report* on an update is not a
        // reason to refuse to perform one.
        log::warn!("update: the pending-install record could not be written: {e}");
    }

    // Then everything this process holds, released in order.
    crate::prepare_for_shutdown(&app);

    // Only now — and if it fails, everything above is put straight back.
    //
    // Returning the error on its own was not enough: by this point the window
    // is hidden, the tray icon is gone and the hotkeys are released, so the
    // message went to a frontend nobody could see. What the user got was an app
    // that disappeared and an update that never happened.
    if let Err(e) = updates::launch(&file) {
        crate::abort_shutdown(&app);
        return Err(e);
    }

    // Immediately, not on a timer. The old code waited a second before exiting
    // in the hope of winning a race against the installer's running-app check;
    // that check no longer runs, because `/P` makes the installer terminate a
    // straggler silently instead of asking. Leaving promptly means there is
    // usually nothing left to terminate at all.
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
/// Every URL the app is allowed to hand to a browser.
///
/// Named rather than inlined so the test below can check it against every link
/// the frontend actually offers — the two halves are in different languages and
/// neither imports the other, so nothing but a test connects them.
const ALLOWED_EXTERNAL: [&str; 4] = [
    "https://github.com/notfeylo/cursed",
    "https://github.com/notfeylo/cursed/issues",
    "https://github.com/notfeylo/cursed/releases",
    // The one the update panel's "download it manually" button asks for, and
    // the one it was missing. That button is the last resort offered after an
    // update has already failed — it opened nothing at all, silently, because
    // the frontend asked for `/releases/latest` and this list stopped at
    // `/releases`.
    "https://github.com/notfeylo/cursed/releases/latest",
];

#[tauri::command]
pub fn open_external(url: String) -> AppResult<()> {
    if !ALLOWED_EXTERNAL.contains(&url.as_str()) {
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
    crate::begin_shutdown();
    app.exit(0);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every link the UI offers must be one the backend will open.
    ///
    /// `open_external` refuses anything not on its list, and the frontend
    /// swallows the refusal — `.catch(() => undefined)` — because a link that
    /// will not open is not worth an error banner. The combination is a button
    /// that does nothing, silently, with nothing in any log. That is what
    /// "download it manually" did: the panel asked for `/releases/latest` and
    /// the list stopped at `/releases`, so the last resort offered after a
    /// failed update was itself dead.
    ///
    /// The two halves live in different languages and neither imports the
    /// other, so this reads the frontend and checks.
    #[test]
    fn every_link_the_frontend_offers_is_one_the_backend_will_open() {
        let frontend = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .map(|repo| repo.join("src"));
        let Some(frontend) = frontend.filter(|dir| dir.is_dir()) else {
            // Only reachable outside a checkout; there is nothing to read.
            return;
        };

        let mut asked = Vec::new();
        let mut stack = vec![frontend];
        while let Some(dir) = stack.pop() {
            let Ok(listing) = std::fs::read_dir(&dir) else {
                continue;
            };
            for item in listing.flatten() {
                let path = item.path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                if !path.extension().is_some_and(|e| e == "tsx" || e == "ts") {
                    continue;
                }
                let Ok(text) = std::fs::read_to_string(&path) else {
                    continue;
                };
                // Only literal URLs. A variable is `Markdown`'s link handler,
                // which passes whatever a document contains and is expected to
                // be refused for anything not on the list.
                for piece in text.split("openExternal(\"").skip(1) {
                    if let Some(url) = piece.split('"').next() {
                        asked.push((path.clone(), url.to_owned()));
                    }
                }
            }
        }

        assert!(!asked.is_empty(), "no openExternal call sites found; the scan is broken");
        for (file, url) in asked {
            assert!(
                ALLOWED_EXTERNAL.contains(&url.as_str()),
                "{} opens {url}, which open_external refuses",
                file.display()
            );
        }
    }

    /// The version that ships must have something to say for itself, or the
    /// "what's new" panel is a blank box shown after every update.
    #[test]
    fn the_running_version_has_a_changelog_entry() {
        let running = env!("CARGO_PKG_VERSION");
        assert!(
            release_notes_for(CHANGELOG, running).is_some(),
            "CHANGELOG.md has no section for {running}; add one before releasing"
        );
    }

    #[test]
    fn a_section_stops_at_the_next_version() {
        let changelog = "\
# Changelog

## 1.21.0 — unreleased

Something changed.

---

## 1.20.0 — 2026-08-11

Something else did.
";
        let notes = release_notes_for(changelog, "1.21.0").expect("1.21.0");
        assert_eq!(notes, "Something changed.");
        assert!(!notes.contains("Something else"), "it ran into the next entry");

        // A leading v is how a git tag spells the same version.
        assert_eq!(release_notes_for(changelog, "v1.21.0"), Some("Something changed.".into()));
        assert_eq!(release_notes_for(changelog, "1.20.0"), Some("Something else did.".into()));
    }

    /// A build between releases is an ordinary state, not an error.
    #[test]
    fn a_version_with_no_entry_is_not_a_failure() {
        assert_eq!(release_notes_for("## 1.0.0 — x\n\nnotes\n", "9.9.9"), None);
        assert_eq!(release_notes_for("", "1.0.0"), None);
        assert_eq!(release_notes_for("## 1.0.0 — x\n", "1.0.0"), None);
        assert_eq!(release_notes_for("## 1.0.0 — x\n\nnotes\n", ""), None);
    }

    /// `1.2.0` must not match the `1.2.0-rc1` heading, nor `1.20.0` the `1.2.0`
    /// one. Matching on a prefix would do both.
    #[test]
    fn versions_are_matched_whole() {
        let changelog = "## 1.2.0 — x\n\nthe real one\n\n## 1.20.0 — y\n\nthe other one\n";
        assert_eq!(release_notes_for(changelog, "1.2.0"), Some("the real one".into()));
        assert_eq!(release_notes_for(changelog, "1.20.0"), Some("the other one".into()));
        assert_eq!(release_notes_for(changelog, "1."), None);
    }

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
        assert!(open_external("https://github.com/notfeylo/cursed.evil.com".into()).is_err());
    }

    #[test]
    fn only_zero_means_unspecified() {
        use crate::state::settings::{MAX_CURSOR_PX, MIN_CURSOR_PX};

        assert_eq!(effective_size(48), 48);
        assert_eq!(effective_size(9_999), MAX_CURSOR_PX);

        // Zero is the sentinel, and it is the only one. This test used to
        // assert that anything under 32 fell back to the system size, which was
        // the bug: once the range opened to 10 px, that threshold silently
        // swallowed every small request the slider could make.
        let inherited = effective_size(0);
        assert!((MIN_CURSOR_PX..=MAX_CURSOR_PX).contains(&inherited));
        assert_eq!(effective_size(MIN_CURSOR_PX), MIN_CURSOR_PX);
    }
}
