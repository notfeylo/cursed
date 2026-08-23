# cursorforge.vercel.app

The landing page. One HTML file, one stylesheet, one small script.

```bash
vercel deploy --prod          # from this directory
```

## One page, four blocks

Title, about, download, rating — plus the footer. Every anchor in the header and
the footer points at a section of this page; there are no other routes.

## The version number syncs itself

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

## The two things that move

Everything else is still: no drifting background, no scroll reveals, no orbit,
no hover transitions, no smooth scrolling.

### 1. The laptop lid

It is closed and swings open as the about section scrolls into view, using a
**scroll-driven CSS animation** (`animation-timeline: view()`). Two things to
know before editing `styles.css`:

- The **open** lid is the default. The closed state lives inside
  `@supports (animation-timeline: view())` and
  `@media (prefers-reduced-motion: no-preference)`, so a browser that cannot
  drive the timeline — or a reader who has asked for less motion — gets a laptop
  that is simply open. Written the other way round they would get one welded
  shut.
- `body` must not set `overflow-x`. `overflow-x: hidden` makes the body a scroll
  container, and a `view()` timeline measured against it stops working. The
  footer clips its own oversized wordmark instead.

The machine is drawn in CSS, not photographed: an aluminium lid with hairline
edge highlights, a notch with a camera in it hanging into the display, a menu
strip that content scrolls *under* so nothing slides out from behind the notch,
and a keyboard deck that is a real plane at `rotateX(74deg)` with a key grid and
a trackpad on it. Below 900px all of that is hidden and the about text becomes
ordinary page content — a phone-width screen with its own scrollbar inside it is
worse than no laptop at all.

### 2. The cursor

`cursor.js` swaps the pointer image on a timer. That is what an animated cursor
*is* — `.ani` is a list of frames — and CSS cannot do it: `cursor` is not an
animatable property, and no browser plays an animated GIF or an `.ani` used as
one.

It is **not** a `<div>` chasing the mouse. That is the single thing the app
refuses to do, because a sprite in an overlay window is composited by the desktop
and trails the real pointer permanently. Setting `cursor` hands the image to the
OS, which draws it on the hardware cursor plane with no lag — the about section's
own argument, applied to the page making it.

`styles.css` sets frame one, so the page keeps a working pointer if the script
never runs.

## Why there is a CSP, and why it changed

`vercel.json` sets `script-src 'self'`. It used to be `'none'` — the page shipped
no JavaScript at all — and the animated cursor is what changed that. `'self'` is
the narrowest setting that still allows `cursor.js`: no inline script, no
`eval`, no third-party origin. If the cursor animation is ever dropped, put it
back to `'none'` in the same commit.

`default-src` is `'none'` with each resource type opened individually, so a new
kind of subresource has to be allowed deliberately rather than inherited.

## Headers

Set in `vercel.json`:

| Header | Why |
| --- | --- |
| `Content-Security-Policy` | One same-origin script, no objects, no framing, no forms |
| `Strict-Transport-Security` | Two years, subdomains, preload-eligible |
| `Cross-Origin-Opener-Policy` | Isolates the browsing context |
| `Cross-Origin-Embedder-Policy` | `require-corp`; every asset here is same-origin |
| `Cross-Origin-Resource-Policy` | Nothing here is meant to be embedded elsewhere |
| `Referrer-Policy` | `no-referrer` — the page has nobody to tell |
| `Permissions-Policy` | Every powerful feature switched off by name |
| `X-Content-Type-Options` | `nosniff` |
| `X-Frame-Options` | Belt-and-braces alongside `frame-ancestors` |

Fonts get a one-year immutable cache; they are content-hashed by filename.

## The rating widget stores nothing

The five stars are `<input type="radio">` elements with no form around them, so
picking one is state the browser holds and CSS reads back through
`input:checked ~ label`. Nothing is submitted, nothing is counted, and the page
never claims an average.

That is deliberate rather than unfinished: an average shown to visitors has to
come from real ratings, and there is nowhere to keep them. Wiring this to a form
service is the change that makes an aggregate honest — **do not add one before
then.**

## Keeping it honest

The version, size and date in the download card are rewritten at build time by
the sync script rather than typed.

The requirements next to them — processor, memory, storage — are **not**
generated, so they are the part that goes stale. They describe the app, not this
page; change them here only when they change there.

Windows is the only platform that ships. Linux and macOS are listed as coming
soon, which is a statement of intent and not a date.

The mark in the masthead is the same geometry as the app icon, generated from
`src-tauri/src/packs/brand.rs`. Regenerate the favicon with:

```bash
cargo run --manifest-path src-tauri/Cargo.toml --release --bin genpacks -- --icon website/favicon.png 256
```
