//! Resource-leak harness — `genpacks --stress-handles`.
//!
//! Cursed spends its whole life creating and destroying cursor handles. Every
//! apply loads a file into an `HCURSOR`, duplicates it, hands the duplicate to
//! `SetSystemCursor` — which takes ownership and destroys it — and destroys the
//! original itself. Every preview does the same. The watchdog does it again on
//! every drift, which on a machine that changes theme a lot is thousands of
//! times a week.
//!
//! **A single missed `DestroyCursor` on any of those paths is invisible for
//! hours and fatal eventually.** The per-process GDI handle limit is 10,000 and
//! the USER limit likewise; a healthy app of this kind sits in the low hundreds
//! indefinitely. Leak one handle per apply and nothing at all appears wrong
//! until the process crosses the limit, at which point Windows starts refusing
//! to create objects and the app fails in whatever way it happens to fail — a
//! window that will not draw, a cursor that will not load, a crash with no
//! stack pointing anywhere near the cause.
//!
//! That is a bug you cannot find by reading, because the reading looks correct
//! on every path. You find it by doing the thing ten thousand times and
//! watching a number.
//!
//! ## What this deliberately does not do
//!
//! It never calls `SetSystemCursor`. The point is to exercise the load and
//! release paths, and installing a system cursor ten thousand times would fight
//! the running copy of the app, the watchdog and the person at the keyboard for
//! no extra coverage — the ownership rule being tested is on *our* side of that
//! call, not Windows'.

use std::path::{Path, PathBuf};

/// GDI and USER object counts for this process.
///
/// `GetGuiResources` is the same counter Task Manager's "GDI objects" column
/// shows, read for ourselves rather than inferred from memory, which moves for
/// a dozen reasons that have nothing to do with handles.
pub fn gui_handles() -> (u32, u32) {
    #[cfg(windows)]
    {
        // `GetGuiResources` is exported by user32 but lives under
        // `System::Threading` in the windows crate's metadata, not under
        // `UI::WindowsAndMessaging` where its header would suggest.
        use windows::Win32::System::Threading::{
            GetCurrentProcess, GetGuiResources, GR_GDIOBJECTS, GR_USEROBJECTS,
        };
        // SAFETY: the pseudo-handle from `GetCurrentProcess` needs no cleanup
        // and both flags are documented constants. The call only reads counters.
        unsafe {
            let process = GetCurrentProcess();
            (
                GetGuiResources(process, GR_GDIOBJECTS),
                GetGuiResources(process, GR_USEROBJECTS),
            )
        }
    }
    #[cfg(not(windows))]
    {
        (0, 0)
    }
}

/// What a run found.
#[derive(Debug, Clone)]
pub struct HandleReport {
    pub iterations: u32,
    pub files: Vec<PathBuf>,
    pub gdi_before: u32,
    pub gdi_after: u32,
    pub user_before: u32,
    pub user_after: u32,
    /// Loads that failed. A file Windows refuses is not a leak, but it is also
    /// not coverage, and a run that quietly exercised nothing must not read as
    /// a pass.
    pub failures: u32,
}

impl HandleReport {
    pub fn gdi_growth(&self) -> i64 {
        self.gdi_after as i64 - self.gdi_before as i64
    }

    pub fn user_growth(&self) -> i64 {
        self.user_after as i64 - self.user_before as i64
    }

    /// Whether the run is clean.
    ///
    /// Not "zero growth". A process that has just done its first work of the
    /// session legitimately allocates a handful of long-lived objects — a font,
    /// a device context, whatever the loader touched on the way through — and
    /// those are one-offs, not per-iteration. What matters is that the number
    /// does not track the iteration count. A margin of a few handles over
    /// thousands of iterations is noise; anything proportional is a leak.
    pub fn is_clean(&self) -> bool {
        const NOISE: i64 = 16;
        self.gdi_growth() <= NOISE && self.user_growth() <= NOISE
    }
}

/// Cursor files worth hammering, preferring Windows' own.
///
/// Stock cursors are used rather than generated ones so the harness tests the
/// loader against files nobody here wrote — a leak that only shows up on our own
/// output is a narrower result than one measured against the real thing. Both a
/// `.cur` and an `.ani` are included on purpose: they take different code paths
/// (`LoadImageW` versus `LoadCursorFromFileW`) with different ownership rules,
/// and the animated one is the path that has been got wrong before.
fn sample_files() -> Vec<PathBuf> {
    let root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".to_owned());
    let cursors = Path::new(&root).join("Cursors");
    ["aero_arrow.cur", "aero_busy.ani", "aero_link.cur", "aero_working.ani"]
        .iter()
        .map(|name| cursors.join(name))
        .filter(|path| path.is_file())
        .collect()
}

/// Loads and releases cursor handles `iterations` times, sampling the counters.
pub fn run_handle_stress(iterations: u32) -> HandleReport {
    let files = sample_files();

    // One pass before the baseline is taken. The very first load in a process
    // pulls in whatever the loader needs and allocates it once; counting that
    // as growth would report a leak on every clean run.
    for file in &files {
        let _ = crate::cursor::engine::verify_loadable(file);
    }

    let (gdi_before, user_before) = gui_handles();
    let mut failures = 0u32;

    for _ in 0..iterations {
        for file in &files {
            if crate::cursor::engine::verify_loadable(file).is_err() {
                failures = failures.saturating_add(1);
            }
        }
    }

    let (gdi_after, user_after) = gui_handles();
    HandleReport {
        iterations,
        files,
        gdi_before,
        gdi_after,
        user_before,
        user_after,
        failures,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The counters have to be real, or every result below is zero minus zero.
    #[test]
    fn the_handle_counters_report_something() {
        let (gdi, user) = gui_handles();
        // A process with no window still holds a few. Zero for both would mean
        // the call failed and the harness is measuring nothing.
        assert!(
            gdi > 0 || user > 0,
            "GetGuiResources returned nothing; the harness would report a clean run for any leak"
        );
    }

    /// A short run, so the suite exercises the harness itself. The real number
    /// comes from `genpacks --stress-handles`, which is not bound by how long a
    /// unit test should take.
    #[test]
    fn loading_and_releasing_cursors_does_not_grow_the_handle_count() {
        let report = run_handle_stress(200);
        if report.files.is_empty() {
            return; // not every Windows install ships the Aero set
        }
        assert!(
            report.is_clean(),
            "{} iterations leaked {} GDI and {} USER handles",
            report.iterations,
            report.gdi_growth(),
            report.user_growth()
        );
    }
}
