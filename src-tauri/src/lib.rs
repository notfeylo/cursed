//! Cursed — Windows pointer replacement.
//!
//! The one architectural rule, restated here because every module depends on it:
//! **Cursed never draws a cursor. It only tells Windows which cursor to
//! draw.** No overlay window, no layered sprite, no hooking. Every pointer in
//! this product is a real `.cur` or `.ani` file handed to the OS, so it is drawn
//! by the GPU's hardware cursor plane — which is why added input latency is zero
//! rather than merely small.

pub mod autostart;
pub mod build;
pub mod commands;
pub mod cursor;
pub mod custom;
pub mod error;
pub mod hash;
pub mod hotkeys;
pub mod idle;
pub mod import;
pub mod packs;
pub mod paths;
pub mod session;
pub mod shell;
pub mod state;
pub mod tray;
pub mod updates;
pub mod util;
pub mod window_state;

use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{Manager, WindowEvent};

/// The window is created hidden so nobody ever sees an unpainted rectangle.
/// Whoever gets here first wins: normally the frontend's `frontend_ready`, and
/// otherwise the fallback below.
static SHOWN: AtomicBool = AtomicBool::new(false);

pub fn show_main_window(app: &tauri::AppHandle) {
    if SHOWN.swap(true, Ordering::SeqCst) {
        return;
    }
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// Shows the window even if the frontend never reports in.
///
/// A webview that fails to load must not leave a running process with no window
/// and no explanation — that is indistinguishable from the app being broken, and
/// the user cannot even quit it without finding the tray icon. Two seconds is
/// far longer than a local bundle needs and short enough not to feel stuck.
pub fn show_main_window_eventually(app: &tauri::AppHandle) {
    let app = app.clone();
    std::thread::Builder::new()
        .name("cursorforge-window-fallback".into())
        .spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(2));
            show_main_window(&app);
        })
        .ok();
}

