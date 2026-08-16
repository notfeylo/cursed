//! Where the window opens (PRD §10.1).
//!
//! Two requirements that pull against each other: remember position and size,
//! *and* always open on the monitor the pointer is on. The resolution is to
//! honour the remembered position only when it is still on that monitor, and
//! otherwise centre on it. Restoring a window to a monitor that is no longer
//! there — or that the user is not looking at — reads as a bug either way.

use crate::error::AppResult;
use crate::paths;
use serde::{Deserialize, Serialize};
use tauri::{LogicalSize, Manager, PhysicalPosition, WebviewWindow};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowState {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

fn file() -> AppResult<std::path::PathBuf> {
    Ok(paths::root()?.join("window.json"))
}

pub fn save(window: &WebviewWindow) {
    let Ok(position) = window.outer_position() else {
        return;
    };
    let Ok(size) = window.inner_size() else {
        return;
    };
    let state = WindowState {
        x: position.x,
        y: position.y,
        width: size.width,
        height: size.height,
    };
    // Through the shared store like every other state file. This one wrote
    // straight over the top, so a crash while saving the window position left a
    // truncated file — and the next launch opened at whatever the defaults are
    // rather than where the user put the window.
    if let Ok(path) = file() {
        if let Ok(text) = serde_json::to_string(&state) {
            let _ = crate::state::store::write(&path, &text);
        }
    }
}

fn load() -> Option<WindowState> {
    let path = file().ok()?;
    let (state, _) = crate::state::store::read::<Option<WindowState>>(&path);
    state
}

pub fn restore(window: &WebviewWindow) {
    let app = window.app_handle();

    let cursor_monitor = app
        .cursor_position()
        .ok()
        .and_then(|point| {
            app.monitor_from_point(point.x, point.y).ok().flatten()
        })
        .or_else(|| window.primary_monitor().ok().flatten());

    if let Some(state) = load() {
        // Clamp to the PRD's declared bounds in case the file was hand-edited.
        let _ = window.set_size(LogicalSize::new(
            state.width.clamp(400, 520),
            state.height.clamp(600, 900),
        ));

        let on_cursor_monitor = cursor_monitor.as_ref().is_some_and(|monitor| {
            let position = monitor.position();
            let size = monitor.size();
            state.x >= position.x
                && state.y >= position.y
                && state.x < position.x + size.width as i32
                && state.y < position.y + size.height as i32
        });

        if on_cursor_monitor {
            let _ = window.set_position(PhysicalPosition::new(state.x, state.y));
            return;
        }
    }

    if let Some(monitor) = cursor_monitor {
        if let Ok(size) = window.outer_size() {
            let area = monitor.size();
            let origin = monitor.position();
            let x = origin.x + (area.width as i32 - size.width as i32) / 2;
            let y = origin.y + (area.height as i32 - size.height as i32) / 2;
            let _ = window.set_position(PhysicalPosition::new(x, y));
        }
    }
}
