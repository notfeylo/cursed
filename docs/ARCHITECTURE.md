# Architecture

## The rule everything else follows

> **Cursed never draws a cursor. It only tells Windows which cursor to
> draw.**

This is the load-bearing decision, so it is worth stating why the obvious
alternative is wrong.

A transparent, always-on-top layered window that tracks the mouse and paints a
sprite is the approach most people reach for. It is composited by DWM at the
monitor's refresh interval, so it trails the real pointer by one to two frames —
8 to 33 ms — permanently, on any hardware. It breaks in exclusive fullscreen. It
flickers across monitors with different refresh rates. It fights with games. And
it burns GPU cycles for as long as it runs.

The Windows system cursor is drawn by the GPU's dedicated **hardware cursor
plane**. It updates independently of the frame buffer, at the mouse's polling
rate, with no compositing step at all. It does not care whether the mouse is
400 DPI or 32,000, whether it polls at 125 Hz or 8 kHz, or what the system is
doing. And it is DPI-aware for free.

So every cursor in Cursed is a real `.cur` or `.ani` file registered with
Windows. "Zero added input latency" is not a performance target that could
regress — it is a property of handing off to the same code path Windows uses for
its own arrow.

## Three layers

```
              ┌──────────────────────────────────────────┐
   hover  ──► │ A. Live layer      SetSystemCursor        │  instant, session-only
              ├──────────────────────────────────────────┤
   commit ──► │ B. Scheme layer    HKCU\...\Cursors       │  survives reboot
              ├──────────────────────────────────────────┤
              │ C. Watchdog        WM_SETTINGCHANGE+poll  │  survives Windows
              └──────────────────────────────────────────┘
```

### A — Live layer (`cursor/engine.rs`)

`SetSystemCursor(hcur, id)` for each role. No registry write, no broadcast, so
it is free enough to run on catalog hover with a 120 ms debounce. Reverted with
`SystemParametersInfoW(SPI_SETCURSORS, …)`, which reloads everything from the
registry.

**The trap this layer exists to contain:** `SetSystemCursor` *takes ownership of
and destroys* the handle it is given. Passing the handle you loaded leaves you
holding a dangling handle; reusing one handle across roles corrupts the
session's cursor table. So for every role, on every apply:

```rust
let original = LoadImageW(…)?;            // we own this
let copy = CopyIcon(HICON(original.0))?;  // SetSystemCursor may destroy this
SetSystemCursor(HCURSOR(copy.0), ocr_id)?;
DestroyCursor(HCURSOR(original.0))?;      // and we free ours
```

Hover previews build and set **only the arrow**. Building all seventeen roles on
every hover would turn a debounce into visible lag, and the arrow is the pointer
the user is actually looking at while browsing.

**`SetSystemCursor` accepts only fourteen ids.** The documented `OCR_*` values
cover fourteen of the seventeen roles. `NWPen`, `Pin` and `Person` are `IDC_*`
resource ids — genuine scheme roles that Windows writes to the registry and
honours after a reload, but that it refuses as a live in-session override.

Treating those refusals as failures is a trap worth naming, because the symptom
does not point at the cause: every apply returns an error, so the code that
records *what* was applied never runs, so nothing persists across launches and
the watchdog has nothing to protect — while the cursor visibly changes and looks
fine. The three are marked best-effort on the live layer, and the registry write
is what carries them.

For the same reason, `commit` records the applied state **before** attempting the
live layer and downgrades a partial live-layer result to a logged warning. Once
the registry write has succeeded the choice has been made; forgetting it because
the in-session override was incomplete would be strictly worse than reporting it.

### B — Scheme layer (`cursor/scheme.rs`)

Writes `HKEY_CURRENT_USER\Control Panel\Cursors` — 17 `REG_EXPAND_SZ` values,
the scheme's display name, and `CursorBaseSize` — then broadcasts with
`SPI_SETCURSORS | SPIF_UPDATEINIFILE | SPIF_SENDCHANGE`.

HKCU only. No administrator rights are involved anywhere in the product.

Paths are stored in their `%APPDATA%\…` form rather than expanded, so they keep
working if the profile moves. The scheme is also registered under
`Control Panel\Cursors\Schemes`, which is what makes it appear as a selectable
scheme in Control Panel → Mouse → Pointers instead of showing as "(Modified)".

A role the set does not define has its value **deleted** — that is how Windows
spells "use the built-in cursor for this role", and it is what makes "apply to
the arrow only" mean the arrow only rather than leaving fifteen stale paths
behind.

### C — Watchdog (`cursor/watchdog.rs`)

A hidden window plus a periodic check. If `HKCU\...\Cursors\Arrow` stops
matching what we committed — theme swap, personalisation reset, a Windows
Update repair pass, another cursor tool — the scheme is silently re-applied.

Two details worth knowing:

- The listener is **not** an `HWND_MESSAGE` window. Message-only windows are
  excluded from broadcast messages, so an `HWND_MESSAGE` parent would never
  receive `WM_SETTINGCHANGE` and the theme-change trigger would silently never
  fire. A never-shown `WS_EX_TOOLWINDOW` top-level window does receive
  broadcasts while staying out of the taskbar, Alt-Tab and the Z-order.
- The thread parks in `MsgWaitForMultipleObjectsEx`. It does not spin, poll a
  queue, or hold a timer callback. Idle cost is one string comparison every few
  seconds.

