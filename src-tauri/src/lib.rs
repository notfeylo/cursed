//! Cursed — Windows pointer replacement.
//!
//! The one architectural rule, restated here because every module depends on it:
//! **Cursed never draws a cursor. It only tells Windows which cursor to
//! draw.** No overlay window, no layered sprite, no hooking. Every pointer in
//! this product is a real `.cur` or `.ani` file handed to the OS, so it is drawn
//! by the GPU's hardware cursor plane — which is why added input latency is zero
//! rather than merely small.

pub mod autostart;
pub mod backup;
pub mod build;
pub mod bundled;
pub mod channel;
pub mod commands;
pub mod cursor;
pub mod custom;
pub mod dataprint;
pub mod error;
#[cfg(test)]
mod fuzz;
pub mod hash;
pub mod hotkeys;
pub mod idle;
pub mod import;
pub mod packs;
pub mod paths;
pub mod photo;
pub mod session;
pub mod shell;
pub mod signing;
pub mod state;
pub mod stress;
pub mod tray;
pub mod updates;
pub mod util;
pub mod window_state;

use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{Manager, WindowEvent};

/// The taskbar mark, embedded rather than read from the bundle — see where it is
/// used. Per channel, because two side-by-side installs showing the same icon in
/// the taskbar and the Alt-Tab list leave clicking as the only way to find out
/// which is which.
#[cfg(not(feature = "dev-channel"))]
const WINDOW_ICON: &[u8] = include_bytes!("../icons/128x128.png");
#[cfg(feature = "dev-channel")]
const WINDOW_ICON: &[u8] = include_bytes!("../icons/dev/128x128.png");

/// The window is created hidden so nobody ever sees an unpainted rectangle.
/// Whoever gets here first wins: normally the frontend's `frontend_ready`, and
/// otherwise the fallback below.
static SHOWN: AtomicBool = AtomicBool::new(false);

/// Set the moment a quit is asked for, and never cleared.
///
/// Several things in this app happen on a delay — the window fallback below, the
/// pause before the updater exits — and a delay is a window in which the event
/// loop can be torn down underneath them. Showing or focusing a window after
/// that panics inside tao's runner with `cannot move state from Destroyed`,
/// which is a crash on the way out, in a GUI process with no console, from a
/// thread that has nothing to do with what the user just did. It was in this
/// machine's log once with nothing around it to explain it.
///
/// Anything that wakes up after sleeping asks this first.
static SHUTTING_DOWN: AtomicBool = AtomicBool::new(false);

/// Announces that this process is on its way out. Idempotent, one-way.
pub fn begin_shutdown() {
    SHUTTING_DOWN.store(true, Ordering::SeqCst);
}

