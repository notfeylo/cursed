# CursorForge

**Give your dead cursor a new life.**

A ~10 MB Windows app that replaces every pointer role with a crisp,
multi-resolution cursor scheme, turns any image you drop on it into a real
`.cur` or `.ani`, and stops Windows quietly reverting it.

No administrator rights. No overlay. No added input latency.

---

## Why

Changing a Windows cursor today means burrowing into Settings → Bluetooth &
devices → Mouse → Additional mouse settings → Pointers, and changing 17 roles
one at a time in a 1998-era file dialog. Third-party packs arrive as zips of
loose `.cur` files, replace only the arrow, ship a single 32×32 bitmap that
turns to mush at 150% DPI, and get wiped by the next theme change.

CursorForge fixes all of that in one window.

## The one architectural rule

> **CursorForge never draws a cursor. It only tells Windows which cursor to
> draw.**

The obvious approach — a transparent always-on-top window that tracks the mouse
and paints a sprite — is permanently laggy. It is composited by DWM at the
monitor's refresh interval, so it trails the real pointer by 8–33 ms forever, it
breaks in exclusive fullscreen, and it fights with games.

The Windows system cursor is drawn by the GPU's dedicated hardware cursor plane.
It updates at the mouse's polling rate with zero compositing latency, and it is
DPI-aware for free. So every cursor CursorForge produces is a real `.cur` /
`.ani` file registered with Windows. Zero added input latency is not a target
here; it is a property of the design.

## What it does

- **116 full schemes**, each defining **all 17 pointer roles** — not just the
  arrow, so you never end up with a neon pointer and a stock hourglass.
- **Every size, sharp.** Catalog cursors are rendered from vectors at 32, 48,
  64, 96, 128, 160, 192 and 256 px into one multi-resolution `.cur`.
- **Any colour.** Artwork ships as greyscale masters and is tinted at apply
  time, so 116 packs × any colour costs the installer nothing.
- **Drop a PNG.** Hotspot picker with alpha-centroid and tip-detect, a 1:1
  preview of all eight sizes, and a real cursor in under three seconds. GIF and
  APNG become real animated `.ani` files.
- **Blend.** Your own arrow over a catalog pack for the other sixteen roles —
  the mode that makes one image into a coherent pointer set.
- **It stays.** A watchdog notices theme changes, personalisation resets and
  other cursor tools, and puts your scheme back.
- **One-click undo.** The original scheme is snapshotted before anything
  changes, and the uninstaller replays it automatically.

## Install

Download `CursorForge_1.0.0_x64-setup.exe` from
[Releases](https://github.com/notfeylo/cursorforge/releases) and double-click it.

Per-user install under `%LOCALAPPDATA%\CursorForge` — **no UAC prompt**, no
terminal, no PowerShell.

## Build from source

Needs Rust (stable, MSVC toolchain), Node 20+, and the Visual Studio C++ build
tools.

```bash
npm install
npm run tauri dev      # run it
npm run tauri build    # produces the NSIS .exe and .msi
```

Useful extras:

```bash
npm run generate:packs                  # export catalog SVG masters to assets/packs
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
```

## What it touches

| Location                            | What                                       |
| ----------------------------------- | ------------------------------------------ |
| `HKCU\Control Panel\Cursors`         | The 17 pointer roles and the scheme name   |
| `HKCU\Control Panel\Cursors\Schemes` | The named scheme, so it appears in Windows |
| `%APPDATA%\CursorForge`              | Settings, presets, custom and cached cursors |

`HKEY_LOCAL_MACHINE` is never written. No driver, no service, no injected DLL,
no `SetWindowsHookEx`. See [SECURITY.md](SECURITY.md).

## Known limitations — stated honestly

1. **Apps that draw their own cursor are unaffected.** Many games using raw
   input, some remote-desktop sessions, and a few hardware-accelerated canvases
   render their own pointer. Overriding those needs process injection, which
   trips anti-cheat and AV heuristics. CursorForge will not do it. This is an
   explicit non-goal.
2. **Cursor trails and click ripples need an overlay**, which the architecture
   rules out. Deferred, and if they ever ship it will be behind a clearly
   labelled "adds latency" toggle.
3. **Secure-desktop surfaces** — the UAC prompt, Ctrl+Alt+Del, the sign-in
   screen — always use the system defaults. Expected and correct.
4. **Animated cursors are capped at 60 frames and 4 seconds.** Beyond that the
   shell's own animation cost becomes visible.
5. **Windows only.** macOS has no supported system-cursor API at all.

## Documentation

- [ARCHITECTURE.md](docs/ARCHITECTURE.md) — how the three layers fit together
- [CURSOR_FORMAT.md](docs/CURSOR_FORMAT.md) — the `.cur` / `.ani` byte layouts
- [TERMS.md](docs/TERMS.md) · [PRIVACY.md](docs/PRIVACY.md) — also rendered
  in-app, offline

## Privacy

CursorForge collects nothing. No analytics, no telemetry, no account. The only
network request it can make is an update check against GitHub Releases, and it
can be turned off. See [PRIVACY.md](docs/PRIVACY.md).

## Licence

MIT — © 2026 feylo. All bundled cursor artwork is original work, licensed
alongside the source.

CursorForge is not affiliated with, endorsed by, or sponsored by Microsoft.
