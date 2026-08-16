/**
 * Builds the development channel's installer.
 *
 *   npm run build:dev
 *
 * Two things have to be true at once, and neither is checked by the build: the
 * cargo feature `dev-channel` must be on, and `src-tauri/dev.tauri.conf.json`
 * must be merged over the normal config. Get the first without the second and
 * the app writes to `Cursed (Dev)` while installing *over the released copy*,
 * replacing the thing being tested with the thing being tested against. Get the
 * second without the first and the reverse: a separate install that fights the
 * real one for the pointer and shares its data directory.
 *
 * Both are silent. So the build is followed by an assertion that the artifact
 * that came out is the one that was asked for.
 */
import { execFileSync } from "node:child_process";
import { readFileSync, readdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const version = JSON.parse(readFileSync(join(root, "package.json"), "utf8")).version;
const config = join(root, "src-tauri", "dev.tauri.conf.json");

const USER_MARKER = "CURSED-CHANNEL:USER";
const DEV_MARKER = "CURSED-CHANNEL:DEV";

console.log(`Building the dev channel for ${version}\n`);

execFileSync(
  "npm",
  ["run", "tauri", "build", "--", "--features", "dev-channel", "--config", config, "--bundles", "nsis"],
  { cwd: root, stdio: "inherit", shell: true },
);

const releaseDir = join(root, "src-tauri", "target", "release");
const problems = [];

// The binary. `Cursed Dev.exe` is `mainBinaryName` from the override file, so
// its mere existence proves the config was merged.
const exe = join(releaseDir, "Cursed Dev.exe");
let binary;
try {
  binary = readFileSync(exe, "latin1");
} catch {
  problems.push(
    `${exe} was not produced — dev.tauri.conf.json was not merged, and the build just overwrote the released binary`,
  );
}

if (binary !== undefined) {
  if (binary.includes(USER_MARKER)) {
    problems.push("the binary carries the USER marker — --features dev-channel did not take effect");
  }
  if (!binary.includes(DEV_MARKER)) {
    problems.push("the binary carries no dev marker — it was not built as the dev channel");
  }
}

// The installer.
const nsis = join(releaseDir, "bundle", "nsis");
let installer;
try {
  installer = readdirSync(nsis).find((f) => f.startsWith("Cursed Dev") && f.endsWith("-setup.exe"));
} catch {
  problems.push(`no NSIS output in ${nsis}`);
}
if (installer === undefined && problems.length === 0) {
  problems.push(`no "Cursed Dev...-setup.exe" in ${nsis}`);
}

if (problems.length > 0) {
  console.error(`\n${problems.length} problem(s) with the build:`);
  for (const problem of problems) console.error(`  FAIL  ${problem}`);
  console.error("\nDo not install this. Fix the invocation and build again.");
  process.exit(1);
}

console.log(`\n  ok  ${exe}`);
console.log(`  ok  ${join(nsis, installer)}`);
console.log("\nThe dev channel installs alongside the released app, with its own");
console.log("data directory, its own tray icon, and no claim on the pointer.");
