# Resource use under repetition

**Covers the handle-leak half of §8 of the research brief.** Updated in place.

The brief calls a handle leak "the most likely cause of long-session crashes in
an app that manipulates cursor handles continuously", and it is right to: the
per-process GDI limit is 10,000, a leak of one handle per apply is invisible for
hours, and when it finally matters the app fails somewhere with no connection to
the cause.

Run it with:

```bash
cargo run --manifest-path src-tauri/Cargo.toml --release --bin genpacks -- --stress-handles 5000
```

## Cursor handle discipline — 2026-08-15, v1.20.0 + working tree

| | |
| --- | --- |
| Machine | Windows 11 Home, build 26200, x86_64 |
| Loads | 20,000 (5,000 iterations × 4 files) |
| Files | `aero_arrow.cur`, `aero_link.cur` (`LoadImageW`), `aero_busy.ani`, `aero_working.ani` (`LoadCursorFromFileW`) |
| GDI objects | 4 → 4 (**+0**) |
| USER objects | 1 → 1 (**+0**) |
| Failures | 0 |

**Result: clean.** The counts did not move at all across twenty thousand
load-and-release cycles, so `cursor::engine`'s ownership rules hold on both
paths — the static one through `LoadImageW`, and the animated one through
`LoadCursorFromFileW`, which is the path that has been got wrong before.

Stock Windows cursors are used rather than generated ones deliberately: a leak
that only appears against our own output is a narrower result than one measured
against files nobody here wrote.

### What this does not cover

- **`SetSystemCursor` is never called.** The harness exercises load and release,
  not install. `SetSystemCursor` takes ownership of the handle it is given and
  destroys it, so the rule under test is on our side of that call — but a leak
  in the *apply* path specifically, rather than the load path, would not be seen
  here. Testing that means installing system cursors thousands of times, which
  fights the running app and the person at the keyboard for coverage of one
  extra line.
- **Not a 24-hour run.** The brief asks for one, sampling threads, working set,
  private bytes and open file handles as well. This measures two counters over
  minutes.
- **The rest of §8 is untouched:** the 500-apply / 200-click / 100-scroll abuse
  matrix, the catalog scroll frame graph, and the environment sweep
  (mixed-DPI, monitor hot-plug, RDP, fast user switching, high contrast).

The number above is real and the method is repeatable. It is one row of §8, not
§8.

---

## The soak — 2026-08-15, 11h50m

**Covers the 24-hour half of §8, at half the hours.** The run was stopped at
11 hours 50 minutes rather than reaching 24. Recorded as what it is: a long run,
not the run that was asked for.

```bash
npm run soak -- 1440 docs/verification/soak/soak-2026-08-15.csv
```

Runs the app's repeated work in a loop for a day, sampling seven counters every
minute into a CSV. Rows are flushed as they are taken, so a reboot or a closed
laptop still leaves everything measured up to that point — a soak whose results
only exist in memory produces nothing the first time anything goes wrong, which
on a twenty-four hour run is most of the time.

Each cycle: four cursor loads and releases, one image decoded and built into a
multi-resolution `.cur`, and one state file written and read back through the
durable store. A different image every cycle, so nothing downstream can cache
its way to a flat line.

### The result

711 samples over 42,600 seconds, one a minute. The warm-up sample at t=0 is
excluded from the comparison for the reason it is taken separately: every
subsystem allocates something long-lived the first time it is used, and counting
that as growth reports a leak on every clean run.

| | Min | Max | First hour | Last hour | Drift |
| --- | --- | --- | --- | --- | --- |
| GDI objects | 4 | 4 | 4 | 4 | **0** |
| USER objects | 1 | 1 | 1 | 1 | **0** |
| Threads | 1 | 3 | 1 | 1 | 0 |
| Open handles | 117 | 118 | 118 | 118 | **0** |
| Working set | 12.83 MB | 14.48 MB | 13.80 MB | 13.93 MB | +0.13 MB |
| Private bytes | 2.62 MB | 4.26 MB | 3.91 MB | 3.94 MB | +0.03 MB |

