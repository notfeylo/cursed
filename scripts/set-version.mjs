/**
 * Writes one version into all three files that carry one.
 *
 *   npm run version:set 1.7.0
 *
 * `package.json`, `src-tauri/tauri.conf.json` and `src-tauri/Cargo.toml` each
 * hold the version separately. Bumping them by hand is how they drift, and when
 * they drift the updater compares a version the app is not running against the
 * newest release — so it either offers an update that is already installed or
 * stays quiet about one that is not.
 *
 * Deliberately edits the text rather than reformatting the parsed file: a JSON
 * round-trip would rewrite whitespace and key order across the whole file, and
 * a version bump should be a one-line diff.
 */
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const version = process.argv[2];

if (!version || !/^\d+\.\d+\.\d+$/.test(version)) {
  console.error("usage: npm run version:set <major.minor.patch>");
  process.exit(1);
}

/** @type {{file: string, pattern: RegExp, replace: string}[]} */
const targets = [
  {
    file: "package.json",
    pattern: /("version"\s*:\s*")\d+\.\d+\.\d+(")/,
    replace: `$1${version}$2`,
  },
  {
    file: "src-tauri/tauri.conf.json",
    pattern: /("version"\s*:\s*")\d+\.\d+\.\d+(")/,
    replace: `$1${version}$2`,
  },
  {
    // Only the package's own version, which is the first one in the file.
    // A dependency pinned to "1.2.3" must not be rewritten.
    file: "src-tauri/Cargo.toml",
    pattern: /(\[package\][\s\S]*?\nversion\s*=\s*")\d+\.\d+\.\d+(")/,
    replace: `$1${version}$2`,
  },
];

let failed = false;

for (const { file, pattern, replace } of targets) {
  const path = join(root, file);
  const before = readFileSync(path, "utf8");

  if (!pattern.test(before)) {
    console.error(`  ${file}: no version field matched — not touched`);
    failed = true;
    continue;
  }

  const after = before.replace(pattern, replace);
  if (after === before) {
    console.log(`  ${file}: already ${version}`);
    continue;
  }

  // Written back as UTF-8 with no BOM. A BOM here is not cosmetic: serde_json
  // rejects one, which silently resets every setting the app has.
  writeFileSync(path, after, "utf8");
  console.log(`  ${file}: -> ${version}`);
}

if (failed) {
  console.error("\nSome files were not updated. Fix them before releasing.");
  process.exit(1);
}

console.log(`\nAll three files now say ${version}.`);
console.log("`cargo test` asserts they agree, so a miss fails the build.");
