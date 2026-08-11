# `src/` — the front end

Everything the user looks at. React 18, TypeScript in strict mode with no `any`,
Tailwind v4 for layout, one small zustand store for state. Built by Vite into
`dist/`, which Tauri then embeds in the binary — there is no server, and nothing
here is fetched at runtime.

**The rule that shapes all of it:** the front end names no file path and no
registry key. It asks the core for a pack by id, a preset by name, a role by
enum variant, and shows what comes back. Every path decision lives in
`src-tauri/src/paths.rs`. That boundary is why a UI change cannot write to the
wrong place, and it is worth keeping even when a direct call would be shorter.

## Where to start reading

`main.tsx` → `App.tsx` → whichever screen you care about. `App.tsx` is the shell:
it holds the current view, the title bar, the backdrop and the one banner slot,
and it is about a hundred lines because everything else lives in a screen.

## Layout

| Path | What it is |
| --- | --- |
| `main.tsx` | Mounts React and kills the webview's browser affordances — context menu, drag-and-drop navigation — because this window is our own chrome, not a page. |
| `App.tsx` | The shell. Current view, title bar, backdrop, banner, and the dev-only specimen route. |
| `store.ts` | The whole app state: current view, settings, presets, catalog, what is applied. zustand, one store, no context providers. `DEFAULT_SETTINGS` is here and is the single answer to "what does a fresh install look like". |
| `lib/ipc.ts` | **The only place that calls `invoke`.** Every command the core exposes, typed, with errors normalised to `IpcError`. If a screen needs something from Rust, it gets a function here first. |
| `lib/types.ts` | Shapes mirrored 1:1 from Rust — `Role`, `Settings`, `Preset`, `PackSummary`. `ROLES` is the canonical order of the seventeen pointer roles and the labels the UI shows for them. |
| `lib/useGlideScroll.ts` | Eased wheel scrolling, installed once on the document. Windows delivers a notch as one ~100px jump; this turns each notch into a target and eases toward it, accumulating on a fast flick. `scroll-behavior` cannot do this — it governs programmatic scrolls only. |
| `styles.css` | Design tokens, base layer, and every effect. See below. |
| `assets/fonts/` | The three families, self-hosted as subset `.woff2`. `LICENSES.md` records what each one is licensed under. |
| `assets/backdrop.png` | The one raster in the app. Everything else is drawn. |

### `screens/` — one file per view

| Screen | What it does |
| --- | --- |
| `Home.tsx` | The landing view. The mark is the composition, not a corner logo. |
| `Catalog.tsx` | Browsing the packs, with a live pointer preview that waits for the pointer to settle before applying — hovering must feel free. |
| `Customise.tsx` | Size, tint, outline, animation speed, and the toggle that decides whether the hand and I-beam follow the pointer's size. |
| `CustomImport.tsx` | The image-to-cursor flow: drop a picture, cut the background, pick the hotspot, get a real `.cur`/`.ani`. The largest screen, because it is a wizard. |
| `Saved.tsx` | Presets — save the current look, switch between them, delete. |
| `Settings.tsx` | Startup, tray, hotkeys, updates, and restoring Windows' own pointers. Hosts `UpdatePanel`. |
| `About.tsx` | Version, build, licences, and the legal documents rendered from the copies embedded in the binary. |
| `Specimen.tsx` | **Dev only.** A reference sheet of every token, control and state, reachable at `?specimen` under `npm run dev`. `import.meta.env.DEV` is a compile-time constant, so the bundler drops this file entirely from a production build — and `npm run check:bundle` fails the build if it ever reaches one. |

### `components/` — shared UI

| Component | What it is |
| --- | --- |
| `ui.tsx` | The primitives: buttons, cards, fields, banner, toggle. Import from here rather than restyling a `<button>`. |
| `TitleBar.tsx` | Frameless chrome. The drag region is the whole bar minus the two buttons, handled natively by `data-tauri-drag-region` so dragging never round-trips through JS. |
| `Mark.tsx` | The Cursed mark, in SVG. The same geometry as `src-tauri/src/packs/brand.rs`, which renders the app icon — one shape, two renderers, and they are meant to stay in step. |
| `UpdatePanel.tsx` | The whole update flow — check, download, verify, install — in one place, inside Settings. An update nobody can find is an update nobody applies. |
| `Markdown.tsx` | A small renderer for the three legal documents. They are ours and ship inside the binary, so this handles headings, lists, links and emphasis and nothing else. |
| `ScreenHeader.tsx` | The title block every screen opens with. |

## Design, in `styles.css`

One file, sectioned by comment rules, because the whole app is one window and
splitting 360 lines across five files makes a token harder to find, not easier.

- **`:root`** holds every colour, radius and duration the app uses. Change a
  token here rather than a value at a call site.
- **Surfaces are glass, not fills** — cards sit *on* the backdrop rather than
  covering it, which is what keeps the window feeling like one object.
- **Hover lifts and casts** instead of gaining a coloured border. A border
  changes an element's shape on hover; a shadow does not.
- **Scrollbars** are thin and dim, styled once for every scroll container.
- **Motion is a garnish.** `prefers-reduced-motion` removes it, and `.no-motion`
  suppresses it while a live cursor preview is on screen — an animating page
  behind a pointer being previewed is exactly the wrong place to draw the eye.

## Conventions

- One component per file, named for the file.
- Business logic goes in `lib/`, not in a component. A screen composes; it does
  not decide.
- No `any`, and `npm run lint:types` is a gate rather than a suggestion.
- If a screen wants data, it goes through `store.ts` or `lib/ipc.ts` — never
  `invoke` directly.
