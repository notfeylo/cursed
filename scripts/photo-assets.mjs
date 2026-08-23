/**
 * Stages — and optionally signs — the C++ runtime that photo mode downloads.
 *
 *     node scripts/photo-assets.mjs            stage and verify
 *     node scripts/photo-assets.mjs --sign     and sign, with the release key
 *
 * ## Why this script exists at all
 *
 * `photo-v1` was published by hand and its four artifacts were correct. This
 * adds ten more — three architectures of a C++ runtime whose filenames must be
 * identical to each other and whose asset names must not be — and doing that by
 * hand is how one architecture ends up publishing another architecture's
 * library under its name.
 *
 * The important part is not the copying. It is that **the bytes staged here are
 * checked against the constants compiled into the app** before anything is
 * signed or uploaded. `src-tauri/src/photo.rs` is the authority: if this machine
 * holds a different build of `msvcp140.dll` than the one whose hash is in the
 * source, this refuses, rather than publishing an artifact that every copy of
 * the app would then reject as corrupt.
 *
 * ## Where the files come from
 *
 * The Visual C++ redistributable directory that ships with Visual Studio and
 * with the Build Tools. Microsoft publishes those files for deployment
 * alongside an application, which is what that directory is for;
 * `docs/PHOTO_MODE.md` records the licence and the provenance.
 */
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { createRequire } from "node:module";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const source = join(root, "src-tauri", "src", "photo.rs");
const staging = join(root, "dist-photo");

/** The release the app fetches from. Read, rather than repeated here. */
const tag = readFileSync(source, "utf8").match(/ARTIFACT_TAG: &str = "([^"]+)"/)?.[1];
if (!tag) {
  console.error("could not read ARTIFACT_TAG from photo.rs");
  process.exit(1);
}

/**
 * Every artifact the app expects, read out of the app itself.
 *
 * The test module is cut off first: it builds `Artifact` values with an empty
 * checksum deliberately, to prove those are refused, and staging one of them
 * would mean staging a file with nothing to check it against.
 */
function artifacts() {
  const text = readFileSync(source, "utf8").split("#[cfg(test)]")[0];
  const pattern =
    /Artifact\s*\{\s*name:\s*"([^"]+)",\s*asset:\s*"([^"]+)",\s*sha256:\s*"([^"]*)",\s*bytes:\s*([0-9_]+),\s*\}/g;
  return [...text.matchAll(pattern)].map((m) => ({
    name: m[1],
    asset: m[2],
    sha256: m[3],
    bytes: Number(m[4].replaceAll("_", "")),
  }));
}

/** The architecture an asset name is tagged with. */
function archOf(asset) {
  const arch = asset.match(/-(x64|arm64|x86)\.dll$/)?.[1];
  if (!arch) throw new Error(`${asset} is not tagged with an architecture`);
  return arch;
}

/**
 * Finds every redistributable copy of one file, for one architecture.
 *
 * Searched rather than configured: the version sits in the path and moves with
 * every Visual Studio update, so a pinned path is a script that breaks silently
 * a few months from now. Every candidate found is offered to the caller, which
 * picks by hash.
 */
function findRedist(arch, file) {
  const found = [];
  for (const base of ["C:/Program Files", "C:/Program Files (x86)"]) {
    const vs = join(base, "Microsoft Visual Studio");
    if (!existsSync(vs)) continue;
    for (const year of readdirSync(vs)) {
      const yearDir = join(vs, year);
      let editions = [];
      try {
        editions = readdirSync(yearDir);
      } catch {
        continue;
      }
      for (const edition of editions) {
        const msvc = join(yearDir, edition, "VC", "Redist", "MSVC");
        if (!existsSync(msvc)) continue;
        for (const version of readdirSync(msvc)) {
          const dir = join(msvc, version, arch);
          if (!existsSync(dir)) continue;
          for (const crt of readdirSync(dir).filter((d) => /\.CRT$/i.test(d))) {
            const candidate = join(dir, crt, file);
            if (existsSync(candidate)) found.push(candidate);
          }
        }
      }
    }
  }
  return found;
}

const sha256 = (path) => createHash("sha256").update(readFileSync(path)).digest("hex");

/* ── stage ─────────────────────────────────────────────────────── */

const wanted = artifacts().filter((a) => /^(msvcp|vcruntime)140/.test(a.name));
if (wanted.length === 0) {
  console.error("photo.rs lists no C++ runtime artifacts; nothing to stage.");
  process.exit(1);
}

