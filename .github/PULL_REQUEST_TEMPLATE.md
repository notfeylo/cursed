## What this changes

<!-- One or two sentences. Link the issue if there is one. -->

## Checklist

- [ ] `cargo clippy --all-targets -- -D warnings` passes, run from `src-tauri/`
- [ ] `cargo test` passes, run from `src-tauri/`
- [ ] `npm run build` passes
- [ ] No `unwrap()`, `expect()` or `panic!()` in a command path
- [ ] No new capability granted to the webview, and no new path or registry key
      reachable from IPC
- [ ] If catalog artwork changed: `npm run generate:packs` was run and
      `assets/packs` is committed

## If this touches the cursor engine

- [ ] It does not introduce an overlay window, a self-drawn cursor, or a hook —
      see the architecture rule in `docs/ARCHITECTURE.md`
- [ ] The `LoadImageW` round-trip tests still pass
- [ ] Tested at more than one DPI or cursor size
