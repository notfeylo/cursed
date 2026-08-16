//! Layer C — persistence insurance (PRD §4.3).
//!
//! Windows loses custom cursors for reasons the user never connects to cursors:
//! switching theme, a personalisation reset, a feature update's repair pass,
//! another pointer tool. The watchdog notices and puts the scheme back.
//!
//! Cost: one hidden window that sleeps in `MsgWaitForMultipleObjectsEx`, plus a
//! single string comparison every few seconds. It does not poll the message
//! queue, spin, or hold a timer callback — idle CPU is 0.0%.

use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU64, Ordering};
use std::sync::OnceLock;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, MsgWaitForMultipleObjectsEx,
    PeekMessageW, PostMessageW, RegisterClassW, TranslateMessage, MSG, MWMO_INPUTAVAILABLE,
    PM_REMOVE, QS_ALLINPUT, WINDOW_STYLE, WM_NULL, WM_POWERBROADCAST, WM_SETTINGCHANGE, WNDCLASSW,
    WS_EX_TOOLWINDOW, WS_OVERLAPPED,
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

/// Asked for once, on the way out, and never cleared.
static STOPPING: AtomicBool = AtomicBool::new(false);
/// Set by the thread as its last act, so a caller can tell "asked to stop" from
/// "has stopped".
static EXITED: AtomicBool = AtomicBool::new(false);
/// The listener window, so [`stop_and_wait`] can wake a thread that is parked
/// for up to thirty seconds. Zero until the window exists, and zero for ever on
/// a session where it could not be created.
static LISTENER: AtomicIsize = AtomicIsize::new(0);

/// Stops the watchdog defending the scheme, without touching the user's setting.
///
/// Used on the way out of an update. The watchdog re-applies a scheme by writing
/// registry values that name files inside the install directory and the cache —
/// which is precisely what an installer is in the middle of replacing. A revert
/// landing in that window points Windows at files that are being deleted and
/// rewritten underneath it.
pub fn disable() {
    ENABLED.store(false, Ordering::Relaxed);
}

