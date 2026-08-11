# `src-tauri/icons/`

The app icon, at every size Windows and Tauri ask for. All of it is derived —
nothing here is hand-drawn in an image editor, and none of it should be edited
directly.

| File | Used by |
| --- | --- |
| `source.png` | The 1024×1024 master everything else comes from. |
| `icon.ico` | The executable, the title bar, Explorer, the tray. |
| `icon.png`, `16x16` … `256x256`, `128x128@2x` | Tauri's standard set. |
| `Square*Logo.png`, `StoreLogo.png` | The MSIX/Store sizes Tauri's bundler expects. Generated for completeness; the product ships as NSIS. |

## Regenerating

```bash
node scripts/make-icon.mjs src-tauri/icons/source.png   # redraw the master
npx tauri icon src-tauri/icons/source.png               # derive the whole set
```

`make-icon.mjs` draws the mark with no image dependencies at all — Node's zlib
is the only thing a PNG encoder needs — supersampled 3× for antialiasing. The
geometry is the same mark as `src/components/Mark.tsx` in the UI and
`src-tauri/src/packs/brand.rs` in the core: one shape, three renderers, and they
are meant to stay in step. Change one and the other two want the same change.
