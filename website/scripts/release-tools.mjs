import { createHash } from "node:crypto";
import { createReadStream, existsSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";

const versionPattern = /^\d+\.\d+\.\d+$/;
const hashPattern = /^[a-f0-9]{64}$/;

export function readReleaseCatalog(sourceDirectory) {
  const catalogPath = join(sourceDirectory, "release.json");
  const catalog = JSON.parse(readFileSync(catalogPath, "utf8"));

  if (!versionPattern.test(catalog.current ?? "")) throw new Error("release.json has an invalid current version");
  if (catalog.fileName !== "Cursed-Setup.exe") throw new Error("release.json must use Cursed-Setup.exe");
  if (!catalog.releases || typeof catalog.releases !== "object") throw new Error("release.json is missing releases");

  for (const [version, release] of Object.entries(catalog.releases)) {
    if (!versionPattern.test(version)) throw new Error(`Invalid release version: ${version}`);
    if (!/^\d{4}-\d{2}-\d{2}$/.test(release.publishedAt ?? "")) throw new Error(`Invalid release date for ${version}`);
    if (!Number.isSafeInteger(release.sizeBytes) || release.sizeBytes <= 0) throw new Error(`Invalid installer size for ${version}`);
    if (!hashPattern.test(release.sha256 ?? "")) throw new Error(`Invalid SHA256 for ${version}`);
  }

  if (!catalog.releases[catalog.current]) throw new Error("Current version is missing from releases");
  return catalog;
}

export function releasePaths(sourceDirectory, catalog, version) {
  const directory = join(sourceDirectory, "downloads", version);
  return {
    directory,
    installer: join(directory, catalog.fileName),
    checksum: join(directory, "SHA256SUMS.txt"),
    publicInstaller: `/downloads/${version}/${catalog.fileName}`,
    publicChecksum: `/downloads/${version}/SHA256SUMS.txt`,
  };
}

export function checksumLine(catalog, release) {
  return `${release.sha256}  ${catalog.fileName}\n`;
}

export async function sha256File(path) {
  const hash = createHash("sha256");
  await new Promise((resolve, reject) => {
    const stream = createReadStream(path);
    stream.on("data", (chunk) => hash.update(chunk));
    stream.on("error", reject);
    stream.on("end", resolve);
  });
  return hash.digest("hex");
}

export async function validateReleaseFiles(sourceDirectory, catalog) {
  for (const [version, release] of Object.entries(catalog.releases)) {
    const paths = releasePaths(sourceDirectory, catalog, version);
    if (!existsSync(paths.installer)) {
      throw new Error(`Missing immutable installer: downloads/${version}/${catalog.fileName}`);
    }

    const actualSize = statSync(paths.installer).size;
    if (actualSize !== release.sizeBytes) {
      throw new Error(`Installer size changed for ${version}: expected ${release.sizeBytes}, found ${actualSize}`);
    }

    const actualHash = await sha256File(paths.installer);
    if (actualHash !== release.sha256) {
      throw new Error(`Installer hash changed for ${version}: immutable release files must never be overwritten`);
    }

    if (!existsSync(paths.checksum)) throw new Error(`Missing checksum file for ${version}`);
    const actualChecksum = readFileSync(paths.checksum, "utf8").replaceAll("\r\n", "\n");
    if (actualChecksum !== checksumLine(catalog, release)) throw new Error(`Checksum file is stale for ${version}`);
  }
}

export function assertLatestRedirect(sourceDirectory, catalog) {
  const config = JSON.parse(readFileSync(join(sourceDirectory, "vercel.json"), "utf8"));
  const redirect = (config.redirects ?? []).find((entry) => entry.source === "/download");
  const expected = releasePaths(sourceDirectory, catalog, catalog.current).publicInstaller;
  if (!redirect || redirect.destination !== expected || redirect.permanent !== false) {
    throw new Error(`vercel.json /download redirect must be temporary and point to ${expected}`);
  }
}
