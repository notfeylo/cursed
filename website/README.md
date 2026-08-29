# cursorforge.vercel.app

The Cursed landing page is a small static site: logo and hero, About, Download,
and a compact footer with in-page Privacy and Terms dialogs.

## Build and deploy

```bash
node scripts/build.mjs
vercel --prod
```

The build script reads the latest public GitHub release to fill in the version,
installer size, and release date. If GitHub is unavailable, it keeps the true
values committed in `index.html`.

Only `dist` is deployed. It contains `index.html`, `styles.css`, `site.js`, the
favicon, and local font files. Source notes, build scripts, Vercel configuration,
and unused media are deliberately excluded from production.

## Privacy and security

The site has no cookies, analytics, forms, accounts, external scripts, or
third-party embeds. Privacy and Terms use the native HTML dialog element and the
small same-origin `site.js` file. Vercel headers deny framing, referrers,
powerful browser permissions, network connections from scripts, and every
resource type the page does not use.

The app itself also has no telemetry. GitHub release download counts can show
rough installer downloads, but the project cannot report completed installs or
active users without adding an explicit, opt-in usage signal and updating the
privacy policy.