pub fn run() {
    let settings = state::settings::get();

    let mut builder = tauri::Builder::default()
        // One instance owns the pointer. A second launch raises the first
        // window instead of starting a rival watchdog.
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            tray::show_window(app);
        }))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec![autostart::SILENT_FLAG]),
        ))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build());

    // A local rotating file, always on at info level — nothing leaves the
    // machine (PRD §15.2).
    //
    // It used to be off unless the user opted in, which meant the one time a
    // problem reached a user there was nothing on disk to read. A log that only
    // exists after you already know you needed it is not a log. The setting now
    // controls verbosity, not existence.
    if let Ok(dir) = paths::logs_dir() {
        builder = builder.plugin(
            tauri_plugin_log::Builder::new()
                .target(tauri_plugin_log::Target::new(
                    tauri_plugin_log::TargetKind::Folder {
                        path: dir,
                        file_name: Some("cursed".into()),
                    },
                ))
                // Bounded, so a long-running tray app cannot fill a disk.
                .max_file_size(2 * 1024 * 1024)
                .rotation_strategy(tauri_plugin_log::RotationStrategy::KeepOne)
                .level(if settings.debug_logging {
                    log::LevelFilter::Debug
                } else {
                    log::LevelFilter::Info
                })
                .build(),
        );
    }

    let result = builder
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::save_settings,
            commands::frontend_ready,
            commands::get_active_state,
            commands::get_cursor_base_size,
            commands::list_packs,
            commands::preview_pack,
            commands::clear_preview,
            commands::apply_pack,
            commands::restore_windows_default,
            commands::list_presets,
            commands::save_preset,
            commands::delete_preset,
            commands::set_default_preset,
            commands::duplicate_preset,
            commands::apply_preset,
            commands::export_preset,
            commands::import_cfpack,
            commands::import_image,
            commands::import_image_bytes,
            commands::preview_custom,
            commands::build_custom_cursor,
            commands::apply_custom_cursor,
            commands::delete_custom_cursor,
            commands::list_custom_cursors,
            commands::get_storage_dir,
            commands::open_storage_dir,
            commands::get_cache_size,
            commands::clear_cache,
            commands::get_legal_doc,
            commands::get_build_info,
            commands::get_diagnostics,
            commands::check_for_updates,
            commands::download_update,
            commands::install_update,
            commands::clear_update_downloads,
            commands::get_update_state,
            commands::start_update_check,
            commands::import_cursor_folder,
            commands::list_imported,
            commands::delete_imported,
            commands::delete_all_imported,
            commands::open_external,
            commands::hide_to_tray,
            commands::quit_app,
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            let settings = state::settings::get();

            log_startup_record();

            // Before anything else touches the registry: capture what was there
            // first. Idempotent, so this is a no-op on every launch but the
            // first (PRD §4.4).
            if let Err(e) = cursor::restore::capture_once() {
                log::warn!("could not capture the original cursor scheme: {e}");
            }

            state::settings::propagate(&settings);
            cursor::watchdog::start();
            session::restore_in_background();

            if let Err(e) = tray::create(&handle) {
                log::warn!("tray icon unavailable: {e}");
            }
            let _ = tray::set_visible(&handle, settings.show_tray_icon);
            let _ = hotkeys::register(&handle, &settings);
            let _ = autostart::apply(&handle, settings.launch_on_startup);

            if settings.auto_check_updates {
                updates::check_in_background();
            }

            if let Some(window) = app.get_webview_window("main") {
                window_state::restore(&window);

                // Set the window icon explicitly from the embedded asset.
                //
                // The taskbar button draws the *window* icon, and Windows will
                // happily keep showing a previously cached one for an executable
                // it has seen before — so an app whose logo changed shows the old
                // mark to the one person most likely to notice. Setting it here
                // sources it from the binary every launch, which no cache sits in
                // front of.
                match tauri::image::Image::from_bytes(include_bytes!("../icons/128x128.png")) {
                    Ok(icon) => {
                        if let Err(e) = window.set_icon(icon) {
                            log::warn!("could not set the window icon: {e}");
                        }
                    }
                    Err(e) => log::warn!("the embedded window icon could not be read: {e}"),
                }
            }

            // An autostarted launch belongs in the tray. A launch the user asked
            // for belongs on screen — even if "start minimised" is on, because
            // they just double-clicked the icon.
            if !(autostart::launched_silently() && settings.start_minimized) {
                show_main_window_eventually(&handle);
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                if let Some(main) = window.app_handle().get_webview_window("main") {
                    window_state::save(&main);
                }
                if state::settings::get().close_to_tray {
                    // Closing hides; quitting is a deliberate act from the tray
                    // or the hotkey, so the watchdog keeps working (PRD §9).
                    api.prevent_close();
                    let _ = window.hide();
                    // Hidden means idle: hand the webview's resident pages back
                    // rather than sitting on them for the rest of the session.
                    idle::release_memory_soon();
                }
            }
        })
        .run(tauri::generate_context!());

    if let Err(e) = result {
        // The last thing a GUI app should do is vanish without explanation.
        log::error!("Cursed could not start: {e}");
        eprintln!("Cursed could not start: {e}");
        std::process::exit(1);
    }
}

/// Written once per launch, before anything can go wrong.
///
/// Every line here has already been the difference between a working install and
/// a broken one: which build is really running, whether the data directory can
/// be written, and how many cursors the catalog can actually offer. Recorded
/// unconditionally, because a log that only starts after you know you needed it
/// is not a log.
fn log_startup_record() {
    log::info!(
        "Cursed {} ({}, {}) starting",
        env!("CARGO_PKG_VERSION"),
        option_env!("CURSORFORGE_COMMIT").unwrap_or("local"),
        std::env::consts::ARCH
    );

    for (label, dir) in [
        ("data", paths::root()),
        ("cache", paths::cache_dir()),
        ("custom", paths::custom_dir()),
        ("logs", paths::logs_dir()),
    ] {
        match dir {
            Ok(path) => {
                let writable = {
                    let probe = path.join(".write-probe");
                    let ok = std::fs::write(&probe, b"x").is_ok();
                    let _ = std::fs::remove_file(&probe);
                    ok
                };
                if writable {
                    log::info!("{label} dir {}", path.display());
                } else {
                    // Not fatal on its own, but it explains almost every
                    // downstream failure, so it must be loud.
                    log::error!("{label} dir is NOT writable: {}", path.display());
                }
            }
            Err(e) => log::error!("{label} dir unavailable: {e}"),
        }
    }

    let built_in = packs::styles::all().len();
    let imported = import::list().map(|v| v.len()).unwrap_or(0);
    log::info!("catalog: {built_in} built-in, {imported} imported");
    if built_in == 0 {
        log::error!("the built-in catalog is empty -- every machine would show no cursors");
    }
}
