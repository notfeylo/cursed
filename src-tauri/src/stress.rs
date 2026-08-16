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

// ── the soak ─────────────────────────────────────────────────────
//
// The handle run above answers one question thoroughly: does loading and
// releasing a cursor leak a handle. A soak answers a different one — does
// anything at all drift over hours of ordinary work — and it answers it worse,
// because "ordinary work" has to be simulated and every number it samples moves
// for reasons that are not leaks.
//
// It is still worth running, for a reason specific to this app: Cursed lives in
// the tray for weeks. A leak of one anything per apply is invisible in every
// test written so far and fatal on day eleven. The only way to see that is to
// do the work for a long time and watch the numbers.

/// One reading of everything worth watching.
#[derive(Debug, Clone, Copy)]
pub struct Sample {
    /// Seconds since the run started.
    pub at_secs: u64,
    pub gdi: u32,
    pub user: u32,
    pub threads: u32,
    pub handles: u32,
    /// Bytes.
    pub working_set: u64,
    pub private: u64,
}

impl Sample {
    pub fn header() -> &'static str {
        "seconds,gdi,user,threads,handles,working_set_bytes,private_bytes"
    }

    pub fn as_csv(&self) -> String {
        format!(
            "{},{},{},{},{},{},{}",
            self.at_secs, self.gdi, self.user, self.threads, self.handles, self.working_set,
            self.private
        )
    }
}

/// Reads every counter at once, so a row is one moment rather than seven.
pub fn sample(at_secs: u64) -> Sample {
    let (gdi, user) = gui_handles();
    let (working_set, private) = memory();
    Sample {
        at_secs,
        gdi,
        user,
        threads: thread_count(),
        handles: handle_count(),
        working_set,
        private,
    }
}

/// Working set and private bytes.
///
/// Private bytes is the number that matters for a leak: the working set falls
/// when Windows trims the process, which happens whenever the machine is under
/// memory pressure and has nothing to do with whether this app is holding on to
/// anything. A working set that stays flat while private bytes climb is a leak
/// that looks like health.
fn memory() -> (u64, u64) {
    #[cfg(windows)]
    {
        use windows::Win32::System::ProcessStatus::{
            GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX,
        };
        use windows::Win32::System::Threading::GetCurrentProcess;

        let mut counters = PROCESS_MEMORY_COUNTERS_EX::default();
        // SAFETY: the struct is a stack local sized by `cb` as the API requires,
        // and the EX form is layout-compatible with the base one — which is why
        // the cast is the documented way to call this.
        let ok = unsafe {
            GetProcessMemoryInfo(
                GetCurrentProcess(),
                &mut counters as *mut _ as *mut PROCESS_MEMORY_COUNTERS,
                std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32,
            )
        };
        if ok.is_ok() {
            return (counters.WorkingSetSize as u64, counters.PrivateUsage as u64);
        }
        (0, 0)
    }
    #[cfg(not(windows))]
    {
        (0, 0)
    }
}

/// Open kernel handles. A thread, file or event never closed shows up here and
/// nowhere else — GDI and USER counters do not see them.
fn handle_count() -> u32 {
    #[cfg(windows)]
    {
        use windows::Win32::System::Threading::{GetCurrentProcess, GetProcessHandleCount};
        let mut count = 0u32;
        // SAFETY: writing a `u32` we own, through the current-process
        // pseudo-handle, which needs no cleanup.
        let ok = unsafe { GetProcessHandleCount(GetCurrentProcess(), &mut count) };
        if ok.is_ok() {
            count
        } else {
            0
        }
    }
    #[cfg(not(windows))]
    {
        0
    }
}

fn thread_count() -> u32 {
    #[cfg(windows)]
    {
        use windows::Win32::System::Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, Thread32First, Thread32Next, TH32CS_SNAPTHREAD, THREADENTRY32,
        };
        let us = std::process::id();
        // SAFETY: the snapshot is owned and closed at the end of the scope, and
        // the entry is a stack local sized by `dwSize` as the API requires.
        unsafe {
            let Ok(snapshot) = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) else {
                return 0;
            };
            let snapshot = windows::core::Owned::new(snapshot);
            let mut entry = THREADENTRY32 {
                dwSize: std::mem::size_of::<THREADENTRY32>() as u32,
                ..Default::default()
            };
            if Thread32First(*snapshot, &mut entry).is_err() {
                return 0;
            }
            let mut count = 0u32;
            loop {
                if entry.th32OwnerProcessID == us {
                    count += 1;
                }
                if Thread32Next(*snapshot, &mut entry).is_err() {
                    break;
                }
            }
            count
        }
    }
    #[cfg(not(windows))]
    {
        0
    }
}