/// Releases everything this process holds, before an installer replaces it.
///
/// An update hands the machine to an installer that is about to overwrite this
/// binary and everything beside it. Whatever is still running at that moment is
/// either something the installer has to kill, or something still writing to a
/// directory being replaced.
///
/// The installer can cope — passive mode terminates a running copy without
/// asking — but being killed is not the same as having exited. A killed process
/// does not save its window position, does not flush its log, and leaves the
/// watchdog's last act unknown. So everything that can be put down in order is
/// put down here, and the kill becomes the fallback it should be rather than the
/// mechanism.
///
/// Ordered deliberately, and the order is the point. Each step either releases
/// something the replacement copy will want, or stops something that would still
/// be writing while the installer replaces what it writes to.
///
///  1. **The watchdog stops, and is waited for.** It is the one thing that would
///     otherwise write registry values naming files the installer is deleting.
///     Told to stand down *and* confirmed gone — a thread parked in a thirty-
///     second wait has not stood down yet.
///  2. **Hotkeys are unregistered.** Global shortcuts are first-come-first-
///     served, and the copy that comes back wants them.
///  3. **The tray icon goes.** It is the one piece of UI that visibly outlives a
///     killed process: a stale icon sits in the notification area until
///     something makes Windows re-scan.
///  4. **The window position is saved, then the window is hidden.**
///  5. **The single-instance mutex is released**, or the relaunched copy sees a
///     previous instance that no longer exists and raises a window belonging to
///     nothing.
///  6. **The log is flushed**, because a rotating file writer in a process that
///     is about to be replaced is the last chance the record has.
///  7. **Any other copy in this session is waited for**, briefly, so the
///     installer is not handed a directory something else still has open.
///
/// What is deliberately *not* here: this process exiting. The caller has not
/// launched the installer yet, and a shutdown thorough enough to end the event
/// loop would exit before the thing it is exiting for had started.
pub fn prepare_for_shutdown(app: &tauri::AppHandle) {
    begin_shutdown();

    // Two seconds is generous: the thread is woken by a posted message rather
    // than left to time out, so the usual answer arrives in single-digit
    // milliseconds. A `false` here is worth a line and nothing more — the
    // installer runs passively and will terminate a straggler itself.
    if !cursor::watchdog::stop_and_wait(std::time::Duration::from_secs(2)) {
        log::warn!("shutdown: the watchdog did not stop in time; the installer will end it");
    }

    if let Err(e) = hotkeys::unregister_all(app) {
        log::warn!("shutdown: hotkeys could not be released: {e}");
    }

    if let Err(e) = tray::set_visible(app, false) {
        log::warn!("shutdown: the tray icon could not be removed: {e}");
    }

    if let Some(window) = app.get_webview_window("main") {
        // Saved first, because the position is read off a window that is about
        // to stop existing.
        window_state::save(&window);

        // Hidden, **not** destroyed. Destroying the last window ends the event
        // loop, and the caller has not launched the installer yet — so tidying
        // up that thoroughly here would exit the process before the thing it is
        // exiting for had been started, and the update would simply never
        // happen. The webview goes with the process a moment later.
        let _ = window.hide();
    }

    // Released explicitly rather than left to process death.
    //
    // `/R` relaunches the app the moment the installer finishes, which can be
    // before Windows has finished tearing this process down. The relaunched copy
    // would then find the mutex still held, decide it is the second instance,
    // and try to raise the window of a process that is exiting — so the update
    // completes and the app never appears.
    tauri_plugin_single_instance::destroy(app);

    log::logger().flush();

    // Last, because everything above releases something and this measures the
    // result. Only *other* processes are counted: this one is still here, and
    // still has an installer to launch.
    let stragglers = wait_for_other_instances(std::time::Duration::from_secs(3));
    if stragglers > 0 {
        log::warn!(
            "shutdown: {stragglers} other copy/copies of this app are still running in this \
             session; the installer will terminate them"
        );
    }
}

