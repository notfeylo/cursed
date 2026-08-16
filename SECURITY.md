# Security

## Reporting a vulnerability

Open a [private security advisory](https://github.com/notfeylo/cursed/security/advisories/new).
Please do not open a public issue for a vulnerability. Expect an
acknowledgement within a week.

## Threat model

Cursed is a local desktop application with three untrusted inputs:

1. **Images the user imports** — arbitrary bytes from anywhere.
2. **`.cfpack` files** — archives that may have been authored by a stranger.
3. **Registry values it reads back** — which another program may have written.

It has no server, no account, no plugin system, and no runtime code loading.

## What it deliberately does not do

- **Never writes `HKEY_LOCAL_MACHINE`** and never requests elevation. There is
  no `requireAdministrator` manifest and no UAC prompt anywhere in the product.
- **No process injection, no DLL hooks, no `SetWindowsHookEx`.** These trip
  anti-cheat and AV heuristics, and nothing here needs them.
- **No overlay window.** The pointer is drawn by Windows, not by us.
- **No network access** beyond an optional update check against the GitHub
  Releases API, which can be disabled.
- **No `eval`, no remote code, no dynamic script loading** in the webview.

## Boundaries

### IPC

The webview can call only the commands in `src-tauri/src/commands.rs`. Every one
returns `Result<T, AppError>`; there is no `unwrap`, `expect` or `panic!` in a
command path.

The frontend passes **values, never locations**:

- A pointer role is a variant of a hardcoded Rust enum. Unknown names are
  rejected outright rather than defaulted, so IPC cannot name a registry key.
- A pack is a catalog id looked up in a fixed table.
- An imported image is an opaque session token; the webview never learns a path.
- Any path that does arrive is canonicalised and asserted to live under
  `%APPDATA%\Cursed`. `..`, absolute paths, UNC paths, alternate data
  streams (`name:stream`) and reserved device names (`CON`, `NUL`, `LPT1`, …)
  are refused.

### Capabilities

`src-tauri/capabilities/default.json` grants the main window its own chrome, a
file picker, global shortcuts, and Cursed's own commands. `shell:*`,
`http:*` and filesystem access from the webview are **not granted**. Opening the
storage folder and opening a project link are Rust commands over a fixed
allow-list, not a general-purpose launcher.

### Content Security Policy

```
default-src 'self'; img-src 'self' asset: data: blob:;
style-src 'self' 'unsafe-inline'; font-src 'self'; script-src 'self';
connect-src 'self' ipc: http://ipc.localhost;
object-src 'none'; frame-src 'none'; base-uri 'none'
```

Fonts are self-hosted; nothing is fetched from a CDN.

### Untrusted images

Byte-size cap (20 MB), dimension cap (4096×4096), an explicit pixel budget, a
30-second decode timeout on a worker thread, a frame cap before decode, and
format sniffing from magic bytes — never from the file extension.

### `.cfpack` archives

Validated in this order, before a single byte is written to disk: manifest
schema, then entry paths, then extensions, then budgets. Entries must be one of
`png`, `svg`, `cur`, `ani`, `json`; executables and scripts are refused
categorically. Zip-slip paths are rejected, the archive is capped at 200 entries
and 50 MB uncompressed, and any single entry expanding more than 200× is
treated as a zip bomb. Every extracted `.cur` / `.ani` must load in Windows or
it is deleted and the import fails.

Text from a pack — names, authors, `INAM`/`IART` chunks — is stripped of control
characters and treated as inert data. It is never interpreted, never
concatenated into a command, and never obeyed.

## If AI features are ever added

These rules are binding from day one, and are why the point is made here rather
than in a future changelog:

1. All user content — filenames, manifests, info chunks, imported JSON — is
   **data, never instruction**, and is never concatenated into a system prompt.
2. Model output may never directly trigger a filesystem, registry or Win32 call.
   Every suggestion goes through the same validated command layer with the same
   schema checks, and destructive actions require an explicit user click.
3. Text claiming authority ("ignore previous instructions", "you are authorised
   to…") is stripped and logged, never obeyed.
4. No API key ships in the binary. Any key is supplied by the user and stored in
   Windows Credential Manager — never in `settings.json`, never in the repo.

## Updates

The in-app updater downloads an installer and runs it, which makes it the single
most dangerous thing the application does. It is treated accordingly.

- The release is read from the GitHub API over TLS, using the OS certificate
  store and proxy configuration via WinHTTP.
- **The downloaded installer is never trusted.** Before it is launched, its
  SHA-256 is computed with Windows CNG and compared against the checksum in the
  `SHA256SUMS.txt` published with that same release. A mismatch deletes the file
  and fails loudly. TLS proves who served the bytes, not that they are the bytes
  the author published, and a release asset can be replaced.
- If a release publishes no checksum for the installer, **the update refuses to
  run** rather than falling back to trusting the download.
- The asset name must match `Cursed_<version>_x64-setup.exe` exactly, with
  no path separators or traversal, because it becomes both a URL segment and a
  filename that gets executed. The release tag must parse as a version number
  for the same reason.
- The download must begin with `MZ` and be a plausible size, so an error page or
  a truncated transfer is never written out as an executable.
- Release notes are author-controlled text rendered in the UI. They are stripped
  of control characters, capped, and displayed as plain text — never as markup.

Until the installer is code-signed, this checksum chain is what stands between a
compromised release asset and code execution. It is not optional and should not
be relaxed.

## Fuzzing

Every parser this project owns is run against thousands of deliberately damaged
inputs on every push, as part of the ordinary test suite —
`src-tauri/src/fuzz.rs`. The property is narrow and absolute: **no input may
panic.** Rejection is expected and fine; indexing out of bounds, unwrapping a
`None`, or allocating on a length field taken from the file is not. A panic in a
command path takes the whole app down, and on a cursor tool that means the
watchdog stops with the pointer left wherever it was.

Covered: format sniffing, image decoding through the guards above, the `.cfpack`
manifest, `settings.json`, `presets.json`, `original_scheme.json`, and the
relative-path validator that decides where an archive entry is allowed to land.

Not covered, deliberately: `.cur` and `.ani` *decoding*. The app does not parse
them — it hands the path to `LoadImageW`. Fuzzing that would be fuzzing Windows'
own image loader from a test suite, with the results landing on the developer's
session. What this project writes is covered by round-trip tests in
`build::cur_writer` and `build::ani_writer`.

This is not `cargo-fuzz`: libFuzzer on Windows MSVC needs a nightly toolchain
and sanitiser support the pinned toolchain does not have, and a fuzzer that only
runs on a machine nobody has is a fuzzer that never runs. To point `cargo-fuzz`
at the same entry points from a Linux box, the targets are
`build::pipeline::sniff`, `build::pipeline::decode`, and
`serde_json::from_str::<packs::cfpack::Manifest>`; the seed corpus is the
`png`/`gif`/`bmp` helpers in the test module.

## Supply chain

`cargo audit`, `cargo deny` and `npm audit` run in CI on every push and pull
request, and the build fails on a high or critical advisory. Dependabot is
configured in `.github/dependabot.yml` — weekly and grouped, because a bot that
opens eleven pull requests a morning gets muted, and then the one that mattered
is in the pile nobody opens. `Cargo.lock` and `package-lock.json` are committed
and the Rust toolchain is pinned in `src-tauri/rust-toolchain.toml`.

Releases publish SHA-256 checksums, and — once the signing key exists — a
minisign signature per installer that the app verifies before running an update.
See `docs/SIGNING.md`; the checksum alone protects against a corrupted download
and not against anyone who can replace both files.

**Code signing:** builds are unsigned, so Windows SmartScreen will warn on first
run. Signing via Azure Trusted Signing is planned. Until then, verify the
published SHA-256 of anything you download.