rmSync(staging, { recursive: true, force: true });
mkdirSync(staging, { recursive: true });

const staged = [];
let failures = 0;
for (const artifact of wanted) {
  const candidates = findRedist(archOf(artifact.asset), artifact.name);
  // **Matched by hash, never by taking the newest.** Two Visual Studio
  // installations can each hold a different build under the same filename, and
  // the only one that may be published is the one the app was compiled against.
  const match = candidates.find((c) => sha256(c) === artifact.sha256);

  if (!match) {
    failures += 1;
    console.error(`  MISSING  ${artifact.asset}`);
    console.error(`           want ${artifact.sha256}`);
    if (candidates.length === 0) {
      console.error(`           nothing on this machine holds ${artifact.name} for that arch`);
    } else {
      for (const c of candidates) console.error(`           have ${sha256(c)}  ${c}`);
    }
    continue;
  }

  const destination = join(staging, artifact.asset);
  copyFileSync(match, destination);
  const size = readFileSync(destination).length;
  if (size !== artifact.bytes) {
    failures += 1;
    console.error(`  SIZE     ${artifact.asset}: staged ${size}, expected ${artifact.bytes}`);
    continue;
  }
  staged.push({ ...artifact, path: destination });
  console.log(`  ok       ${artifact.asset}  ${size} bytes`);
}

if (failures > 0) {
  console.error(
    `\n${failures} artifact(s) could not be staged, and nothing was signed.\n` +
      "Either this machine holds a different build of the redistributable than\n" +
      "photo.rs was written against, or the constants there are wrong. Both are\n" +
      "worth settling before publishing: a mismatch here becomes an artifact that\n" +
      "every copy of the app refuses as corrupt.",
  );
  process.exit(1);
}

/* ── sign ──────────────────────────────────────────────────────── */

if (process.argv.includes("--sign")) {
  const privateKey = process.env.TAURI_SIGNING_PRIVATE_KEY?.trim();
  if (!privateKey) {
    console.error("\nTAURI_SIGNING_PRIVATE_KEY is not set. See docs/SIGNING.md.");
    process.exit(1);
  }
  // Resolved and run through this Node rather than through `npx`, for the
  // reason `sign-release.mjs` sets out at length: Node will not spawn a `.cmd`
  // without a shell, and a shell is the thing that must not be involved.
  const tauriCli = createRequire(join(root, "package.json")).resolve("@tauri-apps/cli/tauri.js");
  console.log("");
  for (const artifact of staged) {
    execFileSync(process.execPath, [tauriCli, "signer", "sign", artifact.path], {
      stdio: ["ignore", "pipe", "pipe"],
      env: {
        ...process.env,
        TAURI_SIGNING_PRIVATE_KEY: privateKey,
        TAURI_SIGNING_PRIVATE_KEY_PASSWORD: process.env.TAURI_SIGNING_PRIVATE_KEY_PASSWORD ?? "",
      },
    });
    // Tauri writes `<file>.sig`, holding the minisign file base64'd a second
    // time because its own updater decodes that layer before verifying. This
    // project does not use that updater — `signing::verify` reads plain
    // minisign — so the extra layer comes off here, exactly as the release
    // signer does it.
    const written = `${artifact.path}.sig`;
    const raw = readFileSync(written, "utf8").trim();
    const minisig = raw.startsWith("untrusted comment:")
      ? raw
      : Buffer.from(raw, "base64").toString("utf8");
    writeFileSync(`${artifact.path}.minisig`, minisig);
    rmSync(written, { force: true });
    console.log(`  signed   ${artifact.asset}.minisig`);
  }
}

/* ── what to do with it ────────────────────────────────────────── */

const signed = existsSync(join(staging, `${staged[0].asset}.minisig`));
console.log(`\n${staged.length} artifacts staged in dist-photo${signed ? ", and signed" : ""}.`);
if (!signed) {
  console.log("\nSign them on the machine that holds the key:");
  console.log('  TAURI_SIGNING_PRIVATE_KEY="$(cat ~/.cursed/cursed.key)" \\');
  console.log("  TAURI_SIGNING_PRIVATE_KEY_PASSWORD=... \\");
  console.log("  node scripts/photo-assets.mjs --sign");
}
console.log(`\nUpload, keeping the ${tag} tag the app is compiled to fetch from:`);
console.log(`  gh release upload ${tag} dist-photo/*`);
