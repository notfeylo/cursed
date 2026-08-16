# `scripts/`

Build, release and verification tooling. Node for anything cross-platform,
PowerShell for the one thing that has to talk to a real Windows install.

| Script | Run it | When |
| --- | --- | --- |
| `build-release.mjs` | `npm run release` | Cutting a release. Builds x64, ARM64 and 32-bit, then the x64 offline build with the WebView2 runtime embedded; stages each under both its versioned name and an unversioned alias, and writes `SHA256SUMS.txt` into `dist-release/`. |
| `verify-uninstall.ps1` | `powershell -File scripts/verify-uninstall.ps1 -Snapshot` before installing, no arguments after uninstalling | **Release gate.** Asserts all seventeen pointer roles came back byte-identical and that no file, registry key, cursor scheme, autostart entry or shortcut remains. Exit code is the number of failed assertions, so it can gate a release automatically. |
| `check-bundle.mjs` | `npm run check:bundle` (after `npm run build`) | Every CI run. Catches a font subset with no Latin glyphs — which silently renders in a fallback face and still looks fine at a glance — the dev-only specimen route reaching production, and a development-channel binary about to ship as the release. |
| `set-version.mjs` | `npm run version:set 1.19.0` | Bumping the version. `package.json`, `src-tauri/tauri.conf.json` and `src-tauri/Cargo.toml` each carry one; bumping by hand is how they drift, and a drifted version makes the updater compare against a version the app is not running. |
| `make-icon.mjs` | `node scripts/make-icon.mjs src-tauri/icons/source.png` | Redrawing the app mark. Renders 1024×1024 with no image dependencies — Node's zlib is all a PNG encoder needs — and `tauri icon` derives the rest of the set from it. |
| `build-dev.mjs` | `npm run build:dev` | Building the development channel's installer. The cargo feature and the config override are both required and neither is checked by the build, so this asserts afterwards that the artifact is the one that was asked for. See [`../docs/CHANNELS.md`](../docs/CHANNELS.md). |
| `channels.mjs` | `npm run channels` | Working out which of the two installed channels is holding the pointer. Read-only: which channels are installed and running, how large each data directory is, who last claimed the scheme and who captured the original. |

## Why the release script builds four installers

Three architectures, because Windows is not one target: a 32-bit Windows 10
cannot run an x64 build at all, and an ARM64 PC can only run one emulated,
paying for it in speed and battery on the machines least able to spare either.

Two WebView2 strategies, because the window is Edge WebView2. The ordinary
installer checks for the runtime and fetches it if missing (~11 MB); the offline
one embeds it (~214 MB) for an air-gapped machine or a network that blocks
Microsoft's download. The small one is the default and the only one the updater
will match — `is_our_installer` accepts `Cursed_<version>_<arch>-setup.exe`, and
the offline build is named to fall outside that on purpose, so a background
update can never quietly pull 214 MB on a metered connection.

## The gate before tagging

```bash
npm run check:bundle
cargo clippy --all-targets -- -D warnings     # from src-tauri, per shipped target
cargo test                                    # from src-tauri
cargo test --features dev-channel             # the other channel's constants
npx tsc --noEmit
```

The second `cargo test` is not redundant. The channels are `#[cfg]`-gated
constants, so the default run never compiles the dev channel's code or the
assertions that hold it apart from the shipped one.

then the uninstall check above, which needs a real machine or a VM rolled back
to a clean snapshot. What each release actually verified — and what it could
not — is recorded in [`../docs/verification/`](../docs/verification/).
