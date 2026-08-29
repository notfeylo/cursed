import { mkdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { checksumLine, readReleaseCatalog, releasePaths, sha256File } from "./release-tools.mjs";

const scriptsDirectory = dirname(fileURLToPath(import.meta.url));
const sourceDirectory = join(scriptsDirectory, "..");
const version = process.argv[2];
const publishedAt = process.argv[3] ?? new Date().toISOString().slice(0, 10);

if (!/^\d+\.\d+\.\d+$/.test(version ?? "")) {
  throw new Error("Usage: node scripts/prepare-download.mjs <version> [YYYY-MM-DD]");
}
if (!/^\d{4}-\d{2}-\d{2}$/.test(publishedAt)) throw new Error("Release date must use YYYY-MM-DD");

const catalog = readReleaseCatalog(sourceDirectory);
const paths = releasePaths(sourceDirectory, catalog, version);
mkdirSync(paths.directory, { recursive: true });

let installerSize;
try {
  installerSize = statSync(paths.installer).size;
} catch {
  throw new Error(`Place the installer at downloads/${version}/${catalog.fileName}, then run this command again`);
}

const installerHash = await sha256File(paths.installer);
const existing = catalog.releases[version];
if (existing && (existing.sizeBytes !== installerSize || existing.sha256 !== installerHash)) {
  throw new Error(`Version ${version} already exists with a different binary. Create a new version instead of overwriting it.`);
}

catalog.releases[version] = existing ?? { publishedAt, sizeBytes: installerSize, sha256: installerHash };
catalog.current = version;
writeFileSync(join(sourceDirectory, "release.json"), `${JSON.stringify(catalog, null, 2)}\n`, "utf8");
writeFileSync(paths.checksum, checksumLine(catalog, catalog.releases[version]), "utf8");

const configPath = join(sourceDirectory, "vercel.json");
const config = JSON.parse(readFileSync(configPath, "utf8"));
const redirect = (config.redirects ?? []).find((entry) => entry.source === "/download");
if (!redirect) throw new Error("vercel.json is missing the generated /download redirect");
redirect.destination = paths.publicInstaller;
redirect.permanent = false;
writeFileSync(configPath, `${JSON.stringify(config, null, 2)}\n`, "utf8");

console.log(`Prepared Cursed ${version}`);
console.log(`Installer: ${paths.publicInstaller}`);
console.log(`SHA256: ${catalog.releases[version].sha256}`);
console.log("Existing versioned releases were preserved.");
