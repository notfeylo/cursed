# Candidate faces — evaluation only

Not shipped. These exist so the three pairings in the specimen sheet
(`npm run dev`, then `?specimen`) can be compared on identical content.

Vite serves project-root files during development and copies only `public/`
into a build, so nothing here reaches the installer. Verified by listing the
`.woff2` files in `dist/assets` after a production build: only the five faces
the app actually uses appear.

They are registered at runtime by `src/screens/Specimen.tsx` rather than in
`styles.css`, because a stylesheet is emitted even when the component that
imports it is tree-shaken — which would have put eight unused fonts inside the
installer.

| Family | Weights | Licence |
|---|---|---|
| Space Grotesk | 600, 700 | SIL Open Font License 1.1 |
| Inter Tight | 400, 500 | SIL Open Font License 1.1 |
| Geist / Geist Mono | 400, 500, 700 / 400 | SIL Open Font License 1.1 |

Latin subsets, fetched from the Google Fonts `css2` endpoint.

**When a pairing is chosen:** move the winning families into
`src/assets/fonts/`, declare them in `styles.css`, add them to
`docs/LICENSES.md` beside the existing OFL entries, and delete this directory.

**If none is chosen:** delete this directory. Nothing else references it.