/// Undoes [`prepare_for_shutdown`], for the one case where the shutdown does not
/// end in an exit.
///
/// **An update that fails at the last step used to leave the app alive and
/// invisible.** `install_update` verifies the installer, releases the watchdog,
/// the hotkeys and the tray icon, hides the window, and only then spawns the
/// installer — so a spawn that fails (antivirus quarantine, a file locked by
/// the scanner, an exhausted desktop heap) returned an error to a frontend that
/// no longer had a window to show it in. From the outside the app simply
/// vanished: no window, no tray icon, no hotkeys, and the pointer scheme left
/// undefended for the rest of the session.
///
/// This puts back everything that can be put back, so the failure is what it
/// should always have been — an error message in a working app.
///
/// **One thing does not come back.** The single-instance mutex was destroyed on
/// the way down and this process no longer holds it, so a second copy launched
/// from now on will start rather than raising this window. That is a worse
/// state than before the update attempt and a far better one than an app the
/// user cannot see, and it lasts until the next restart.
pub fn abort_shutdown(app: &tauri::AppHandle) {
    log::warn!("shutdown: the installer did not start, so the app is being put back together");
    SHUTTING_DOWN.store(false, Ordering::SeqCst);

    cursor::watchdog::restart();
    let settings = state::settings::get();
    // After the restart, because `configure` is what decides whether the thread
    // that is now running is allowed to act.
    state::settings::propagate(&settings);

    if let Err(e) = hotkeys::register(app, &settings) {
        log::warn!("shutdown: hotkeys could not be re-registered: {e}");
    }
    if let Err(e) = tray::set_visible(app, settings.show_tray_icon) {
        log::warn!("shutdown: the tray icon could not be restored: {e}");
    }
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// Waits for any other copy of this executable in this Windows session to exit,
/// and returns how many were still there when the wait ran out.
///
/// A second copy is not supposed to exist — the single-instance plugin sees to
/// that — but "not supposed to" is not the same as "does not", and the case that
/// matters is a previous instance that was killed rather than closed and whose
/// process object has not been reaped. Anything still holding the install
/// directory when the installer starts is what produces the "failed to kill,
/// close it first" abort the diagnosis records.
///
/// Session-scoped on purpose, matching the installer: it is built `currentUser`
/// and uses `FindProcessCurrentUser`, so a second signed-in Windows user running
/// Cursed is neither seen nor terminated by it, and must not be waited for here
/// either.
fn wait_for_other_instances(timeout: std::time::Duration) -> usize {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        let others = session_siblings();
        if others == 0 {
            return 0;
        }
        if std::time::Instant::now() >= deadline {
            return others;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

/// How many other processes of this executable are running in this session.
fn session_siblings() -> usize {
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };
    use windows::Win32::System::RemoteDesktop::ProcessIdToSessionId;

    let Ok(exe) = std::env::current_exe() else {
        return 0;
    };
    let Some(name) = exe.file_name().map(|n| n.to_string_lossy().to_ascii_lowercase()) else {
        return 0;
    };

    let us = std::process::id();
    let mut our_session = 0u32;
    // SAFETY: writing a `u32` we own. A failure leaves it zero, which the
    // comparison below treats as "sessions are unknown", counting nothing.
    if unsafe { ProcessIdToSessionId(us, &mut our_session) }.is_err() {
        return 0;
    }

    // SAFETY: the snapshot handle is closed by `Owned` when this scope ends, and
    // every entry is a stack local sized by `dwSize` as the API requires.
    unsafe {
        let Ok(snapshot) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
            return 0;
        };
        let snapshot = windows::core::Owned::new(snapshot);

        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        if Process32FirstW(*snapshot, &mut entry).is_err() {
            return 0;
        }

        let mut found = 0;
        loop {
            let pid = entry.th32ProcessID;
            if pid != us && pid != 0 {
                let theirs = String::from_utf16_lossy(&entry.szExeFile)
                    .trim_end_matches('\0')
                    .to_ascii_lowercase();
                if theirs == name {
                    let mut session = u32::MAX;
                    if ProcessIdToSessionId(pid, &mut session).is_ok() && session == our_session {
                        found += 1;
                    }
                }
            }
            if Process32NextW(*snapshot, &mut entry).is_err() {
                break;
            }
        }
        found
    }
}

pub fn is_shutting_down() -> bool {
    SHUTTING_DOWN.load(Ordering::SeqCst)
}

pub fn show_main_window(app: &tauri::AppHandle) {
    // Checked before `SHOWN`, so a quit that lands first does not also consume
    // the one-shot and leave a genuine later show silently doing nothing.
    if is_shutting_down() {
        return;
    }
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

/// Writes a panic to the log before the process dies.
///
/// A GUI build has no console — `windows_subsystem = "windows"` sees to that —
/// so the default panic handler prints its message to a stderr nobody is
/// reading and the app simply vanishes. From the outside that is
/// indistinguishable from "I clicked it and nothing happened", which is the
/// least actionable bug report there is.
///
/// The default hook still runs afterwards, so a debug build keeps its console
/// output and its backtrace.
fn log_panics() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // `panic::Location` gives file and line; the payload is the message.
        // Both are best-effort — a panic with a non-string payload is rare but
        // must not panic *this* hook, which would abort with no log at all.
        let where_ = info
            .location()
            .map(|l| format!("{}:{}", l.file(), l.line()))
            .unwrap_or_else(|| "unknown location".to_owned());
        let what = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| (*s).to_owned())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "unknown payload".to_owned());

        log::error!("panic at {where_}: {what}");
        // The log is a rotating file writer, and the process is about to end.
        // Nothing here forces a flush, so this is the one place worth being
        // explicit that the record has to reach disk before exit.
        log::logger().flush();

        default(info);
    }));
}

