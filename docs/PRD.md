# The original product brief

The specification Cursed was built to, kept as written on 2026-08-07 rather than
edited into agreement with what shipped.

**Read it as history, not as instruction.** Where this and the rest of `docs/`
disagree, the rest of `docs/` is right — it describes the product that exists,
and this describes the one that was planned. Several things here were
deliberately overruled during the build and the reasoning is recorded elsewhere:

- The product and repository are **Cursed** / `notfeylo/cursed`, not CursorForge.
  §0's naming and §16's repo URL are both superseded; the old name is
  permanently burned and must never be reused (`CONTRIBUTING.md` explains why).
- The floor is **Windows 10 1803**, not 1809, and it is set by WebView2 rather
  than chosen.
- The catalog is **36 hand-made packs**, not a generated set. §7 describes the
  generated catalog that 1.18.0 removed.
- Releases ship **x64, ARM64 and 32-bit**, which §11 does not anticipate.

It is kept because the code cites it: several modules and CI steps reference a
section number ("PRD §13.4", "PRD §9", "PRD §12"), and a reference nobody can
follow is worse than no reference. The section numbering below is therefore
unchanged.

The closing build-instruction section has been dropped; everything that was
binding in it is now in `CONTRIBUTING.md` and `docs/ARCHITECTURE.md`.

---

## 0. NAMING

Working name: **CURSORFORGE** (short mark: **FORGE**).

Alternatives if `cursorforge` is taken on GitHub / npm / domain:
`Kursr` · `PointrX` · `MORPHCURSOR` · `NOVAPOINT` · `CURSR`

Tagline: **"Give your dead cursor a new life."**
Hero line inside the app: **ENHANCE YOUR CURSOR**

Repo: `github.com/feylo/cursorforge` — **public**, MIT license.

---

## 1. PROBLEM STATEMENT

Changing a Windows mouse cursor today is genuinely bad:

1. **The native path is buried and broken.** Settings → Bluetooth & devices → Mouse → Additional mouse settings → Pointers tab → change 17 individual roles one by one, browsing to `.cur` files in a 1998-era file dialog. Most people give up.
2. **Third-party cursor packs are hostile.** Downloads are `.zip` files full of loose `.cur` files with an `Install.inf` you have to right-click → Install. Half of them are broken, mis-sized, or bundle adware.
3. **Nothing survives.** Theme changes, Windows updates, and personalization resets silently wipe custom cursors back to default.
4. **Nothing scales.** At 150% / 200% DPI, or with Windows' cursor-size accessibility slider raised, third-party cursors turn into blurry mush because they ship a single 32×32 bitmap.
5. **There is no way to use your own image.** If you have a PNG of a crosshair, logo, or icon you want as your cursor, there is no consumer tool that converts it correctly — hotspot, alpha, multi-resolution, and all 17 roles.

**CursorForge is one small `.exe` that fixes all five.**

---

## 2. PRODUCT VISION

A ~10 MB desktop app the size of a browser extension window. Open it, click **ENHANCE YOUR CURSOR**, browse a beautiful catalog, click a cursor — it applies instantly with zero lag. Click **DONE**. It persists forever, through reboots, theme changes, and every application. Drag in a PNG and it becomes a real, correctly-built Windows cursor in under three seconds.

**Non-negotiables:**
- Zero added input latency. Ever.
- Pixel-perfect at every DPI and every cursor size.
- Runs silently in the background at ~0% CPU.
- Never requires admin. Never touches `HKLM`. Never injects into another process.
- Never leaves the machine in a broken state — one-click full restore.

---

## 3. THE CORE ARCHITECTURAL RULE (READ THIS FIRST)

> **CursorForge NEVER draws a cursor itself. It only tells Windows which cursor to draw.**

This is the single most important engineering decision in the product, and it is what separates it from every laggy competitor.

**Why an overlay is forbidden:**
A transparent always-on-top layered window that tracks the mouse and paints a cursor sprite is the obvious approach — and it is fundamentally, unfixably laggy. It is composited by DWM at the monitor's refresh interval, so it trails the real pointer by 1–2 frames (8–33 ms) permanently. It breaks in exclusive-fullscreen, it flickers across monitors with mixed refresh rates, it fights with games, and it burns GPU cycles forever.

**Why the OS cursor is correct:**
The Windows system cursor is drawn by the GPU's dedicated **hardware cursor plane**. It updates independently of the frame buffer, at the mouse's polling rate, with literally zero compositing latency. It is completely unaffected by mouse DPI (400 or 32,000), polling rate (125 Hz or 8 kHz), display refresh, or system load. It is DPI-aware for free.

