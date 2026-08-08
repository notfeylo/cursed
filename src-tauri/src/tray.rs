//! The tray icon: what the app is, most of the time.
//!
//! CursorForge spends its life minimised. The tray menu therefore carries the
//! two actions that matter without opening the window — switch preset, and put
//! Windows back the way it was — plus a genuine Quit, because a tray app that
//! cannot be quit from its tray is a tray app people uninstall.

use crate::error::{AppError, AppResult};
use crate::state::presets;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Runtime};

pub const TRAY_ID: &str = "cursorforge";

const OPEN: &str = "open";
const RESTORE: &str = "restore";
const QUIT: &str = "quit";
const PRESET_PREFIX: &str = "preset:";

fn build_menu<R: Runtime>(app: &AppHandle<R>) -> AppResult<Menu<R>> {
    let map = |e: tauri::Error| AppError::msg(e.to_string());

    let open = MenuItem::with_id(app, OPEN, "Open CursorForge", true, None::<&str>).map_err(map)?;
    let restore = MenuItem::with_id(app, RESTORE, "Restore Windows Default", true, None::<&str>)
        .map_err(map)?;
    let quit = MenuItem::with_id(app, QUIT, "Quit", true, None::<&str>).map_err(map)?;
    let separator = PredefinedMenuItem::separator(app).map_err(map)?;

    let saved = presets::list().unwrap_or_default();
    let menu = if saved.is_empty() {
        Menu::with_items(app, &[&open, &separator, &restore, &separator, &quit]).map_err(map)?
    } else {
        let items: Vec<MenuItem<R>> = saved
            .iter()
            .take(10)
            .map(|preset| {
                MenuItem::with_id(
                    app,
                    format!("{PRESET_PREFIX}{}", preset.id),
                    &preset.name,
                    true,
                    None::<&str>,
                )
                .map_err(map)
            })
            .collect::<AppResult<_>>()?;
        let refs: Vec<&dyn tauri::menu::IsMenuItem<R>> =
            items.iter().map(|i| i as &dyn tauri::menu::IsMenuItem<R>).collect();
        let presets_menu = Submenu::with_items(app, "Presets", true, &refs).map_err(map)?;

        Menu::with_items(
            app,
            &[&open, &separator, &presets_menu, &restore, &separator, &quit],
        )
        .map_err(map)?
    };
    Ok(menu)
}

pub fn create(app: &AppHandle) -> AppResult<()> {
    let map = |e: tauri::Error| AppError::msg(e.to_string());
    let icon = app
        .default_window_icon()
        .cloned()
        .ok_or_else(|| AppError::msg("the application icon is missing"))?;

    TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon)
        .tooltip(tooltip_text())
        .menu(&build_menu(app)?)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| {
            let id = event.id().as_ref();
            match id {
                OPEN => show_window(app),
                RESTORE => {
                    let _ = crate::cursor::restore_default();
                    crate::session::forget();
                    refresh_tooltip(app);
                }
                QUIT => app.exit(0),
                other => {
                    if let Some(preset_id) = other.strip_prefix(PRESET_PREFIX) {
                        if let Ok(preset) = presets::get(preset_id) {
                            let _ = crate::commands::apply_preset_inner(app, &preset);
                        }
                    }
                }
            }
        })
        .on_tray_icon_event(|tray, event| {
            // Left click opens; right click is the menu, which Tauri handles.
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_window(tray.app_handle());
            }
        })
        .build(app)
        .map_err(map)?;
    Ok(())
}

/// Rebuilds the menu so newly saved presets appear without a restart.
pub fn refresh_menu(app: &AppHandle) {
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        if let Ok(menu) = build_menu(app) {
            let _ = tray.set_menu(Some(menu));
        }
    }
}

fn tooltip_text() -> String {
    match crate::cursor::active_state() {
        Ok(state) if !state.is_default => format!(
            "CursorForge — {} · {}px",
            state.pack_name.unwrap_or_else(|| "CUSTOM".into()),
            state.size
        ),
        _ => "CursorForge — Windows default".to_owned(),
    }
}

pub fn refresh_tooltip(app: &AppHandle) {
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let _ = tray.set_tooltip(Some(tooltip_text()));
    }
}

pub fn set_visible(app: &AppHandle, visible: bool) -> AppResult<()> {
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        tray.set_visible(visible)
            .map_err(|e| AppError::msg(e.to_string()))?;
    }
    Ok(())
}

/// Brings the window back, on the monitor the pointer is on (PRD §10.1).
pub fn show_window(app: &AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    crate::window_state::restore(&window);
    let _ = window.show();
    let _ = window.unminimize();
    let _ = window.set_focus();
}
