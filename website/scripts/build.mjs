import { copyFileSync, cpSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { assertLatestRedirect, readReleaseCatalog, releasePaths, validateReleaseFiles } from "./release-tools.mjs";

const scriptsDirectory = dirname(fileURLToPath(import.meta.url));
const sourceDirectory = join(scriptsDirectory, "..");
const outputDirectory = join(sourceDirectory, "dist");

function escapeHtml(value) {
  return String(value)
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;");
}

function setById(html, id, value) {
  const pattern = new RegExp(`(<([a-z]+)[^>]*\\bid="${id}"[^>]*>)([\\s\\S]*?)(</\\2>)`, "i");
  if (!pattern.test(html)) throw new Error(`Missing release field: ${id}`);
  return html.replace(pattern, `$1${escapeHtml(value)}$4`);
}

function setAttributeById(html, id, attribute, value) {
  const pattern = new RegExp(`(<[a-z]+[^>]*\\bid="${id}"[^>]*\\b${attribute}=")([^"]*)(")`, "i");
  if (!pattern.test(html)) throw new Error(`Missing ${attribute} on release field: ${id}`);
  return html.replace(pattern, `$1${escapeHtml(value)}$3`);
}

const catalog = readReleaseCatalog(sourceDirectory);
await validateReleaseFiles(sourceDirectory, catalog);
assertLatestRedirect(sourceDirectory, catalog);
const version = catalog.current;
const release = catalog.releases[version];
const paths = releasePaths(sourceDirectory, catalog, version);

rmSync(outputDirectory, { recursive: true, force: true });
mkdirSync(outputDirectory, { recursive: true });

const posthogToken = process.env.POSTHOG_PROJECT_TOKEN?.trim() ?? "";
const posthogRegion = process.env.POSTHOG_REGION?.trim().toLowerCase() === "eu" ? "eu" : "us";
const analyticsConfig = {
  token: posthogToken,
  endpoint: posthogToken ? `/c7-${posthogRegion}/events` : "",
  version,
};
writeFileSync(
  join(outputDirectory, "analytics-config.js"),
  `window.CURSED_DOWNLOAD_ANALYTICS = ${JSON.stringify(analyticsConfig)};\n`,
  "utf8",
);

let html = readFileSync(join(sourceDirectory, "index.html"), "utf8");
html = setById(html, "rel-version", version);
html = setById(html, "rel-size", `${(release.sizeBytes / 1_048_576).toFixed(2)} MB`);
html = setById(html, "rel-date", release.publishedAt);
html = setById(html, "rel-sha256", release.sha256);
html = setAttributeById(html, "rel-checksum-link", "href", paths.publicChecksum);
writeFileSync(join(outputDirectory, "index.html"), html, "utf8");
copyFileSync(join(sourceDirectory, "404.html"), join(outputDirectory, "404.html"));
copyFileSync(join(sourceDirectory, "privacy.html"), join(outputDirectory, "privacy.html"));
copyFileSync(join(sourceDirectory, "terms.html"), join(outputDirectory, "terms.html"));
copyFileSync(join(sourceDirectory, "installer-return-codes.html"), join(outputDirectory, "installer-return-codes.html"));
copyFileSync(join(sourceDirectory, "press.html"), join(outputDirectory, "press.html"));

for (const file of ["styles.css", "site.js", "download-events.js", "favicon.png", "robots.txt", "sitemap.xml"]) {
  copyFileSync(join(sourceDirectory, file), join(outputDirectory, file));
}
cpSync(join(sourceDirectory, "fonts"), join(outputDirectory, "fonts"), { recursive: true });
cpSync(join(sourceDirectory, "media"), join(outputDirectory, "media"), { recursive: true });
cpSync(join(sourceDirectory, "guides"), join(outputDirectory, "guides"), { recursive: true });
cpSync(join(sourceDirectory, "downloads"), join(outputDirectory, "downloads"), { recursive: true });
cpSync(join(sourceDirectory, ".well-known"), join(outputDirectory, ".well-known"), { recursive: true });

console.log(`Built Cursed ${version} with verified immutable download assets.`);
