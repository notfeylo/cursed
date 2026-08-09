# Contributing

Thanks for looking. Cursed is small on purpose, so contributions that keep
it small are the most welcome kind.

## Before you write code

Read [ARCHITECTURE.md](docs/ARCHITECTURE.md), and in particular this rule:

> **Cursed never draws a cursor. It only tells Windows which cursor to
> draw.**

A pull request that introduces an overlay window, a layered sprite, a mouse
hook, or any form of process injection will be declined regardless of how well
it is written. Those approaches are permanently laggy, they break in fullscreen,
and they trip anti-cheat software. This is not a preference.

## Setup

```bash
npm install
npm run tauri dev
```

You need Rust (stable, MSVC toolchain), Node 20+, and the Visual Studio C++
build tools.

## Before you open a pull request

```bash
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
npm run build
```

All three must pass. CI runs the same three plus `cargo audit` and `npm audit`.

## House rules

- **No `unwrap()`, `expect()` or `panic!()` in a command path.** A panic in a
  Tauri command takes the whole app down. Return `AppError`.
- **TypeScript is strict and there is no `any`.**
- **The frontend never names a path or a registry key.** If a change needs new
  data from Rust, add a typed command — do not widen an existing one to take a
  location.
- **Format writers are hand-rolled on purpose.** `cur_writer.rs` and
  `ani_writer.rs` are written against the published byte layouts because no
  available crate carries a hotspot per resolution. If you change them, the
  `LoadImageW` round-trip tests must still pass — Windows' own loader is the
  only authority on whether the bytes are correct.
- **Every catalog pack defines all 17 roles.** `npm run generate:packs` fails
  the build if one does not.

## Adding a catalog pack

Packs are parameter sets, not folders of bitmaps. Add an entry to
`src-tauri/src/packs/styles.rs`; if it needs artwork that does not exist yet,
add a variant to `src-tauri/src/packs/art.rs`.

Then run `npm run generate:packs` and commit the exported SVG masters under
`assets/packs/` so the artwork is reviewable in the diff.

Two constraints on names: no third-party trademark, and no artwork that
replicates a protected character or logo. There is a test that enforces the
first one.

## Commits

Conventional commits — `feat:`, `fix:`, `docs:`, `refactor:`, `test:`,
`chore:` — with a scope where it helps: `feat(catalog): …`.

Keep commit messages about the change and its reason. No tool attribution, no
generated-with footers, no co-author trailers.

## Reporting bugs

Include your Windows version, display scaling, cursor size, and whether the app
that misbehaved draws its own pointer (many games do — see the limitations in
the README). A screenshot of Control Panel → Mouse → Pointers is often the
fastest way to show what went wrong.

Security issues go through a
[private advisory](https://github.com/notfeylo/cursorforge/security/advisories/new),
not a public issue. See [SECURITY.md](SECURITY.md).

## Adding or replacing a font

Faces are self-hosted WOFF2 under `src/assets/fonts/`. There is no CDN: the app
works offline and its CSP has no route to one.

**Take the file from the `/* latin */` block.** The Google Fonts `css2`
response contains one `@font-face` per subset and the *first* one is
`cyrillic-ext`. Taking the first `woff2` URL gives a file of roughly 3–4 KB with
no Latin coverage at all — every glyph then renders from a fallback face, and
the interface still looks plausible, just wrong. A real latin subset is roughly
**13–22 KB**.

```bash
css=$(curl -s -A "$UA" "https://fonts.googleapis.com/css2?family=Space+Grotesk:wght@600&display=block&subset=latin")
url=$(echo "$css" | awk '/\/\* latin \*\//{f=1} f && /src:/{print; exit}' \
      | grep -oE "https://fonts.gstatic.com[^)]+\.woff2")
```

Then update `EXPECTED_FONTS` in `scripts/check-bundle.mjs` and run
`npm run build && npm run check:bundle`. That check runs in CI and fails on a
file that is too small, one that expands to too few glyphs, an orphan face
nobody uses, and any font in the built bundle that is not in the expected list.

**Never declare an evaluation-only face in `styles.css`.** A stylesheet is
emitted even when the only component importing it has been tree-shaken, so
candidate fonts declared there end up inside the installer. Put them in
`dev-fonts/` and register them at runtime from the specimen, which is how the
three-way pairing comparison works.

## The specimen sheet

`npm run dev`, then `http://localhost:1420/?specimen`. Every colour token, type
size, spacing step, component state, icon and long-text torture case on one
page, so a single screenshot answers a design question instead of a tour of
seven screens. It is gated on `import.meta.env.DEV` and must never appear in a
build; `npm run check:bundle` asserts that.