pub fn run() {
    // Installed before anything else so a panic during startup — the window
    // that never appears — is still recorded.
    log_panics();

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
            commands::get_scheme_status,
            commands::acknowledge_scheme_loss,
            commands::list_presets,
            commands::save_preset,
            commands::delete_preset,
            commands::set_default_preset,
            commands::duplicate_preset,
            commands::apply_preset,
            commands::export_preset,
            commands::import_cfpack,
            commands::open_matte_editor,
            commands::preview_matte,
            commands::get_photo_status,
            commands::install_photo_mode,
            commands::get_photo_progress,
            commands::cancel_photo_install,
            commands::remove_photo_mode,
            commands::suggested_backup_name,
            commands::export_all_data,
            commands::import_all_data,
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
            commands::get_release_notes,
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

            // Whether the last launch handed the machine to an installer, and
            // whether that installer did what it said. Answered here because it
            // can only be answered here: the process that started the update is
            // gone, and the version now running is the only evidence of how it
            // went.
            updates::settle_and_report();

            // And whether a photo-mode removal is still waiting on a library
            // that was loaded when the user pressed the button. Here because
            // this is the last moment before anything can load it again.
            photo::sweep_pending_removal();

            // Before anything else touches the registry: capture what was there
            // first. Idempotent, so this is a no-op on every launch but the
            // first (PRD §4.4).
            if let Err(e) = cursor::restore::capture_once() {
                log::warn!("could not capture the original cursor scheme: {e}");
            }

            // Shipped packs land before the catalog is first asked for, so a
            // machine that has never run the app already has them.
            bundled::install_missing();

            // Claimed here rather than left to the watchdog's first tick, which
            // is a whole interval away. A channel that spends its first five
            // seconds not knowing whether it owns the pointer cannot tell the
            // person at the keyboard either, and the window is on screen by
            // then.
            if channel::guards_pointer_by_default() {
                cursor::crosschannel::try_claim();
            }
            match cursor::crosschannel::deferral_notice() {
                Some(notice) => log::info!("cross-channel: {notice}"),
                // Worth a line of its own: it is the answer to "why is my dev
                // build not putting the cursor back", and without it the only
                // evidence is an absence of log lines.
                None if !channel::guards_pointer_by_default() => log::info!(
                    "cross-channel: the {} channel does not defend the pointer scheme",
                    channel::NAME
                ),
                None => {}
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
                match tauri::image::Image::from_bytes(WINDOW_ICON) {
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
                // Not while shutting down. `prepare_for_shutdown` destroys the
                // window on the way into an update, and answering that by
                // hiding to the tray would leave the webview alive holding the
                // files the installer is about to replace.
                if !is_shutting_down() && state::settings::get().close_to_tray {
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
    // The marker is printed rather than merely defined, and this is the only
    // place it is read at runtime. `check-bundle.mjs` fails any release artifact
    // containing the dev channel's marker, and it can only find a string the
    // linker kept — which it does because this line uses it. A `#[used]` static
    // would express the intent better and survive `/OPT:REF` worse.
    log::info!(
        "Cursed {} ({}, {}, {} channel) starting [{}]",
        env!("CARGO_PKG_VERSION"),
        option_env!("CURSED_COMMIT").unwrap_or("local"),
        std::env::consts::ARCH,
        channel::NAME,
        channel::MARKER
    );

    // Which of the two update guarantees this build actually carries. Recorded
    // every launch because the weaker one is invisible from the outside — an
    // unsigned build and a signed one behave identically right up until the
    // moment it matters.
    log::info!("update verification: {}", signing::describe());

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
