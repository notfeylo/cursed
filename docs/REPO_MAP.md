# What lives where

One screen, for someone opening this repository cold. `ARCHITECTURE.md` explains
*how* the app works; this explains where to find it.

## The shape of it

Cursed is a Tauri v2 desktop app: a Rust core that talks to Windows, and a React
front end that never does. The front end names no file path and no registry key
— it asks the core for a pack by id and shows what comes back. That boundary is
the reason a UI change can never write to the wrong place.

```
src-tauri/          The Rust core. Everything that touches Windows.
src/                The React front end. Everything the user looks at.
assets/             Generated cursor packs + the bundled third-party archives.
website/            cursorforge.vercel.app. Static, no build step, no scripts.
scripts/            Build, release and verification tooling.
docs/               This, and everything else worth writing down.
```

## `src-tauri/src` — the core

| Path | What it is for |
| --- | --- |
| `commands.rs` | **The only `#[tauri::command]` surface.** Every call the UI can make is here, and nowhere else. |
| `cursor/` | The three layers: `engine` (live `SetSystemCursor`), `scheme` (the registry), `watchdog` (puts it back when Windows changes it), `restore` (undo, used by Settings *and* the uninstaller). |
| `build/` | Turning artwork into cursor files: `svg`, `bitmap`, `matte` (background removal), `pipeline`, `cur_writer`, `ani_writer`, `hotspot`. Pure and unit-tested — no registry, no Win32. |
| `packs/` | The built-in catalog. `styles.rs` is the 291 pack definitions, `art.rs` draws the roles, `brand.rs` the mark, `catalog.rs` assembles it. |
| `custom.rs` | Cursors built from the user's own images, including their optional hover artwork. |
| `import.rs` | Folders and zips of `.cur`/`.ani` the user already had. |
| `bundled.rs` | Packs embedded in the binary and installed on first run. Licence-checked; see `LICENSES.md`. |
| `updates.rs` | The one network request the app makes. Architecture-aware asset matching, checksum verification. |
| `paths.rs` | Every path the app writes. Nothing else builds one. |
| `session.rs`, `state/` | What is applied now, and the settings behind it. |

## `src` — the front end

`screens/` one per view · `components/` shared UI · `lib/` data shaping and the
typed IPC client · `store.ts` state. One component per file, named for the file.
Business logic belongs in `lib/`, not in a component.

## `scripts/`

| Script | Run it when |
| --- | --- |
| `build-release.mjs` (`npm run release`) | Cutting a release. Builds x64 + ARM64 + 32-bit + the offline installer, stages aliases, writes `SHA256SUMS.txt`. |
| `verify-uninstall.ps1` | Before a release. `-Snapshot` before installing, no arguments after uninstalling. **Release gate.** |
| `check-bundle.mjs` (`npm run check:bundle`) | Run by CI. Catches fonts with no Latin glyphs and the dev-only specimen route reaching production. |
| `set-version.mjs` (`npm run version:set`) | Bumping the version in all three files that carry one. |
| `make-icon.mjs` | Regenerating the icon set from the mark. |

## Things that look odd and are deliberate

- **The directory is `CURSORFORGE`, the product is `Cursed`.** The repository was
  renamed; the checkout was not. The name `cursorforge` is **permanently
  burned** — see `CONTRIBUTING.md`. Creating anything under it kills updates for
  every 1.6.x/1.7.0 install still relying on GitHub's rename redirect.
- **`dev-fonts/`** feeds the dev-only specimen route at `?specimen` and is
  referenced only as a runtime URL. Vite copies `public/` into a build and
  nothing else, so it cannot ship; `check:bundle` asserts the specimen module is
  absent from production regardless.
- **`logo-sheets/`** is git-ignored. The brand and matte tooling writes contact
  sheets there on demand. Committed renders went stale the moment the mark
  changed, which is worse than no renders.
- **`assets/packs/` is generated and committed.** CI regenerates it and fails if
  the result differs, so the catalog in the repo is always the catalog the code
  produces.
- **`LEGACY_APP_DIR` and `LEGACY_SCHEME_PREFIX`** still name CursorForge. They
  exist to clean up after installs that predate the rename, and deleting them
  would strand those users.
