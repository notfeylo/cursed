# `src-tauri/` — the core

The Rust half. Everything that touches Windows is here, and nothing that touches
Windows is anywhere else.

**The one architectural rule, restated because every module depends on it:**
Cursed never draws a cursor. It only tells Windows which cursor to draw. No
overlay window, no layered sprite, no hooks. Every pointer in this product is a
real `.cur` or `.ani` file handed to the OS, so it is drawn by the same code path
that draws the stock arrow — which is the only way a cursor keeps up with the
pointer instead of trailing it. Anything that reintroduces an overlay is wrong
regardless of how well it is written, and it is why "works inside every game" is
an explicit non-goal: a game reading raw input needs injection, and we do not
inject.

## Where to start reading

`src/lib.rs` — the setup function, in the order things actually happen. Then
`src/commands.rs`, which is the entire surface the UI can reach. If a behaviour
is not reachable from one of those two files, it is not reachable at all.

## `src/` — the modules

| Path | What it is for |
| --- | --- |
| `lib.rs` | App setup: plugins, window, tray, hotkeys, the watchdog, and the order they start in. |
| `main.rs` | Three lines. The real entry point is `lib.rs`, so the library can be unit-tested without a window. |
| `commands.rs` | **The only `#[tauri::command]` surface.** Every call the UI can make is here and nowhere else. |
| `cursor/` | The three layers of applying a cursor — see below. |
| `build/` | Artwork in, cursor files out. Pure, and unit-tested: no registry, no Win32. |
| `packs/` | The catalog: parametric artwork, the brand mark, and the renderer. |
| `state/` | `settings.rs` (what the user chose) and `presets.rs` (saved looks). |
| `session.rs` | What is applied *now*, across launches. The registry persists the cursor; this persists the reason — which pack, which tint, which size — so the watchdog can tell a theme reset from a deliberate change. |
| `custom.rs` | The image-to-cursor feature. Staging (decode, normalise, show) is deliberately separate from building (write the files): nothing touches the pointer until the user has seen what they are about to get. |
| `import.rs` | Folders and zips of `.cur`/`.ani` someone already had, turned into first-class catalog entries. |
| `bundled.rs` | The 36 hand-made packs embedded in the installer and unpacked on first run. |
| `updates.rs` | Check, download, verify, install — on WinHTTP rather than an HTTP crate, so it uses the OS certificate store and proxy settings and keeps ~2 MB of TLS stack out of a 12 MB binary. |
| `hash.rs` | SHA-256 through Windows CNG, used to verify a downloaded installer against the published checksum *before* it is executed. |
| `paths.rs` | Every path the app writes, and the only place that decides what a legal path is. Nothing else builds one. |
| `tray.rs` | The tray icon and its menu — switch preset, put Windows back, quit. This app spends most of its life here. |
| `hotkeys.rs` | Global shortcuts. Registration is per-accelerator: one clash skips one shortcut instead of losing them all. |
| `autostart.rs` | Launch on sign-in, per-user under `HKCU\...\Run`. `--silent` is what sends an autostarted launch to the tray instead of throwing a window at someone who just signed in. |
| `window_state.rs` | Where the window opens: remembered position, but only if it is still on the monitor the pointer is on. |
| `idle.rs` | Gives working set back when the window is hidden. A tray app has no business holding a rendering engine nobody is looking at. |
| `shell.rs` | The two narrow shell affordances that survive the Tauri shell plugin being denied outright: open our own folder, open one of three allow-listed URLs. Neither takes an argument the webview controls. |
| `error.rs`, `util.rs` | The error type every command returns, and the small shared helpers. |
| `bin/genpacks.rs` | The offline tool: renders the catalog to `assets/packs/`, draws the app icon, and produces the review sheets (`--ladder`, `--logo-sheet`, `--cutout`, `--roles`) used when judging artwork. |

### `cursor/` — the three layers

| File | Layer |
| --- | --- |
| `engine.rs` | **A.** `SetSystemCursor`. Instant, session-only, lost on reboot. Note that it accepts only the fourteen documented `OCR_*` ids — `NWPen`, `Pin` and `Person` are `IDC_*` and are best-effort here, or every apply would report failure. |
| `scheme.rs` | **B.** `HKCU\Control Panel\Cursors`. Survives reboot; this is what actually persists. |
| `watchdog.rs` | **C.** Notices when something else stomps on B — a theme change, a settings sync, another cursor app — and re-applies. |
| `restore.rs` | Undo. Hands the machine back exactly as it was found, and is used by Settings *and* by the uninstaller. |
| `roles.rs` | The seventeen roles and their registry names. |

### `build/` — artwork to cursor files

`svg` → `bitmap` → `matte` (background removal) → `pipeline` → `cur_writer` /
`ani_writer`, with `hotspot` deciding where the click lands and `cur_reader`
parsing files that already exist.

The writers are hand-rolled against the published byte layouts rather than
delegated to a crate, because the crates available do not carry a hotspot *per
resolution* — which is the one thing a multi-resolution cursor needs most. The
format itself is documented in [`../docs/CURSOR_FORMAT.md`](../docs/CURSOR_FORMAT.md).

### `packs/`

`styles.rs` defines the single generated blend base that fills the roles an
imported pack leaves unmapped; `art.rs` draws the roles; `brand.rs` draws the
mark (the same geometry as `src/components/Mark.tsx`); `catalog.rs` assembles
everything the UI sees; `cfpack.rs` is the pack file format; `logo.rs` is the
lockup used for the icon and the site.

## Configuration and build inputs

| File | What it decides |
| --- | --- |
| `tauri.conf.json` | Window, bundle, updater and CSP. The version here must match `package.json` and `Cargo.toml` — use `npm run version:set`. |
| `capabilities/default.json` | Exactly which plugin commands the webview may call. Everything else is denied, including the shell plugin. |
| `installer-hooks.nsh` | NSIS hooks for install and uninstall. Both uninstall hooks return immediately when `$UpdateMode = 1`: Tauri upgrades by running the *previous* version's uninstaller, and Tauri guards its own app-data removal but inserts these hooks unguarded. |
| `rust-toolchain.toml` | The pinned compiler **and every target a release ships**. Add targets here, never with `rustup target add` — that adds to the *default* toolchain, and the build then fails with "can't find crate for `std`" against a target rustup reports as installed. |
| `deny.toml` | Licence allow-list and the ban on any networking stack reaching a Windows build. Its `[graph] targets` list must stay identical to the one in `rust-toolchain.toml`. |
| `build.rs` | `tauri-build`. Regenerates `gen/` from `capabilities/`, which is why `gen/` is not committed. |
| `icons/` | The app icon set. See [`icons/README.md`](icons/README.md). |
| `generated/` | Empty in git, filled at build time. |

## Working on it

```bash
cargo test                          # from this directory, so the pinned toolchain applies
cargo clippy --all-targets -- -D warnings
cargo clippy --target aarch64-pc-windows-msvc --all-targets -- -D warnings
```

Run cargo from **this directory**, not the repo root with `--manifest-path`.
Rustup picks a toolchain from the current directory, so from the root you get
whatever `stable` happens to be installed rather than the pinned compiler — and
for a cross target, one with no `std` for it.

The Win32 behaviour that cost real debugging time is written down in
[`../docs/ARCHITECTURE.md`](../docs/ARCHITECTURE.md), not rediscovered.