**Both GUI counters never moved.** Not "returned to where they started" —
across nearly twelve hours and every cycle in them, `GetGuiResources` reported
the same two numbers every single time. Handles moved by one, in the first
minute, and never again.

Memory drifted by 0.13 MB and 0.03 MB between the first hour's mean and the
last's. That is not a slope: the working set oscillates over a 1.65 MB range
minute to minute, so a 0.13 MB difference between two hourly means is inside the
noise by an order of magnitude. Over 11.8 hours a real per-cycle leak of even
one kilobyte would be visible as a line; there is no line.

**Read the slope, not the endpoints.** A number that tracks the cycle count is a
leak. Nothing here tracks anything.

### What is still owed

The run was stopped at 11h50m, so the 24-hour row is **half done**. Twelve hours
is long enough to see any leak that matters — a per-cycle leak is linear and
would have been unmistakable — but it is not what §8 asks for, and half a run
recorded as a whole one is how a gap becomes permanent. Re-run with:

```bash
npm run soak -- 1440 docs/verification/soak/soak-<date>.csv
```

The CSV from this run is not committed: it is 23 KB of samples regenerated on
demand, and `docs/verification/soak/*.csv` is git-ignored for that reason.

**Read the slope, not the endpoints.** A number that tracks the cycle count is
a leak; a number that rises once and stops is a subsystem waking up.

### What the soak deliberately leaves out

- **`SetSystemCursor` and the registry.** The same reason the handle harness
  leaves them out, and a stronger one: this runs for hours, and a harness that
  spent those hours applying cursors would fight the released copy of the app,
  the watchdog, and the person trying to use the machine.
- **Everything that needs the UI.** The brief's 200 clicks per control, 100
  catalog scrolls, 100 window/tray cycles and 60 seconds of continuous resize
  all need a driven GUI. There is no UI automation harness in this project and
  building one is a larger piece of work than the rows it would fill.
- **The frame graph.** Catalog scroll performance and the 60 fps target need a
  profiler attached to a running window, which is the same missing harness.

---

## Parsers under damage — 2026-08-15, working tree

**Covers the fuzzing half of §2.4.** Runs on every push; `src-tauri/src/fuzz.rs`.

| | |
| --- | --- |
| Method | seeded mutation of valid inputs, in the ordinary test suite |
| Property | no input may panic |
| Inputs per run | ~24,600 across six parsers |
| Panics found | **0** |
| Wall clock | ~14 s of the suite's 22 s |

| Parser | Iterations | Result |
| --- | --- | --- |
| `pipeline::sniff` | 4,000 × 4 seeds | clean |
| `pipeline::decode` | 600 × 3 seeds | clean |
| `.cfpack` manifest | 4,000 × 3 seeds | clean |
| `settings.json` (parse + `sanitised`) | 4,000 | clean |
| `presets.json` | 4,000 | clean |
| `original_scheme.json` | 4,000 | clean |
| `paths::validate_relative` | 4,000 × 4 seeds | clean |

Six mutation strategies: bit flips, truncation, run overwrite, insertion,
deletion, and a hostile length field written across the header. The seed is
fixed, so a failure names an input that can be reproduced.

### What this does not cover

- **`.cur` and `.ani` decoding**, deliberately. `build::cur_reader` hands the
  path to `LoadImageW`; fuzzing it would be fuzzing Windows' own loader from a
  test suite, on the developer's live session.
- **The zip layer**, as opposed to the manifest inside it. `cfpack::import` and
  `backup::import` are checked by unit tests against specific hostile entries —
  traversal, executables, reserved names, expansion ratio — rather than by
  mutation. A generated archive is a much bigger harness than the guards it
  would be testing.
- **`cargo-fuzz` proper**, with coverage guidance. It needs a nightly toolchain
  and libFuzzer support the pinned Windows MSVC toolchain does not have.
  `SECURITY.md` names the entry points for anyone with a Linux box.

Zero panics is the expected result and not a strong one on its own — it is the
floor. What makes it worth recording is that it now runs on every push, so the
floor stays where it is.
