# trycursed.com

The Cursed landing page is a small static site: logo and hero, About, Download,
and a compact footer with in-page Privacy and Terms dialogs.

## Build and deploy

```bash
node scripts/build.mjs
vercel --prod
```

`release.json` is the source of truth for the current download and every
immutable Store release. The build verifies each installer byte-for-byte,
checks its committed checksum, fills the homepage version and checksum, and
checks that `/download` points to the current versioned file.

Only `dist` is deployed. It contains the homepage, branded 404 page, guide
library, stylesheet, small same-origin dialog script, favicon, local font files,
`robots.txt`, and `sitemap.xml`. Source notes, build scripts, Vercel
configuration, and unused media are deliberately excluded from production.
`www.trycursed.com` is the canonical address; the apex domain and old Vercel
hostname redirect there permanently. Use the `www` version of an immutable
download URL in Microsoft Partner Center so the submitted URL itself returns
the binary without a domain redirect.

## Add a Store release

1. Create `downloads/<version>/` without changing an older folder.
2. Put the standard x64 installer at `downloads/<version>/Cursed-Setup.exe`.
3. Run `node scripts/prepare-download.mjs <version> YYYY-MM-DD`.
4. Run `node scripts/build.mjs`. The build stops if any historical binary was
   changed, a checksum is stale, or `/download` points at the wrong version.
5. Commit the new version folder, `release.json`, the generated checksum, and
   the generated `vercel.json` redirect together, then deploy.

The current Partner Center URL will be:
`https://www.trycursed.com/downloads/1.27.0/Cursed-Setup.exe`.

## Privacy and security

The site has no cookies, forms, accounts, external scripts, or third-party
embeds. Privacy and Terms use the native HTML dialog element and the small
same-origin `site.js` file. A dormant, first-party download-event client is
documented in `POSTHOG.md`; it sends nothing unless a public PostHog project
token is configured at build time. Vercel headers deny framing, referrers,
powerful browser permissions, cross-origin connections from scripts, and every
resource type the page does not use.

The app itself also has no telemetry. GitHub release download counts can show
rough installer downloads, but the project cannot report completed installs or
active users without adding an explicit, opt-in usage signal and updating the
privacy policy.