**Therefore: every cursor in CursorForge is a real `.cur` or `.ani` file registered with Windows.** All "smoothness" requirements in this PRD are satisfied automatically and by definition, because we are handing off to the same code path Windows uses for its own arrow.

---

## 4. THE CURSOR ENGINE — TWO LAYERS

### 4.1 Layer A — Live Layer (instant preview, in-session)

Used while the user is browsing the catalog so that hovering/clicking a cursor changes the pointer **immediately** with no flicker and no registry write.

Win32: `SetSystemCursor(hcur, id)` from `user32.dll`, called for each of the 17 role IDs.

**CRITICAL IMPLEMENTATION TRAP — do not skip:**
`SetSystemCursor` **takes ownership of and destroys** the handle you pass it. Passing the same handle twice, or passing a handle you still hold a reference to, causes handle invalidation, silent failure, or GDI leaks that degrade the whole session.

**Correct pattern (Rust, `windows` crate):**
```rust
// For EVERY role, for EVERY apply:
let h = LoadImageW(None, &path_wide, IMAGE_CURSOR, w, h, LR_LOADFROMFILE | LR_DEFAULTSIZE)?;
let copy = CopyIcon(HICON(h.0))?;      // Give SetSystemCursor a copy it may destroy
SetSystemCursor(HCURSOR(copy.0), ocr_id)?;
DestroyCursor(HCURSOR(h.0))?;           // We own and free the original
```

Revert the live layer with:
`SystemParametersInfoW(SPI_SETCURSORS, 0, None, SPIF_flags(0))` — reloads every cursor from the registry.

### 4.2 Layer B — Scheme Layer (persistence, survives reboot)

Writes the pointer scheme into the user's registry so the cursor persists **even when CursorForge is not running and even after uninstall-adjacent scenarios**.

Key: `HKEY_CURRENT_USER\Control Panel\Cursors` (HKCU — **no admin required**)

Set `(Default)` = `"CursorForge — <PresetName>"` and these 17 `REG_EXPAND_SZ` values:

| Registry value | Role | OCR constant |
|---|---|---|
| `Arrow` | Normal select | `OCR_NORMAL` (32512) |
| `Help` | Help select | `OCR_HELP` (32651) |
| `AppStarting` | Working in background | `OCR_APPSTARTING` (32650) |
| `Wait` | Busy | `OCR_WAIT` (32514) |
| `Crosshair` | Precision select | `OCR_CROSS` (32515) |
| `IBeam` | Text select | `OCR_IBEAM` (32513) |
| `NWPen` | Handwriting | `OCR_UP`-adjacent (32631) |
| `No` | Unavailable | `OCR_NO` (32648) |
| `SizeNS` | Vertical resize | `OCR_SIZENS` (32645) |
| `SizeWE` | Horizontal resize | `OCR_SIZEWE` (32644) |
| `SizeNWSE` | Diagonal resize 1 | `OCR_SIZENWSE` (32642) |
| `SizeNESW` | Diagonal resize 2 | `OCR_SIZENESW` (32643) |
| `SizeAll` | Move | `OCR_SIZEALL` (32646) |
| `UpArrow` | Alternate select | `OCR_UP` (32516) |
| `Hand` | Link select | `OCR_HAND` (32649) |
| `Pin` | Location select | (Win10+ scheme value) |
| `Person` | Person select | (Win10+ scheme value) |

Then broadcast the change:
```rust
SystemParametersInfoW(
    SPI_SETCURSORS, 0, None,
    SPIF_UPDATEINIFILE | SPIF_SENDCHANGE
)?;
```

**Shipping all 17 roles for every catalog item is a hard requirement.** Competitors replace only the arrow, which leaves you with a neon cursor and a stock Windows I-beam and hourglass. Full-scheme coherence is a primary differentiator.

### 4.3 Layer C — The Watchdog (persistence insurance)

A lightweight Rust background task that guarantees the chosen cursor is never silently lost.

- Subscribe to `WM_SETTINGCHANGE` on a hidden message-only window (`HWND_MESSAGE`).
- Plus a cheap 5-second registry read of `HKCU\Control Panel\Cursors\Arrow`.
- If the value no longer matches the active preset (Windows theme swap, personalization reset, another cursor tool, a Windows Update repair), silently re-apply the scheme.
- Toggleable in Settings ("Protect my cursor", default **ON**).
- Cost: one string comparison every 5 s. Measured CPU: **0.0%**.

### 4.4 First-run safety snapshot

On very first launch, before touching anything, read all 17 existing registry values plus `CursorBaseSize` and the scheme `(Default)` name, and write them to `%APPDATA%\CursorForge\backup\original_scheme.json`.