/// What one cycle of the soak does, and how many times it did it.
#[derive(Debug, Default, Clone, Copy)]
pub struct Work {
    pub cursor_loads: u64,
    pub image_decodes: u64,
    pub cursors_built: u64,
    pub state_round_trips: u64,
    pub failures: u64,
}

/// One pass of everything the app does repeatedly, without touching the
/// machine's actual pointer.
///
/// **`SetSystemCursor` and the registry are deliberately absent**, for the same
/// reason `run_handle_stress` leaves them out and a stronger one: a soak runs
/// for hours, and a harness that spent those hours applying cursors would fight
/// the released copy of the app, the watchdog, and the person trying to use the
/// machine. What it costs is coverage of the apply path specifically; what it
/// buys is a harness anybody will actually leave running.
fn one_cycle(work: &mut Work, files: &[PathBuf]) {
    // The load and release path, which is the one with a known-hard ownership
    // rule.
    for file in files {
        match crate::cursor::engine::verify_loadable(file) {
            Ok(_) => work.cursor_loads += 1,
            Err(_) => work.failures += 1,
        }
    }

    // The import pipeline, end to end: decode, matte, resample, write a real
    // `.cur`. This is the heaviest thing the app does and the one most likely
    // to hold on to something.
    let png = synthetic_png(64, 64, work.image_decodes as u8);
    match crate::build::pipeline::decode(png) {
        Ok(source) => {
            work.image_decodes += 1;
            match source
                .first()
                .ok()
                .cloned()
                .and_then(|bitmap| crate::build::pipeline::prepare_master(&bitmap).ok())
            {
                Some(master) => {
                    let sizes =
                        crate::build::pipeline::sizes_for_source(master.width, master.height);
                    let options = crate::build::pipeline::Finish::default();
                    if crate::build::pipeline::build_cur(&master, (0.5, 0.5), &options, &sizes)
                        .is_ok()
                    {
                        work.cursors_built += 1;
                    } else {
                        work.failures += 1;
                    }
                }
                None => work.failures += 1,
            }
        }
        Err(_) => work.failures += 1,
    }

    // The state layer, which every setting change and every preset save goes
    // through. Written to a scratch file rather than the real data directory:
    // a soak must not be able to damage the thing it is protecting.
    let scratch = std::env::temp_dir()
        .join("cursorforge-soak")
        .join("state.json");
    let settings = crate::state::settings::Settings::default();
    if let Ok(json) = serde_json::to_string(&settings) {
        if crate::state::store::write(&scratch, &json).is_ok() {
            let (_read, _source) =
                crate::state::store::read::<crate::state::settings::Settings>(&scratch);
            work.state_round_trips += 1;
        } else {
            work.failures += 1;
        }
    }
}

/// A different image every time, so the decoder is not handed the same bytes
/// twice and nothing downstream can be caching its way to a flat line.
fn synthetic_png(width: u32, height: u32, seed: u8) -> Vec<u8> {
    let mut image = image::RgbaImage::new(width, height);
    for (x, y, pixel) in image.enumerate_pixels_mut() {
        let on = ((x / 4) + (y / 4) + seed as u32) % 2 == 0;
        *pixel = if on {
            image::Rgba([seed, 40u8.wrapping_add(seed), 200, 255])
        } else {
            image::Rgba([255, 255, 255, 255])
        };
    }
    let mut out = std::io::Cursor::new(Vec::new());
    let _ = image::DynamicImage::ImageRgba8(image).write_to(&mut out, image::ImageFormat::Png);
    out.into_inner()
}

/// What a soak found.
#[derive(Debug, Clone)]
pub struct SoakReport {
    pub samples: Vec<Sample>,
    pub work: Work,
    pub cycles: u64,
}

