# cursorforge.vercel.app

The landing page. Static HTML and CSS, no build step, no JavaScript.

```bash
vercel deploy --prod          # from this directory
```

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

The version, size and SHA-256 shown in the download card describe a specific
release artifact. When a new version ships, all three change — and the checksum
must be the one you get by downloading the published asset and hashing it, not
the one from a local build. A checksum nobody can reproduce is worse than none.

The mark in the masthead is the same geometry as the app icon, generated from
`src-tauri/src/packs/brand.rs`. Regenerate the favicon with:

```bash
npm run generate:icon        # app icon
cargo run --manifest-path src-tauri/Cargo.toml --release --bin genpacks -- --icon website/favicon.png 256
```