- **Settings → "Restore Windows Default"** replays this snapshot exactly.
- The NSIS uninstaller runs the same restore automatically before removing files.
- Never leave a machine with dangling registry paths to deleted `.cur` files.

---

## 5. DPI, SCALING & SIZE CORRECTNESS

This is where 100% of competing cursor packs fail. Requirements:

1. **Every catalog cursor ships as a multi-resolution `.cur`.** The `ICONDIR` format supports multiple images in one file; Windows picks the closest match. Ship: **32, 48, 64, 96, 128, 160, 192, 256** px.
2. **Respect `CursorBaseSize`** (`HKCU\Control Panel\Cursors\CursorBaseSize`, `REG_DWORD`, 32–256). Windows' accessibility cursor-size slider writes here. Read it at apply time, and expose it as a slider in our own UI so users never need the Settings app again.
3. **Hotspots are stored normalized** (`0.0–1.0` floats) in the pack manifest, then multiplied by the target pixel size at build time. Storing an absolute pixel hotspot breaks at every size but one.
4. **Animated `.ani` files cannot be multi-resolution.** Generate one `.ani` per target size at import/build time and select the correct file when applying, based on the detected `CursorBaseSize` and primary-monitor DPI.
5. **Per-monitor DPI:** the OS handles cursor scaling across mixed-DPI monitors automatically *provided* the multi-res `.cur` contains the needed sizes. This is another reason overlays are forbidden — they do not get this for free.
6. **Rendering quality:** all downscaling uses Lanczos3 on **premultiplied alpha**, then un-premultiplies. Naive straight-alpha resizing produces dark halos on glowing/neon cursors.

---

## 6. THE PNG → CURSOR PIPELINE (KILLER FEATURE)

Drag in any PNG (or GIF / APNG / sprite sheet) and get a real Windows cursor. Fully offline, in Rust, no server.

### 6.1 Static PNG → `.cur`

