//! Giving memory back when the window is hidden (PRD §12).
//!
//! CursorForge spends nearly all of its life in the tray. While the window is
//! open the WebView2 host legitimately needs its working set; once it is hidden,
//! most of those pages are a rendering engine nobody is looking at.
//!
//! `SetProcessWorkingSetSize(-1, -1)` is the documented way to say "I am idle,
//! reclaim what you like". It does not free or corrupt anything: the pages stay
//! valid and fault back in if the window is reopened. It simply stops an idle
//! tray app from holding tens of megabytes of resident memory that the rest of
//! the machine could be using.
//!
//! The cost is paid on reopen, in page faults, which is the right trade for a
//! process that is hidden far more often than it is looked at.

/// Asks Windows to reclaim this process's unused resident pages.
///
/// Best-effort by design: if the call fails the app is simply a little larger,
/// which is not worth surfacing to anyone.
pub fn release_memory() {
    #[cfg(windows)]
    {
        use windows::Win32::System::Threading::{
            GetCurrentProcess, SetProcessWorkingSetSize,
        };
        // SAFETY: the pseudo-handle from GetCurrentProcess needs no cleanup, and
        // (usize::MAX, usize::MAX) is the documented "trim as you see fit"
        // sentinel rather than a real size.
        unsafe {
            let _ = SetProcessWorkingSetSize(GetCurrentProcess(), usize::MAX, usize::MAX);
        }
    }
}

/// Trims shortly after the window is hidden.
///
/// Deliberately delayed: hiding a window kicks off teardown work in the webview,
/// and trimming while that is still running just forces the same pages straight
/// back in. A couple of seconds later the process really is idle.
pub fn release_memory_soon() {
    std::thread::Builder::new()
        .name("cursorforge-trim".into())
        .spawn(|| {
            std::thread::sleep(std::time::Duration::from_secs(2));
            release_memory();
        })
        .ok();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn releasing_memory_is_safe_to_call_and_leaves_the_process_working() {
        // The point is that this is a hint to the OS, not a deallocation: memory
        // allocated before the call is still perfectly valid after it.
        let canary: Vec<u64> = (0..8_192).collect();
        release_memory();
        assert_eq!(canary.len(), 8_192);
        assert_eq!(canary[8_191], 8_191);

        // And it is idempotent.
        release_memory();
        release_memory();
    }
}
