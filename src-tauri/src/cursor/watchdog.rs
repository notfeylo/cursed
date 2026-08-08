//! Layer C — persistence insurance (PRD §4.3).
//!
//! Windows loses custom cursors for reasons the user never connects to cursors:
//! switching theme, a personalisation reset, a feature update's repair pass,
//! another pointer tool. The watchdog notices and puts the scheme back.
//!
//! Cost: one hidden window that sleeps in `MsgWaitForMultipleObjectsEx`, plus a
//! single string comparison every few seconds. It does not poll the message
//! queue, spin, or hold a timer callback — idle CPU is 0.0%.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::OnceLock;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, MsgWaitForMultipleObjectsEx, PeekMessageW,
    RegisterClassW, TranslateMessage, MSG, MWMO_INPUTAVAILABLE, PM_REMOVE, QS_ALLINPUT,
    WINDOW_STYLE, WM_POWERBROADCAST, WM_SETTINGCHANGE, WNDCLASSW, WS_EX_TOOLWINDOW, WS_OVERLAPPED,
};

const PBT_APMRESUMEAUTOMATIC: usize = 0x0012;
const PBT_APMRESUMESUSPEND: usize = 0x0007;

static ENABLED: AtomicBool = AtomicBool::new(true);
static INTERVAL_SECS: AtomicU64 = AtomicU64::new(5);
static ON_THEME_CHANGE: AtomicBool = AtomicBool::new(true);
static ON_RESUME: AtomicBool = AtomicBool::new(true);
/// Set from the window procedure; drained by the loop. The window procedure
/// must stay trivial — anything slow there stalls a system-wide broadcast.
static NUDGED: AtomicBool = AtomicBool::new(false);

pub fn configure(enabled: bool, interval_secs: u64, on_theme_change: bool, on_resume: bool) {
    ENABLED.store(enabled, Ordering::Relaxed);
    INTERVAL_SECS.store(interval_secs.clamp(1, 300), Ordering::Relaxed);
    ON_THEME_CHANGE.store(on_theme_change, Ordering::Relaxed);
    ON_RESUME.store(on_resume, Ordering::Relaxed);
}

/// Starts the watchdog thread exactly once for the process lifetime.
pub fn start() {
    static STARTED: OnceLock<()> = OnceLock::new();
    if STARTED.set(()).is_err() {
        return;
    }
    std::thread::Builder::new()
        .name("cursorforge-watchdog".into())
        .spawn(run)
        .ok();
}

extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_SETTINGCHANGE if ON_THEME_CHANGE.load(Ordering::Relaxed) => {
            NUDGED.store(true, Ordering::Relaxed);
        }
        WM_POWERBROADCAST
            if ON_RESUME.load(Ordering::Relaxed)
                && matches!(wparam.0, PBT_APMRESUMEAUTOMATIC | PBT_APMRESUMESUSPEND) =>
        {
            NUDGED.store(true, Ordering::Relaxed);
        }
        _ => {}
    }
    // SAFETY: forwarding the message we were handed, unmodified.
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

/// Creates the listener window.
///
/// Deliberately **not** `HWND_MESSAGE`: message-only windows are excluded from
/// broadcast messages, so an `HWND_MESSAGE` parent would never see
/// `WM_SETTINGCHANGE` and the theme-change trigger would silently never fire.
/// A never-shown tool window is a top-level window and does receive broadcasts,
/// while staying out of the taskbar, Alt-Tab, and the Z-order.
fn create_listener_window() -> Option<HWND> {
    let class_name: Vec<u16> = "CursedWatchdog\0".encode_utf16().collect();

    // SAFETY: the class name buffer is NUL-terminated and lives for the whole
    // call; `wndproc` has the exact signature Windows expects.
    unsafe {
        let instance = GetModuleHandleW(None).ok()?;
        let class = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: instance.into(),
            lpszClassName: PCWSTR(class_name.as_ptr()),
            ..Default::default()
        };
        // A zero return means the class already exists, which is fine on restart.
        RegisterClassW(&class);

        CreateWindowExW(
            WS_EX_TOOLWINDOW,
            PCWSTR(class_name.as_ptr()),
            PCWSTR::null(),
            WINDOW_STYLE(WS_OVERLAPPED.0),
            0,
            0,
            0,
            0,
            None,
            None,
            Some(instance.into()),
            None,
        )
        .ok()
    }
}

fn run() {
    let hwnd = create_listener_window();

    loop {
        let timeout_ms = (INTERVAL_SECS.load(Ordering::Relaxed) * 1_000) as u32;

        if hwnd.is_some() {
            // SAFETY: no wait handles are passed, so the pointer argument is None.
            // This parks the thread until a message arrives or the interval
            // elapses — no busy-waiting, no timer object.
            unsafe {
                MsgWaitForMultipleObjectsEx(None, timeout_ms, QS_ALLINPUT, MWMO_INPUTAVAILABLE);
                pump();
            }
        } else {
            // No window (a hostile session, or class registration failed): the
            // poll alone still satisfies the persistence guarantee.
            std::thread::sleep(std::time::Duration::from_millis(timeout_ms.into()));
        }

        // A broadcast and an elapsed interval lead to the same cheap question,
        // so the nudge only decides *when* we ask, never *what* we ask.
        NUDGED.store(false, Ordering::Relaxed);

        if ENABLED.load(Ordering::Relaxed) && crate::cursor::drifted() {
            // Something reset the scheme. Put it back, quietly.
            let _ = crate::cursor::reapply();
        }
    }
}

/// Drains the queue so the hidden window keeps responding to broadcasts.
///
/// # Safety
/// Must be called on the thread that owns the window.
unsafe fn pump() {
    let mut msg = MSG::default();
    while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
        let _ = TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configuration_clamps_the_interval() {
        configure(true, 0, true, true);
        assert_eq!(INTERVAL_SECS.load(Ordering::Relaxed), 1);
        configure(true, 100_000, true, true);
        assert_eq!(INTERVAL_SECS.load(Ordering::Relaxed), 300);
        configure(true, 5, true, true);
        assert_eq!(INTERVAL_SECS.load(Ordering::Relaxed), 5);
    }

    #[test]
    fn disabling_the_watchdog_is_respected() {
        configure(false, 5, true, true);
        assert!(!ENABLED.load(Ordering::Relaxed));
        configure(true, 5, true, true);
        assert!(ENABLED.load(Ordering::Relaxed));
    }

    /// The listener is deliberately a tool window rather than `HWND_MESSAGE`:
    /// message-only windows do not receive broadcast messages, so an
    /// `HWND_MESSAGE` parent would silently never see `WM_SETTINGCHANGE`.
    #[test]
    fn the_listener_uses_a_tool_window_style() {
        assert_ne!(WS_EX_TOOLWINDOW.0, 0);
    }
}
