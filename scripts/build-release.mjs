/**
 * Builds every installer a release ships, and stages them with checksums.
 *
 *   node scripts/build-release.mjs
 *
 * ## Why there is more than one
 *
 * **Three architectures.** Windows is not one target. A 32-bit Windows 10 still
 * exists and cannot run an x64 build at all; an ARM64 PC can run one, but only
 * through emulation, paying for it in speed and battery on the machines least
 * able to spare either. One installer per architecture is the only way "it runs
 * on your Windows" is true rather than mostly true.
 *
 * **Two WebView2 strategies.** Cursed's window is Edge WebView2, which is
 * present on every Windows 11 and has shipped to Windows 10 through Windows
 * Update for years — so for nearly everyone the runtime is already there and the
 * installer only has to check. `downloadBootstrapper` does that in about 37 MB.
 *
 * `offlineInstaller` embeds the whole runtime instead: ~211 MB that installs
 * with no network at all. That is the right answer for a machine behind a
 * corporate proxy, an air-gapped PC, or a Windows 10 install old enough never to
 * have received the runtime — and the wrong answer for everyone else, because
 * the in-app updater downloads in the background, unasked. Making that download
 * 211 MB instead of 37 MB would be a quiet tax on anyone paying for their data.
 *
 * So the small one is the default and the one the updater matches, and the
 * offline one is published beside it for the people who need it. The updater's
 * `is_our_installer` only accepts `Cursed_<version>_<arch>-setup.exe`, so the
 * offline build is named to fall outside that on purpose — it must never be
 * chosen automatically.
 */
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { copyFileSync, mkdirSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const stage = join(root, "dist-release");
const version = JSON.parse(readFileSync(join(root, "package.json"), "utf8")).version;

/**
 * Rust target triple -> the arch tag Tauri puts in the bundle filename, and the
 * unversioned name it is also published under.
 *
 * Both names ship. The versioned one is what the in-app updater looks for; the
 * alias is what the website and README link to, so a release does not silently
 * turn every download link on the site into a 404 the moment it is published.
 */
const TARGETS = [
  { triple: "x86_64-pc-windows-msvc", arch: "x64", alias: "Cursed-Setup.exe" },
  { triple: "aarch64-pc-windows-msvc", arch: "arm64", alias: "Cursed-Setup-ARM64.exe" },
  { triple: "i686-pc-windows-msvc", arch: "x86", alias: "Cursed-Setup-x86.exe" },
];

const run = (args, env) =>
  execFileSync("npm", ["run", "tauri", "build", "--", ...args], {
    cwd: root,
    stdio: "inherit",
    shell: true,
    env: { ...process.env, ...env },
  });

rmSync(stage, { recursive: true, force: true });
mkdirSync(stage, { recursive: true });

const shipped = [];
const keep = (from, name) => {
  copyFileSync(from, join(stage, name));
  shipped.push(name);
  console.log(`  staged  ${name}  (${(statSync(from).size / 1_048_576).toFixed(1)} MB)`);
};

for (const { triple, arch, alias } of TARGETS) {
  console.log(`\n=== ${arch} (${triple}) ===`);
  run(["--target", triple, "--bundles", "nsis"]);
  const built = join(root, "src-tauri", "target", triple, "release", "bundle", "nsis");
  const name = `Cursed_${version}_${arch}-setup.exe`;
  if (!readdirSync(built).includes(name)) {
    throw new Error(`expected ${name} in ${built}, found: ${readdirSync(built).join(", ")}`);
  }
  keep(join(built, name), name);
  keep(join(built, name), alias);
}

// The offline build, x64 only. Anyone on a network locked down enough to need
// it is overwhelmingly on an ordinary x64 desktop, and a second 211 MB asset per
// architecture is a lot of release weight to carry on a guess.
console.log(`\n=== x64, offline (WebView2 runtime embedded) ===`);
// Written to a file rather than passed inline: the override is JSON, JSON is
// full of double quotes, and this spawns through a shell on Windows.
const override = join(stage, "offline.tauri.conf.json");
writeFileSync(
  override,
  JSON.stringify({ bundle: { windows: { webviewInstallMode: { type: "offlineInstaller", silent: true } } } }),
  "utf8",
);
run(["--target", "x86_64-pc-windows-msvc", "--bundles", "nsis", "--config", override]);
rmSync(override, { force: true });
keep(
  join(root, "src-tauri", "target", "x86_64-pc-windows-msvc", "release", "bundle", "nsis", `Cursed_${version}_x64-setup.exe`),
  "Cursed-Setup-Offline-x64.exe",
);

const sums = shipped
  .sort()
  .map((name) => `${createHash("sha256").update(readFileSync(join(stage, name))).digest("hex")}  ${name}`)
  .join("\n");
writeFileSync(join(stage, "SHA256SUMS.txt"), `${sums}\n`, "utf8");

console.log(`\n${shipped.length} installers staged in dist-release/ for ${version}.\n\n${sums}`);
