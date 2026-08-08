# Contributing

Thanks for looking. CursorForge is small on purpose, so contributions that keep
it small are the most welcome kind.

## Before you write code

Read [ARCHITECTURE.md](docs/ARCHITECTURE.md), and in particular this rule:

> **CursorForge never draws a cursor. It only tells Windows which cursor to
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
[private advisory](https://github.com/feylo/cursorforge/security/advisories/new),
not a public issue. See [SECURITY.md](SECURITY.md).
