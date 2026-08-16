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
