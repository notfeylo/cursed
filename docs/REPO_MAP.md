# What lives where

One screen, for someone opening this repository cold. `ARCHITECTURE.md` explains
*how* the app works; this explains where to find it. Every directory below also
carries its own README with the detail.

## The shape of it

Cursed is a Tauri v2 desktop app: a Rust core that talks to Windows, and a React
front end that never does. The front end names no file path and no registry key
— it asks the core for a pack by id and shows what comes back. That boundary is
the reason a UI change can never write to the wrong place.

```
src/                The React front end. Everything the user looks at.
src-tauri/          The Rust core. Everything that touches Windows.
assets/             The 83 bundled packs, and the generated artwork kept for review.
website/            trycursed.com static source, guides, build script, and deployment policy.
scripts/            Build, release and verification tooling.
docs/               This, and everything else worth writing down.
.github/            CI and the issue/PR templates.
dev-fonts/          Fonts for the dev-only specimen screen. Cannot ship.
```

## `src/` — the front end · [README](../src/README.md)

`screens/` one per view · `components/` shared UI · `lib/` typed IPC and shared
shapes · `store.ts` state · `styles.css` every design token.

| Path | What it is for |
| --- | --- |
| `App.tsx` | The shell: current view, title bar, backdrop, banner. |
| `store.ts` | The whole app state, in one zustand store. `DEFAULT_SETTINGS` is what a fresh install looks like. |
| `lib/ipc.ts` | **The only place that calls `invoke`.** Every core command, typed. |
| `lib/types.ts` | Shapes mirrored 1:1 from Rust, including the canonical order of the seventeen roles. |
| `lib/useGlideScroll.ts` | Eased wheel scrolling. Windows delivers a notch as one ~100px jump and `scroll-behavior` does not apply to the wheel. |
| `screens/Specimen.tsx` | Dev-only reference sheet at `?specimen`. Dropped from production builds, and `check:bundle` fails if it ever is not. |

## `src-tauri/` — the core · [README](../src-tauri/README.md)

| Path | What it is for |
| --- | --- |
| `src/lib.rs` | Setup, in the order things actually happen. Start here. |
| `src/commands.rs` | **The only `#[tauri::command]` surface.** Every call the UI can make. |
| `src/cursor/` | The three layers: `engine` (live `SetSystemCursor`), `scheme` (the registry), `watchdog` (puts it back when Windows changes it), `restore` (undo, used by Settings *and* the uninstaller), `crosschannel` (which of two installed channels may defend the scheme). |
| `src/channel.rs` | Which of the two side-by-side installs this binary is. Every per-channel name comes from here and nowhere else — see [`../docs/CHANNELS.md`](CHANNELS.md). |
| `src/build/` | Artwork into cursor files: `svg`, `bitmap`, `matte` (background removal), `pipeline`, `cur_writer`, `ani_writer`, `hotspot`, `cur_reader` (one frame, via Windows), `icon_reader` (`.cur`/`.ico`/`.ani` parsed from bytes, every frame). Pure and unit-tested — no registry, no Win32. |
| `src/packs/` | `styles.rs` defines the one generated blend base, `art.rs` draws the roles, `brand.rs` the mark, `catalog.rs` assembles it, `cfpack.rs` is the pack format. |
| `src/bundled.rs` | **The catalog.** The 83 packs embedded in the binary and installed on first run. |
| `src/custom.rs` | Cursors built from the user's own images, including optional hover artwork. |
| `src/import.rs` | Folders and zips of `.cur`/`.ani` the user already had. |
| `src/updates.rs` | The one network request the app makes, on WinHTTP. Architecture-aware asset matching, checksum verified before anything is executed. |
| `src/paths.rs` | Every path the app writes. Nothing else builds one. |
| `src/signing.rs` | Verifies a downloaded installer against the release signature, when a key is compiled in. See [`SIGNING.md`](SIGNING.md). |
| `src/backup.rs` | Everything the user made, in one zip. Restore merges rather than replaces. |
| `src/dataprint.rs` | Fingerprints the data directory, so `verify-release.ps1` can prove an update touched nothing. |
| `src/stress.rs` | The handle harness and the soak: what the counters do over hours of work. |
| `src/fuzz.rs` | Test-only. Thousands of damaged inputs per parser, per push. No input may panic. |
| `src/session.rs`, `src/state/` | What is applied now, and the settings behind it. |
| `src/bin/genpacks.rs` | The offline tool: exports the catalog, draws the icon, renders the review sheets, checks the seventeen roles, runs the soak. |
| `tauri.conf.json`, `capabilities/` | Window and bundle config, and exactly which plugin commands the webview may call. |
| `dev.tauri.conf.json` | Merged over the above to build the development channel. Product name, identifier and icons only. |
| `installer-hooks.nsh` | NSIS install/uninstall hooks. Both uninstall hooks return immediately in update mode. |
| `rust-toolchain.toml`, `deny.toml` | The pinned compiler with every shipped target, and the licence/ban policy scoped to those same targets. |
| `icons/` | The app icon set, all derived from `source.png`. |