impl SoakReport {
    pub fn first(&self) -> Option<&Sample> {
        self.samples.first()
    }

    pub fn last(&self) -> Option<&Sample> {
        self.samples.last()
    }

    /// Growth in each counter from the first sample to the last.
    pub fn growth(&self) -> Option<(i64, i64, i64, i64, i64, i64)> {
        let (a, b) = (self.first()?, self.last()?);
        Some((
            b.gdi as i64 - a.gdi as i64,
            b.user as i64 - a.user as i64,
            b.threads as i64 - a.threads as i64,
            b.handles as i64 - a.handles as i64,
            b.working_set as i64 - a.working_set as i64,
            b.private as i64 - a.private as i64,
        ))
    }

    pub fn csv(&self) -> String {
        let mut out = String::from(Sample::header());
        out.push('\n');
        for sample in &self.samples {
            out.push_str(&sample.as_csv());
            out.push('\n');
        }
        out
    }
}

/// Runs the work loop for `duration`, sampling every `interval`.
///
/// The first sample is taken *after* a warm-up cycle. Every subsystem here
/// allocates something long-lived the first time it is used — a decoder table, a
/// thread pool, whatever the loader touched — and counting that as growth would
/// report a leak on every clean run, which is how a check gets ignored.
pub fn run_soak(
    duration: std::time::Duration,
    interval: std::time::Duration,
    mut on_sample: impl FnMut(&Sample),
) -> SoakReport {
    let files = sample_files();
    let mut work = Work::default();

    one_cycle(&mut work, &files);
    work = Work::default();

    let started = std::time::Instant::now();
    let mut samples = vec![sample(0)];
    on_sample(&samples[0]);

    let mut next_sample = started + interval;
    let mut cycles = 0u64;

    while started.elapsed() < duration {
        one_cycle(&mut work, &files);
        cycles += 1;

        if std::time::Instant::now() >= next_sample {
            let taken = sample(started.elapsed().as_secs());
            on_sample(&taken);
            samples.push(taken);
            next_sample += interval;
        }
    }

    let taken = sample(started.elapsed().as_secs());
    on_sample(&taken);
    samples.push(taken);

    SoakReport {
        samples,
        work,
        cycles,
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

    /// Every counter the soak samples has to return something.
    ///
    /// A soak whose numbers are all zero reports a beautifully flat line for any
    /// leak whatsoever, which is worse than not running it — it is a graph that
    /// says everything is fine.
    #[test]
    fn every_sampled_counter_reports_something() {
        let taken = sample(0);
        assert!(taken.gdi > 0 || taken.user > 0, "GUI counters are dead");
        assert!(taken.threads > 0, "a running process has at least one thread");
        assert!(taken.handles > 0, "a running process has open handles");
        assert!(taken.working_set > 0, "working set reads as zero");
        assert!(taken.private > 0, "private bytes read as zero");
    }

    /// A short soak, so the loop itself is covered by the suite. The real run is
    /// `genpacks --soak`, which is not bound by how long a unit test may take.
    #[test]
    fn a_short_soak_does_work_and_produces_rows() {
        let report = run_soak(
            std::time::Duration::from_millis(400),
            std::time::Duration::from_millis(100),
            |_| {},
        );

        assert!(report.cycles > 0, "the soak did no work");
        assert!(report.samples.len() >= 2, "a growth figure needs two samples");
        assert!(report.work.image_decodes > 0, "the image path was never exercised");
        assert!(report.work.state_round_trips > 0, "the state path was never exercised");
        assert!(report.growth().is_some());

        // The CSV is what a long run leaves behind, so its shape matters as much
        // as the numbers.
        let csv = report.csv();
        assert!(csv.starts_with(Sample::header()));
        assert_eq!(csv.lines().count(), report.samples.len() + 1);
    }

    /// Each cycle must be handed different bytes, or the decoder is being asked
    /// the same question a hundred thousand times and any cache in the stack
    /// flattens the graph for free.
    #[test]
    fn the_synthetic_image_differs_between_cycles() {
        assert_ne!(synthetic_png(32, 32, 1), synthetic_png(32, 32, 2));
        assert_eq!(synthetic_png(32, 32, 7), synthetic_png(32, 32, 7));
    }
}