## Safety net

`cursor/restore.rs` captures the complete pre-existing scheme to
`%APPDATA%\Cursed\backup\original_scheme.json` **before the first write**,
and never re-captures — re-capturing on a later launch would overwrite the
user's real defaults with ours and turn "restore" into a no-op.

Settings → Restore Windows Default replays that snapshot. So does the
uninstaller, via `Cursed.exe --restore-defaults`, before it deletes
anything.

## Rendering

`packs/art.rs` describes all 17 role glyphs as parametric SVG; `packs/styles.rs`
holds the 216 packs as parameter sets. Nothing is a shipped bitmap.

At apply time each role is rendered from the vector **at every target size** —
32, 48, 64, 96, 128, 160, 192, 256 — tinted, optionally outlined, and packed
into one multi-resolution `.cur`. Rendering per size rather than resampling one
master is what keeps 256 px genuinely sharp.

Two details that decide whether a cursor looks right:

- **Hotspots are normalised (0.0–1.0)** and multiplied by the target size. An
  absolute pixel hotspot is correct at exactly one size and wrong at the other
  seven.
- **Downscaling is premultiplied.** Resampling straight alpha blends edge pixels
  against transparent black, which is where the dark halo on glowing artwork
  comes from.
- **The contrast outline is drawn per size**, after resampling, so it is always
  exactly one device pixel. Drawn before, it would vanish at 32 px and go chunky
  at 256 px.

Results are cached under `cache/<pack>/<tint>-<size>-<outline>/`. The first
apply of a combination does real work across a thread per core; every later one
is a directory listing.

## Idle footprint

Cursed spends nearly all of its life hidden in the tray, so `idle.rs` calls
`SetProcessWorkingSetSize(-1, -1)` a couple of seconds after the window is
hidden. That is the documented way to tell Windows "I am idle, reclaim what you
like" — nothing is freed or invalidated, the pages simply stop being resident and
fault back in if the window is reopened.

The delay matters: hiding a window kicks off teardown inside the webview, and
trimming while that is still running just pulls the same pages straight back in.

Measured on a 24-core machine, hidden in the tray: **0.000% CPU**, and the
working set drops from roughly 75 MB to a fraction of that. Without the trim the
process sits on the whole webview working set for the rest of the session, which
is memory a tray app has no business holding.

## Session state

The registry persists the *cursor*; `session.rs` persists the *reason* — which
pack, which tint, which size — in `applied.json`. Without it a fresh launch
would have no idea what "correct" means and the watchdog could not tell a theme
reset from the user's own choice.

Startup **adopts** rather than re-applies: the scheme is already live, so
re-writing it on every sign-in would mean a pointless registry write and a
system-wide broadcast for no visible change.

## Module map

```
src-tauri/src/
├── main.rs           entry; also handles --restore-defaults for the uninstaller
├── lib.rs            plugins, setup, window lifecycle
├── commands.rs       the ONLY #[tauri::command] surface
├── error.rs          AppError — one type across the IPC boundary
├── paths.rs          the only place that decides what a legal path is
├── session.rs        what is applied, and why, across launches
├── custom.rs         staging and building cursors from user images
├── shell.rs          two allow-listed shell affordances
├── updates.rs        the one network request, on WinHTTP
├── tray.rs / hotkeys.rs / autostart.rs / window_state.rs
├── cursor/
│   ├── roles.rs      17 roles ↔ registry value ↔ OCR id (the IPC boundary)
│   ├── engine.rs     live layer, CopyIcon discipline
│   ├── scheme.rs     HKCU scheme layer
│   ├── watchdog.rs   drift detection
│   └── restore.rs    snapshot and restore
├── build/
│   ├── cur_writer.rs ICONDIR / ICONDIRENTRY / BGRA DIB / AND mask
│   ├── ani_writer.rs RIFF ACON / anih / rate / seq / fram
│   ├── bitmap.rs     resample, trim, tint, outline
│   ├── svg.rs        resvg rasterisation
│   ├── hotspot.rs    centroid / tip-detect / manual
│   └── pipeline.rs   decode guards and the build steps
├── packs/
│   ├── art.rs        17 parametric role glyphs
│   ├── styles.rs     the 216 packs
│   ├── catalog.rs    render, cache, export
│   └── cfpack.rs     import/export with strict validation
└── state/
    ├── settings.rs
    └── presets.rs
```

## Frontend

React 18 + TypeScript (strict, no `any`) + Vite + Tailwind, with a single
Zustand store and plain view state instead of a router — there are six screens.

The webview holds no privileged capability. It cannot read a file, run a shell
command, or make an HTTP request; it can only call the typed commands in
`commands.rs`. See [SECURITY.md](../SECURITY.md).

## A permanently burned name

The repository was renamed `cursorforge` → `cursed`. The release-feed path in
`updates.rs` is compiled into the binary, so every 1.6.x and 1.7.0 install in the
world is still asking GitHub for `notfeylo/cursorforge/releases/latest`.

Those installs keep working purely because GitHub redirects the old name. **That
redirect is destroyed the instant anything is created under `cursorforge`
again** — a repo, a fork restored under that name, anything. The failure is
silent: no error the user sees, the update check simply stops finding releases.

So the name is burned permanently, and this is the only reason. Current builds
point at `notfeylo/cursed` directly and do not depend on the redirect.
