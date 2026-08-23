# cursorforge.vercel.app

The landing page. Static HTML and CSS, no JavaScript **on the page**.

```bash
vercel deploy --prod          # from this directory
```

## The version number syncs itself now

`vercel.json` sets `buildCommand` to `node scripts/sync-release.mjs`, so every
deploy rewrites the version, installer size and release date from the live
GitHub release before publishing.

That script already existed and nothing ran it. The result was the published
site sitting on **1.17** while the repository had said 1.20.0 for a week:
syncing was a manual step in front of a manual deploy, and a step you have to
remember is a step that gets missed. Three releases missed it.

It fails soft by design — an unreachable GitHub API leaves the committed values
in place and the deploy succeeds — so wiring it in cannot break a deploy, only
keep one honest.

The page still ships no JavaScript. The sync runs at build time, on Vercel's
builder, and what reaches the browser is plain HTML with the numbers already in
it. That is why the CSP can stay at `script-src 'none'`.

## One page, four blocks

The site is a single page: title, an about section, the download, the footer.
There is no FAQ, no screenshot gallery and no separate routes — every anchor in
the header and footer points at a section of this page.

## The one animation, and why it is CSS

Everything that used to move is gone: the drifting background blobs, the grid,
the flying cursors, the orbit, the scroll reveals, the hover transitions. What
is left is the laptop in the about section, whose lid is closed and swings open
as you scroll it into view.

That is a **scroll-driven CSS animation** (`animation-timeline: view()`), not
JavaScript, because this site ships none. Two consequences worth knowing before
editing `styles.css`:

- The **open** lid is the default, and the closed state lives inside
  `@supports (animation-timeline: view())` and
  `@media (prefers-reduced-motion: no-preference)`. A browser without
  scroll-driven animations, or a reader who has asked for less motion, gets a
  laptop that is simply open. Written the other way round they would get one
  permanently shut.
- `body` must not set `overflow-x`. `overflow-x: hidden` makes the body a scroll
  container, and a `view()` timeline measured against it stops working. The
  footer clips its own oversized wordmark instead.

Below 900px the laptop flattens into an ordinary bordered panel and the about
text becomes normal page content — a phone-width screen with its own scrollbar
inside it is worse than no laptop at all.

## Why there is no JavaScript

The page explains the app and hands over an `.exe`. Nothing on it needs to run
code, so `vercel.json` sets `script-src 'none'` rather than `'self'` — a CSP that
forbids scripts entirely is worth more than one that merely restricts their
origin, and it costs nothing here.

`default-src` is `'none'` with each resource type opened individually, so a new
kind of subresource has to be allowed deliberately rather than inherited.

## Headers

Set in `vercel.json`:

| Header | Why |
| --- | --- |
| `Content-Security-Policy` | No scripts, no objects, no framing, no forms |
| `Strict-Transport-Security` | Two years, subdomains, preload-eligible |
| `Cross-Origin-Opener-Policy` | Isolates the browsing context |
| `Cross-Origin-Embedder-Policy` | `require-corp`; every asset here is same-origin |
| `Cross-Origin-Resource-Policy` | Nothing here is meant to be embedded elsewhere |
| `Referrer-Policy` | `no-referrer` — the page has nobody to tell |
| `Permissions-Policy` | Every powerful feature switched off by name |
| `X-Content-Type-Options` | `nosniff` |
| `X-Frame-Options` | Belt-and-braces alongside `frame-ancestors` |

Fonts get a one-year immutable cache; they are content-hashed by filename.

## Keeping it honest

The version, size and date shown in the download card describe a specific
release artifact, and all three are rewritten at build time by the sync script
above rather than typed.

The requirements list next to it is not generated, so it is the part that goes
stale. The Windows 10 1803 floor, the three architectures and the WebView2
dependency all come from the app, not from this page — change them here only
when they change there.

Windows is the only platform that ships. Linux and macOS are listed as coming
soon, and that is a statement about intent, not a date.

The mark in the masthead is the same geometry as the app icon, generated from
`src-tauri/src/packs/brand.rs`. Regenerate the favicon with:

```bash
npm run generate:icon        # app icon
cargo run --manifest-path src-tauri/Cargo.toml --release --bin genpacks -- --icon website/favicon.png 256
```
