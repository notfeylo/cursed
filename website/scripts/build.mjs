import { copyFileSync, cpSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const scriptsDirectory = dirname(fileURLToPath(import.meta.url));
const sourceDirectory = join(scriptsDirectory, "..");
const outputDirectory = join(sourceDirectory, "dist");
const releaseApi = "https://api.github.com/repos/notfeylo/cursed/releases/latest";

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

async function syncRelease(html) {
  try {
    const response = await fetch(releaseApi, {
      headers: { Accept: "application/vnd.github+json", "User-Agent": "cursed-website-build" },
    });
    if (!response.ok) throw new Error(`GitHub API returned ${response.status}`);

    const release = await response.json();
    const version = String(release.tag_name ?? "").replace(/^v/, "");
    const installer = (release.assets ?? []).find((asset) => asset.name === "Cursed-Setup.exe");
    if (!version || !installer) throw new Error("Latest release is missing the standard installer");

    const size = `${(installer.size / 1_048_576).toFixed(2)} MB`;
    const date = String(release.published_at ?? "").slice(0, 10);
    html = setById(html, "rel-version", version);
    html = setById(html, "rel-size", size);
    if (date) html = setById(html, "rel-date", date);
    console.log(`Release details: ${version}, ${size}${date ? `, ${date}` : ""}`);
  } catch (error) {
    console.warn(`Release sync skipped: ${error.message}. Using the committed values.`);
  }
  return html;
}

rmSync(outputDirectory, { recursive: true, force: true });
mkdirSync(outputDirectory, { recursive: true });

let html = readFileSync(join(sourceDirectory, "index.html"), "utf8");
html = await syncRelease(html);
writeFileSync(join(outputDirectory, "index.html"), html, "utf8");

for (const file of ["styles.css", "site.js", "favicon.png"]) {
  copyFileSync(join(sourceDirectory, file), join(outputDirectory, file));
}
cpSync(join(sourceDirectory, "fonts"), join(outputDirectory, "fonts"), { recursive: true });

console.log("Built a production folder containing only public site assets.");