## `assets/` · [README](../assets/README.md)

`bundled/` is eighty-three `.zip` packs embedded by `include_bytes!` — this is what
ships. `packs/` is generated SVG artwork, committed only so a change to the
drawing code shows up as a change to a picture; nothing reads it at runtime.

## `scripts/` · [README](../scripts/README.md)

| Script | Run it when |
| --- | --- |
| `build-release.mjs` (`npm run release`) | Cutting a release. Builds x64 + ARM64 + 32-bit + the offline installer, stages aliases, writes `SHA256SUMS.txt`. |
| `verify-release.ps1` | **In a VM, before publishing.** The whole matrix in one command: baseline, first install, update, data comparison, roles, uninstall, pass/fail table. See [`verification/VM_SETUP.md`](verification/VM_SETUP.md). |
| `verify-uninstall.ps1` | Before a release. `-Snapshot` before installing, no arguments after uninstalling. **Release gate**, and called by the above. |
| `sign-release.mjs` | Signs each versioned installer with the release key. Run by the release workflow, which refuses to build without the secrets. |
| `check-bundle.mjs` (`npm run check:bundle`) | Run by CI. Catches fonts with no Latin glyphs, the dev-only specimen route reaching production, a dev-channel binary about to ship as the release, an update path that could reach the uninstaller, anything bundled that is not NSIS, and mojibake anywhere in the source. |
| `set-version.mjs` (`npm run version:set`) | Bumping the version in all three files that carry one. |
| `make-icon.mjs` | Regenerating the icon master from the mark. |
| `build-dev.mjs` (`npm run build:dev`) | Building the development channel, and proving the build was invoked correctly. |
| `channels.mjs` (`npm run channels`) | Finding out which of the two installed channels is holding the pointer. |

Two more live in `genpacks` rather than in `scripts/`, because they need the
app's own code:

| Command | Run it when |
| --- | --- |
| `npm run check:roles` | A cursor does not change in some application. Reads all seventeen registry entries and checks each resolves to a loadable cursor. Almost always the answer. |
| `npm run soak -- <minutes> <csv>` | Before a release, or after anything that touches the cursor lifecycle. Samples GDI, USER, threads, handles and memory once a minute. |

## `.github/`

`workflows/build.yml` runs on every push: the frontend build, `check:bundle`,
clippy, tests, the catalog check, an installer build with a size budget, plus a
cross-architecture clippy matrix and a `npm audit` / `cargo audit` /
`cargo deny` job. `workflows/release.yml` fires on a `v*` tag and produces a
**draft** release with every installer attached — publish it by hand, or publish
first and the workflow has nothing to add.

## Things that look odd and are deliberate

- **The directory is `CURSORFORGE`, the product is `Cursed`.** The repository was
  renamed; the checkout was not. The name `cursorforge` is **permanently
  burned** — see `CONTRIBUTING.md`. Creating anything under it kills updates for
  every 1.6.x/1.7.0 install still relying on GitHub's rename redirect.
- **Cargo is run from `src-tauri/`, never from the root with `--manifest-path`.**
  Rustup resolves a toolchain from the current directory, so from the root the
  pinned compiler and its target list are never seen. This cost four consecutive
  red CI runs before 1.18.0.
- **`dev-fonts/`** feeds the dev-only specimen route at `?specimen` and is
  referenced only as a runtime URL. Vite copies `public/` into a build and
  nothing else, so it cannot ship; `check:bundle` asserts the specimen module is
  absent from production regardless.
- **`logo-sheets/`** is git-ignored. The brand and matte tooling writes contact
  sheets there on demand. Committed renders went stale the moment the mark
  changed, which is worse than no renders.
- **`assets/packs/` is generated and committed**, and CI regenerates it and fails
  if the result differs. It held 216 packs until 1.18.0 and holds one now: the
  other 215 were the generated catalog, replaced by the hand-made packs.
- **`docs/PRD.md` is the brief the product was built to, not a description of
  it.** It is kept because code and CI cite its section numbers; where it and the
  rest of `docs/` disagree, the rest of `docs/` is what shipped.
- **`src-tauri/gen/` is not committed.** `tauri-build` regenerates it from
  `capabilities/` on every build, and committing it lands every capability
  change twice in the diff.
- **`LEGACY_APP_DIR` and `LEGACY_SCHEME_PREFIX`** still name CursorForge. They
  exist to clean up after installs that predate the rename, and deleting them
  would strand those users.
