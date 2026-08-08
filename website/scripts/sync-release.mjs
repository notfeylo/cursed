/**
 * Rewrites the version, size and release date in `index.html` from the latest
 * GitHub Release.
 *
 *   node scripts/sync-release.mjs
 *
 * The site's CSP is `script-src 'none'`, so the page cannot fetch this itself —
 * and it should not. A hardcoded version is how the README ended up telling
 * people to download 1.1.0 when 1.6.1 was current: publishing a release does not
 * edit prose, so the prose has to be generated.
 *
 * Failing here does not fail the deploy. The committed values are the previous
 * release's, which are stale but true — a site that will not build is worse than
 * a site showing last week's version number.
 */
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const page = join(here, "..", "index.html");
const API = "https://api.github.com/repos/notfeylo/cursorforge/releases/latest";

/** Replaces the text inside `<tag id="…">…</tag>`, leaving attributes alone. */
function setById(html, id, value) {
  const pattern = new RegExp(`(<([a-z]+)[^>]*\\bid="${id}"[^>]*>)([\\s\\S]*?)(</\\2>)`, "i");
  if (!pattern.test(html)) {
    console.warn(`  no element with id="${id}" — skipped`);
    return html;
  }
  return html.replace(pattern, `$1${value}$4`);
}

const response = await fetch(API, {
  headers: {
    Accept: "application/vnd.github+json",
    // GitHub refuses requests without one.
    "User-Agent": "cursed-website-build",
  },
});

if (!response.ok) {
  console.warn(`GitHub API returned ${response.status}; leaving the page as committed.`);
  process.exit(0);
}

const release = await response.json();
const version = String(release.tag_name ?? "").replace(/^v/, "");
const installer = (release.assets ?? []).find((a) => a.name === "Cursed-Setup.exe");

if (!version || !installer) {
  console.warn("No version or Cursed-Setup.exe in the latest release; leaving the page as committed.");
  process.exit(0);
}

const size = `${(installer.size / 1_048_576).toFixed(2)} MB`;
const date = String(release.published_at ?? "").slice(0, 10);

let html = readFileSync(page, "utf8");
html = setById(html, "rel-version", version);
html = setById(html, "rel-size", size);
if (date) html = setById(html, "rel-date", date);

writeFileSync(page, html, "utf8");
console.log(`index.html now reads ${version}, ${size}${date ? `, released ${date}` : ""}.`);