1. **Ingest & validate.** Max 20 MB, max 4096×4096, must decode as PNG/JPEG/WebP/BMP. Reject anything else. Decode with the `image` crate under a 30-second timeout and an explicit pixel-count guard (decompression-bomb protection).
2. **Auto-trim** fully-transparent border rows/columns; preserve aspect ratio.
3. **Hotspot selection UI.** Draggable crosshair over a zoomable preview, with snap presets: `Center`, `Top-Left`, `Alpha Centroid` (default), `Tip Detect` (topmost-leftmost opaque pixel — correct for arrow-shaped images).
4. **Resample** to all 8 target sizes, premultiplied Lanczos3, with an optional 1 px dark outline pass for legibility on white backgrounds (toggle: "Add contrast outline", default ON).
5. **Write a real `.cur` file.** Not a renamed PNG. Structure:
   - `ICONDIR`: `idReserved=0`, **`idType=2`** (2 = cursor, 1 = icon), `idCount=8`
   - Each `ICONDIRENTRY`: `bWidth`, `bHeight`, `bColorCount=0`, `bReserved=0`, **`wPlanes` = hotspot X**, **`wBitCount` = hotspot Y** (cursor files reuse these two fields for the hotspot — this is the #1 thing hand-rolled converters get wrong), `dwBytesInRes`, `dwImageOffset`
   - Payload: `BITMAPINFOHEADER` with **`biHeight = 2 × height`** (XOR colour mask + AND transparency mask stacked), `biBitCount=32`, `biCompression=BI_RGB`, BGRA rows **bottom-up**, followed by a 1-bpp AND mask (row-padded to 4-byte boundaries) derived from the alpha channel.
6. **Verify before install.** Load the generated file back with `LoadImageW(..., IMAGE_CURSOR, ...)`; if it fails, surface a clear error instead of installing a broken cursor.

### 6.2 Animated GIF / APNG / sprite sheet → `.ani`

Write a real RIFF `ACON` container:

- `RIFF` → `ACON`
- `anih` chunk (36 bytes): `cbSize=36`, `cSteps`, `cFrames`, `cx=cy=cBitCount=cPlanes=0`, `jifRate` (default frame delay in jiffies), `flags = AF_ICON (0x1)`
- `LIST` `fram` containing one `icon` chunk per frame — **each `icon` chunk is a complete, valid `.cur` file** (same writer as 6.1, same hotspot)
- Optional `rate` chunk (per-frame delays) and `seq` chunk (playback order)
- Optional `INAM` / `IART` info chunks for pack name and author

**Timing:** jiffies = 1/60 second. Convert `delay_ms → round(ms × 60 / 1000)`, clamp to `1..=100`. Cap frames at **60** and total duration at 4 s to keep files small and playback smooth.

**Sprite sheets:** ask for columns × rows (or auto-detect square grid), slice, then treat as frames.

### 6.3 Role application for custom cursors

After building, the user picks which roles the custom cursor applies to:
- **Just the arrow** (default, safest)
- **Arrow + Link select + Precision** (recommended)
- **All 17 roles** (uses the same image everywhere — offer a warning)
- **Blend:** custom arrow + a chosen catalog pack for the remaining 16 roles ← *this is the smart default and nobody else does it*

---

## 7. THE CATALOG

**v1 target: 64 full schemes** (each = 17 roles × 8 sizes, generated from vector sources at build time).

| Category | Contents |
|---|---|
| **PRECISION** | Plus, Thin Cross, Dot, T-Cross, Chevron, Bracket, Micro-dot, Gap-cross |
| **NEON** | Glow arrow, Electric blue, Cyber magenta, Toxic green, Ember, Ice, Plasma, Vaporwave |
| **MINIMAL** | Hairline, Mono outline, Solid ink, Ghost, Paper, Bevel, Flat, Nano |
| **RETRO / PIXEL** | 8-bit arrow, Amiga, Mac Classic, CRT, DOS block, Game Boy, Terminal, Pixel hand |
| **GAMING** | 12 crosshair styles ported from the DEADEYE asset set, tuned for desktop use |
| **ANIMATED** | Pulse ring, Orbit dot, Ripple, Glitch, Scanline, Loading-spinner replacements for `Wait` / `AppStarting`, Breathing glow, Comet |
| **FUN** | Arcade, Holo, Liquid, Origami, Blade, Rune, Circuit, Sticker |

### 7.1 Runtime recolour — the catalog multiplier

All non-animated catalog assets ship as **white/greyscale masters**. A Rust HSV/tint pass recolours them at apply time to any user-chosen colour, with an optional opacity and outline setting.

**64 schemes × unlimited colours = an effectively infinite catalog from a ~4 MB asset payload.** This is how the installer stays small while the catalog looks enormous.

### 7.2 Catalog data model

`packs/<pack-id>/pack.json`
```json
{
  "id": "neon-plasma",
  "name": "PLASMA",
  "category": "NEON",
  "author": "feylo",
  "license": "MIT",
  "version": "1.0.0",
  "recolorable": true,
  "animated": false,
  "roles": {
    "Arrow":     { "src": "arrow.svg",  "hotspot": [0.06, 0.04] },
    "IBeam":     { "src": "ibeam.svg",  "hotspot": [0.50, 0.50] }
  }
}
```

---

## 8. SAVED PRESETS ("SAVE PANEL")

A preset is the complete, restorable state of the user's pointer.

```json
{
  "id": "uuid",
  "name": "GAMING",
  "created": "2026-08-07T00:00:00Z",
  "basePack": "precision-gap-cross",
  "overrides": { "Arrow": "custom/my-logo.cur" },
  "tint": "#2E8BFF",
  "size": 48,
  "outline": true,
  "hotkey": "Ctrl+Alt+2",
  "isDefault": false
}
```

UI: card grid with a live thumbnail, name, colour chip, size badge. Actions: **Apply**, **Set as default**, **Rename**, **Duplicate**, **Bind hotkey**, **Export `.cfpack`**, **Delete**.

**Export / import `.cfpack`** = a zip containing `manifest.json` + assets. This makes presets shareable, and it is the seed of a community marketplace in v2.

---

## 9. SETTINGS (SPEC — NOT PLACEHOLDER)

**General**
- Launch on Windows startup *(default ON)* — `tauri-plugin-autostart`, launches with `--silent`
- Start minimized to tray *(ON)*
- Close button minimizes to tray instead of quitting *(ON)*
- Show tray icon *(ON)*
- Check for updates automatically *(ON)*

**Cursor**
- Cursor size slider: 32 → 256 px, live preview *(default: inherit system)*
- Accent / tint colour picker + hex input
- Contrast outline *(ON)*
- Apply to: Arrow only / Recommended / All roles / Blend *(default: Blend)*
- Animation speed multiplier: 0.5× – 2.0× *(1.0×)*
- Re-apply on resume from sleep *(ON)*

**Protection**
- Protect my cursor (watchdog) *(ON)*
- Watchdog interval: 3 / 5 / 10 / 30 s *(5 s)*
- Re-apply after theme change *(ON)*

**Hotkeys**
- Global toggle: custom ↔ Windows default *(Ctrl+Alt+0)*
- Preset slots 1–5 *(Ctrl+Alt+1…5)*
- Open CursorForge *(Ctrl+Alt+C)*

**Advanced**
- Storage location + "Open folder"
- Cache size + "Clear generated cursors"
- Export all presets / Import
- Enable debug logging *(OFF)*
- **Restore Windows Default** *(destructive-styled button, confirm dialog)*

**About**
- Version, build hash, "Check for updates"
- Terms & Conditions, Privacy Policy, Licenses (all rendered in-app, offline)
- GitHub link, report a bug

---

## 10. UI / UX SPECIFICATION

### 10.1 Window

- **420 × 660 px**, resizable, min `400 × 600`, max `520 × 900` — deliberately extension-sized, not a full desktop app
- Frameless, custom title bar (drag region, minimize, close-to-tray), **12 px** corner radius
- Remembers position and size
- Always opens on the monitor containing the cursor

### 10.2 Design tokens

```
--bg            #050507   /* near-black canvas */
--surface       #0B0D12   /* cards */
--elevated      #131722   /* hover / modals */
--border        #1E2532
--border-hi     #2A3446

--accent        #2E8BFF   /* primary blue */
--accent-hi     #5CB8FF
--accent-dim    #0A2540
--accent-glow   rgba(46,139,255,0.28)

--text          #EDF1F7
--text-muted    #8A94A6
--text-dim      #58606E

--danger        #FF4D5E
--success       #33D6A6

radius: 4 / 8 / 12 / 16
shadow-glow: 0 0 24px var(--accent-glow)
```

### 10.3 Typography

- **Display / headings:** `Chakra Petch` — uppercase, `600/700`, `letter-spacing: 0.08em`. Technical, precise, slightly futuristic without being a gamer cliché.
- **Body / UI:** `Inter` — `400/500`, normal case.
- **Numeric / mono:** `JetBrains Mono` for sizes, hex values, hotkeys.
- **All fonts self-hosted as WOFF2 in `src/assets/fonts/`.** No Google Fonts CDN — required for offline use and for the strict CSP.

### 10.4 Screen flow

```
HOME  ─►  CATALOG  ─►  [click cursor = instant live apply]  ─►  DONE
  │
  ├──►  CUSTOM IMPORT  ─►  hotspot picker  ─►  preview  ─►  SAVE + APPLY
  ├──►  SAVED PRESETS  ─►  apply / edit / export
  └──►  SETTINGS  ─►  About / Legal
```

**HOME**
Centered, generous vertical space. Small animated cursor mark. Display type:
```
        ENHANCE
     YOUR CURSOR
```
Muted subline: *"Give your dead cursor a new life."*
Primary CTA: full-width **ENHANCE YOUR CURSOR** button — blue fill, subtle outward glow on hover, `translateY(-1px)`.
Below: current preset chip ("ACTIVE — PLASMA · 48px · #2E8BFF") and three ghost buttons: `CUSTOM` · `SAVED` · `SETTINGS`.

**CATALOG**
- Sticky header: back chevron, search field, category pills (horizontal scroll)
- 3-column grid of tiles: dark card, cursor rendered large and centered, name in display type on hover, animated badge if `.ani`
- **Hover = live preview applied to the real system cursor** (debounced 120 ms). Move away = reverts. Click = commits.
- Bottom sticky bar: colour swatch row + size slider + **DONE** button
- Empty search state with a suggestion, never a blank grid

**CUSTOM IMPORT**
- Full-panel drop zone with dashed border, `+` glyph, "Drop a PNG — or click to browse"
- After drop: left = zoomable preview with draggable hotspot crosshair; right = the 8 generated sizes rendered at 1:1 so the user sees exactly what they will get
- Role-application selector, outline toggle, name field
- `BUILD & APPLY` primary button; progress is instant (<3 s), so use a subtle shimmer, not a spinner

**SAVED**
Card grid, `+ NEW FROM CURRENT` as the first tile. Default preset carries a small accent bar.

### 10.5 Motion

- 120–180 ms, `cubic-bezier(0.16, 1, 0.3, 1)`. No bounce, no spring, no page-slide.
- Screen transitions: 8 px vertical fade only.
- Respect `prefers-reduced-motion` — disable all non-essential motion.
- Never animate anything while a live cursor preview is active (avoid perceived stutter).

---

## 11. TECHNICAL STACK

### Frontend
| Concern | Choice |
|---|---|
| Framework | React 18 + TypeScript (strict) |
| Build | Vite 5 |
| Styling | Tailwind CSS + CSS variables from §10.2 |
| State | Zustand (single store, persisted slice) |
| Routing | Local view state (no router — 6 screens) |
| Icons | `lucide-react` |
| Motion | CSS transitions; `framer-motion` only if genuinely needed |

### Backend (Rust)
| Concern | Crate |
|---|---|
| Shell | `tauri` v2 |
| Win32 | `windows` (features: `Win32_UI_WindowsAndMessaging`, `Win32_Graphics_Gdi`, `Win32_System_Registry`, `Win32_UI_Shell`, `Win32_Foundation`) |
| Registry | `winreg` (or `windows` registry APIs directly) |
| Images | `image` + `fast_image_resize` |
| SVG raster | `resvg` + `tiny-skia` (build-time asset generation) |
| Serialization | `serde`, `serde_json` |
| Errors | `thiserror` + `anyhow` |
| Logging | `tauri-plugin-log` (file-rotating, off by default) |
| Zip (`.cfpack`) | `zip` |

### Tauri plugins
`autostart` · `single-instance` · `store` · `dialog` · `fs` · `updater` · `global-shortcut` · `process` · `log`

### Bundling
- `tauri build` → **NSIS `.exe` installer** (primary) + `.msi` (secondary)
- Per-user install (`%LOCALAPPDATA%\CursorForge`) → **no admin prompt**
- Desktop shortcut + Start Menu entry, created by default
- Uninstaller runs the cursor-restore routine before deleting files
- Target installer size: **< 12 MB**

### Rust module layout
```
src-tauri/src/
├── main.rs
├── lib.rs
├── cursor/
│   ├── mod.rs
│   ├── engine.rs       // SetSystemCursor live layer + CopyIcon discipline
│   ├── scheme.rs       // HKCU registry scheme layer, 17 roles
│   ├── watchdog.rs     // WM_SETTINGCHANGE + poll + re-apply
│   ├── roles.rs        // hardcoded role enum ↔ registry key ↔ OCR id
│   └── restore.rs      // snapshot + restore original scheme
├── build/
│   ├── cur_writer.rs   // ICONDIR / ICONDIRENTRY / BGRA DIB / AND mask
│   ├── ani_writer.rs   // RIFF ACON / anih / fram / rate / seq
│   ├── pipeline.rs     // decode → trim → resample → tint → outline → write
│   └── hotspot.rs      // centroid / tip-detect / manual
├── packs/
│   ├── catalog.rs
│   └── cfpack.rs       // export / import, strict validation
├── state/
│   ├── settings.rs
│   └── presets.rs
├── tray.rs
├── autostart.rs
├── hotkeys.rs
└── commands.rs         // the ONLY #[tauri::command] surface
```

---

## 12. PERFORMANCE BUDGET (ENFORCED, NOT ASPIRATIONAL)

| Metric | Target | Hard fail |
|---|---|---|
| Cold start → interactive | < 1.2 s | > 2.5 s |
| Catalog scroll | 60 fps | dropped frames |
| Cursor apply latency | < 80 ms | > 200 ms |
| PNG → cursor build (8 sizes) | < 3 s | > 8 s |
| Idle CPU (tray, watchdog on) | 0.0 – 0.1 % | > 0.5 % |
| Idle RAM (window hidden) | < 25 MB | > 60 MB |
| RAM (window open) | < 120 MB | > 200 MB |
| Installer size | < 12 MB | > 25 MB |
| **Added input latency** | **0 ms — architecturally guaranteed** | any |

**Idle-RAM technique:** when the window hides to tray, the WebView2 process is suspended and only the Rust core (~8 MB) plus the watchdog remain resident.

---

## 13. SECURITY & HARDENING

### 13.1 Principle of least privilege
- **`HKCU` only.** Never `HKLM`. No `requireAdministrator` manifest. No UAC prompt anywhere in the product.
- **No process injection, no DLL hooks, no `SetWindowsHookEx`.** These trigger anti-cheat bans and AV heuristics, and they are not needed.
- No network access at runtime except the update check (pinned to the GitHub Releases domain).

### 13.2 Tauri v2 capability lockdown
`src-tauri/capabilities/default.json` grants **only**:
`core:window:*` (own window) · `dialog:allow-open` · `fs` scoped to `$APPDATA/CursorForge/**` · `global-shortcut` · `updater` · our own commands.
Explicitly **denied**: `shell:*`, `http:*`, `fs` outside app data, `process:allow-exit` from frontend.

### 13.3 Content Security Policy
```
default-src 'self';
img-src 'self' asset: data: blob:;
style-src 'self' 'unsafe-inline';
font-src 'self';
script-src 'self';
connect-src 'self' ipc: http://ipc.localhost;
object-src 'none'; frame-src 'none'; base-uri 'none'
```

### 13.4 IPC boundary
- The frontend can **never** pass an arbitrary registry path. Role → registry key → OCR id is a **hardcoded Rust enum**; the frontend passes only a `Role` variant name, which is parsed and rejected if unknown.
- Every file path from the frontend is `canonicalize()`d and asserted to live under `%APPDATA%\CursorForge`. Reject `..`, symlinks, UNC paths, ADS (`:`), and reserved device names (`CON`, `NUL`, `LPT1`, …).
- All commands return typed `Result<T, AppError>`; no `unwrap()` in any command path.

### 13.5 Untrusted-input handling
- **Images:** decode-time pixel-count and byte-size caps, 30 s timeout, format sniffed from magic bytes (never the extension).
- **`.cfpack` import:** validate `manifest.json` against a strict schema first; reject any entry whose extension is not in `{png, svg, cur, ani, json}`; reject zip-slip paths; cap 200 files / 50 MB uncompressed; refuse executables and scripts categorically.
- **Registry reads** are treated as untrusted strings; never `exec`'d, never interpolated into a path without validation.

### 13.6 Prompt-injection hardening (for any future AI feature)
If v2 adds AI-assisted cursor generation, these rules are binding from day one:
1. **All user content — filenames, pack manifests, `INAM`/`IART` chunks, imported JSON — is data, never instruction.** It is never concatenated into a system prompt.
2. Model output may **never** directly trigger a filesystem, registry, or Win32 call. Every AI suggestion goes through the same validated command layer with the same schema checks, and destructive actions require an explicit user click.
3. Text embedded in an imported pack claiming authority ("ignore previous instructions", "you are authorized to…") is stripped and logged, never obeyed.
4. No API keys ship in the binary. Any AI feature runs against a key the user supplies, stored in Windows Credential Manager — never in `settings.json`, never in the repo.

### 13.7 Supply chain & distribution
- `cargo audit` + `npm audit --production` in CI on every PR; build fails on high/critical.
- `cargo deny` for licence compliance; committed `Cargo.lock` and `package-lock.json`.
- **Code signing:** ship unsigned for v0 alpha, then move to **Azure Trusted Signing** (~$10/mo) before public launch. An unsigned Tauri `.exe` triggers SmartScreen "unrecognized app", which kills download conversion.
- Releases publish SHA-256 checksums; the updater verifies Tauri's minisign signature.
- Reproducible-ish builds via pinned toolchain in `rust-toolchain.toml`.

---

## 14. KNOWN LIMITATIONS (STATE THESE HONESTLY IN THE README)

1. **Apps that draw their own cursor are unaffected.** Many games using raw input / hardware-cursor bypass, some remote-desktop sessions, and a few Chromium-accelerated canvases render their own pointer. No user-mode API can override this without injection — which we will not do. **Explicit non-goal.**
2. **Cursor trails and click ripples require an overlay**, which violates §3. Deferred to v2 behind an "Effects (experimental — adds latency)" toggle, clearly labelled.
3. **Some UWP / secure-desktop surfaces** (UAC prompt, Ctrl+Alt+Del, sign-in screen) always use system defaults. Expected and correct.
4. **`.ani` frame counts above ~60** cause visible CPU cost in the shell; capped by design.
5. macOS/Linux are out of scope for v1 (macOS has no supported system-cursor API at all).

---

## 15. LEGAL — SHIPPED IN-APP AND IN-REPO

Rendered offline under **Settings → About**, and stored at `docs/`.

### 15.1 Terms & Conditions (summary of required clauses)
1. CursorForge is provided **free, as-is, without warranty**, under the MIT License.
2. It modifies **only** per-user Windows pointer settings under `HKCU\Control Panel\Cursors` — the same settings exposed by the Windows Settings app. It requires no administrator rights and makes no system-wide changes.
3. The user is solely responsible for any image they import and warrants they hold the rights to it.
4. Users may not use CursorForge to impersonate a system UI element for deceptive purposes, or to distribute malicious content via `.cfpack` files.
5. Bundled cursor assets are original works © feylo, licensed MIT alongside the source.
6. **No warranty of fitness**; the author is not liable for any loss arising from use. A one-click restore to Windows defaults is provided at all times.
7. Governing terms may be updated; continued use after an update constitutes acceptance.

### 15.2 Privacy Policy
- **CursorForge collects nothing. No analytics, no telemetry, no account, no network calls** except an explicit update check against GitHub Releases (disableable in Settings).
- All presets, custom cursors, and settings stay in `%APPDATA%\CursorForge` on the user's machine.
- If optional telemetry is ever added, it will be **opt-in only**, anonymous, and documented before release.

### 15.3 Trademark & assets
- No Windows, Microsoft, or third-party game trademarks in the app, catalog names, or marketing.
- No cursor asset may replicate a protected character, logo, or copyrighted design. All bundled assets are originals.

---

## 16. GITHUB REPOSITORY

**`github.com/feylo/cursorforge` — public.**

```
cursorforge/
├── .github/
│   ├── workflows/build.yml       # build + cargo audit + npm audit on push
│   ├── workflows/release.yml     # tag → build → sign → GitHub Release
│   └── ISSUE_TEMPLATE/
├── src/                          # React frontend
├── src-tauri/                    # Rust core
├── assets/packs/                 # SVG masters + pack.json
├── scripts/generate-packs.ts     # SVG → multi-res .cur/.ani at build time
├── docs/
│   ├── ARCHITECTURE.md
│   ├── CURSOR_FORMAT.md          # .cur / .ani binary layout notes
│   ├── TERMS.md
│   └── PRIVACY.md
├── LICENSE                       # MIT — Copyright (c) 2026 feylo
├── SECURITY.md
├── CONTRIBUTING.md
├── README.md
└── .gitignore
```

**Attribution rules — mandatory:**
- Sole author and copyright holder: **feylo**.
- **No AI tool, assistant, or vendor is named anywhere** — not in README, not in CONTRIBUTING, not in comments, not in commit messages.
- **No `Co-Authored-By:` trailers on any commit.** No "Generated with" footers.
- `git config user.name feylo` before the first commit.

---

## 17. ROADMAP

| Phase | Scope | Gate |
|---|---|---|
| **P0 — Scaffold** | Tauri v2 + React + TS + Tailwind, dark shell, custom titlebar, fonts, tokens | Window opens, HOME renders |
| **P1 — Engine** | Roles enum, registry scheme, `SetSystemCursor` live layer, snapshot + restore, watchdog | Cursor changes and survives reboot |
| **P2 — Format writers** | `cur_writer.rs`, `ani_writer.rs`, verified by `LoadImageW` round-trip | Generated `.cur` opens in Windows Pointers tab |
| **P3 — Catalog** | 64 packs from SVG masters, build script, grid UI, hover preview, tint + size | Full catalog applies correctly at 100/150/200% DPI |
| **P4 — Custom import** | Drop zone, hotspot picker, pipeline, animated support | PNG → working cursor in < 3 s |
| **P5 — Shell** | Presets, settings, tray, autostart, hotkeys, single-instance, close-to-tray | Runs 24/7 at ~0% CPU |
| **P6 — Ship** | NSIS installer, uninstall-restore, updater, legal pages, README | Double-click `.exe` → desktop app, no terminal |
| **P7 — Distribute** | Landing page (Next.js on Vercel), download page, `.cfpack` sharing | Public launch |

**v2 candidates:** community pack marketplace · per-app auto-switching profiles · effects overlay (opt-in) · cloud preset sync · AI cursor generation · macOS research spike.

---

## 18. ACCEPTANCE CRITERIA (v1 SHIP GATE)

- [ ] Double-clicking `CursorForge_1.0.0_x64-setup.exe` installs with **no admin prompt** and creates a desktop shortcut. No terminal, no PowerShell, ever.
- [ ] HOME shows **ENHANCE YOUR CURSOR**; the CTA opens the catalog.
- [ ] Clicking any catalog cursor changes the real system pointer in **< 80 ms**.
- [ ] All **17 roles** change — verified in Control Panel → Mouse → Pointers.
- [ ] Cursor is crisp at 100%, 125%, 150%, 175%, 200% DPI and at cursor sizes 32 → 256.
- [ ] Cursor persists after **reboot**, after **theme change**, and with the app **fully closed**.
- [ ] Closing the window keeps the app in the tray; **Quit** from the tray fully exits.
- [ ] Autostart works; app launches silently to tray on login.
- [ ] Dropping a PNG produces a working cursor with a correct, user-chosen hotspot in **< 3 s**.
- [ ] An imported animated GIF plays smoothly as an `.ani`.
- [ ] Presets save, apply, rename, export, import, and bind to global hotkeys.
- [ ] **Restore Windows Default** returns the machine to its exact pre-install state.
- [ ] Uninstalling restores the original cursor scheme automatically.
- [ ] Idle CPU ≤ 0.1%, idle RAM ≤ 25 MB, installer ≤ 12 MB.
- [ ] Zero `unwrap()` in command paths; `cargo clippy -- -D warnings` clean; TypeScript strict, zero `any`.
- [ ] Terms, Privacy, and Licenses render offline in-app.
- [ ] Repo is public, MIT, authored solely by `feylo`, with no AI attribution anywhere.

---