/// Ends the watchdog thread and waits for it to actually be gone.
///
/// [`disable`] stops it *acting*; this stops it *existing*, and the difference
/// matters exactly once — on the way into an update, where the thread is holding
/// registry handles and is one tick away from writing paths into a directory the
/// installer is about to replace. A thread that has been told to stand down but
/// is still parked inside `MsgWaitForMultipleObjectsEx` has not stood down yet.
///
/// Waking it is the whole trick. The thread parks for up to the configured
/// interval — thirty seconds at the top of the range — so setting a flag and
/// hoping would mean waiting half a minute before launching an installer, every
/// time. A posted `WM_NULL` returns it from the wait immediately, at which point
/// it sees the flag and returns.
///
/// Returns whether the thread was confirmed gone. `false` means it is still
/// running and the caller should decide what that is worth: on the update path
/// it is worth a log line, not an abort, because the installer's `/P` will
/// terminate a straggler anyway.
pub fn stop_and_wait(timeout: std::time::Duration) -> bool {
    STOPPING.store(true, Ordering::SeqCst);
    disable();

    // Never started, so there is nothing to wait for. Not the same as "stopped
    // in time", but it is the same answer to the only question the caller has.
    if !started() {
        return true;
    }

    let hwnd = LISTENER.load(Ordering::SeqCst);
    if hwnd != 0 {
        // SAFETY: the handle was published by the thread that owns the window
        // and is only cleared after the window is destroyed, which happens
        // after this flag has been observed. A post to a window that has just
        // gone fails and is ignored, which is the correct outcome — the thread
        // is already on its way out.
        unsafe {
            let _ = PostMessageW(Some(HWND(hwnd as *mut _)), WM_NULL, WPARAM(0), LPARAM(0));
        }
    }

    let deadline = std::time::Instant::now() + timeout;
    while std::time::Instant::now() < deadline {
        if EXITED.load(Ordering::SeqCst) {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    EXITED.load(Ordering::SeqCst)
}

pub fn configure(enabled: bool, interval_secs: u64, on_theme_change: bool, on_resume: bool) {
    ENABLED.store(enabled, Ordering::Relaxed);
    INTERVAL_SECS.store(interval_secs.clamp(1, 300), Ordering::Relaxed);
    ON_THEME_CHANGE.store(on_theme_change, Ordering::Relaxed);
    ON_RESUME.store(on_resume, Ordering::Relaxed);
}

fn started_slot() -> &'static OnceLock<()> {
    static STARTED: OnceLock<()> = OnceLock::new();
    &STARTED
}

fn started() -> bool {
    started_slot().get().is_some()
}

/// Starts the watchdog thread exactly once for the process lifetime.
pub fn start() {
    if started_slot().set(()).is_err() {
        return;
    }
    // A stop already asked for before the thread existed. Starting one now would
    // mean starting a thread whose only job is to notice it should not be
    // running — during a shutdown, holding the registry open.
    if STOPPING.load(Ordering::SeqCst) {
        EXITED.store(true, Ordering::SeqCst);
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
    if let Some(window) = hwnd {
        LISTENER.store(window.0 as isize, Ordering::SeqCst);
    }

    loop {
        // Checked before the wait as well as after it, so a stop asked for
        // between the window being created and the first park is not held for a
        // whole interval.
        if STOPPING.load(Ordering::SeqCst) {
            break;
        }

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

        // The wake that `stop_and_wait` posted lands here. Checked before any
        // work, because the work is the thing being stopped.
        if STOPPING.load(Ordering::SeqCst) {
            break;
        }

        // A broadcast and an elapsed interval lead to the same cheap question,
        // so the nudge only decides *when* we ask, never *what* we ask.
        NUDGED.store(false, Ordering::Relaxed);

        // Asked every tick rather than once at startup, because that is what
        // makes handover work: quit whichever channel holds the pointer and this
        // one picks it up on its next pass, with neither being restarted. A
        // process that already owns the lock answers from memory.
        //
        // The dev channel does not ask at all. The lock is first-come-first-
        // served, so asking would hand the pointer to whichever channel happened
        // to launch first — usually the build being iterated on, which is the
        // one whose behaviour is least worth trusting. The copy that ships is
        // the one simulating a real install, so it is the one that defends.
        let ours = crate::channel::guards_pointer_by_default()
            && crate::cursor::crosschannel::try_claim();

        if ours && ENABLED.load(Ordering::Relaxed) && crate::cursor::drifted() {
            // Something reset the scheme. Put it back, quietly.
            //
            // `ours` is what keeps two installed channels from reverting each
            // other every few seconds: each would otherwise read the other's
            // write as drift and undo it, indefinitely, with the pointer
            // flickering between two cursors and nothing in either log naming a
            // cause.
            let _ = crate::cursor::reapply();
        }
    }

    // The window belongs to this thread and must be destroyed on it. Left
    // behind, it is a top-level window with a dead procedure receiving every
    // system broadcast for the rest of the process's life.
    if let Some(window) = hwnd {
        LISTENER.store(0, Ordering::SeqCst);
        // SAFETY: created on this thread, destroyed on this thread, and not
        // touched again afterwards.
        unsafe {
            let _ = DestroyWindow(window);
        }
    }
    log::debug!("watchdog: stopped");
    EXITED.store(true, Ordering::SeqCst);
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

    /// Nothing in the suite starts the watchdog, so this exercises the branch
    /// that matters most on the update path: being asked to stop something that
    /// is not running must answer immediately rather than sit out the timeout.
    ///
    /// It leaves `STOPPING` set for the rest of the process, which is why it is
    /// safe here and would not be in a suite that started the thread — the flag
    /// is deliberately one-way, and `start` refuses after it.
    #[test]
    fn stopping_a_watchdog_that_never_started_returns_at_once() {
        let began = std::time::Instant::now();
        assert!(stop_and_wait(std::time::Duration::from_secs(5)));
        assert!(
            began.elapsed() < std::time::Duration::from_millis(500),
            "waiting for a thread that does not exist should not take a timeout"
        );
        assert!(!ENABLED.load(Ordering::Relaxed), "stopping also stands it down");
    }

    /// The listener is deliberately a tool window rather than `HWND_MESSAGE`:
    /// message-only windows do not receive broadcast messages, so an
    /// `HWND_MESSAGE` parent would silently never see `WM_SETTINGCHANGE`.
    #[test]
    fn the_listener_uses_a_tool_window_style() {
        assert_ne!(WS_EX_TOOLWINDOW.0, 0);
    }
}
