/**
 * Guards two bugs that were invisible by design.
 *
 *   npm run check:bundle          (after `npm run build`)
 *
 * **1. Fonts that carry no Latin glyphs.**
 * The Google Fonts `css2` response lists one `@font-face` per subset, and the
 * *first* is `cyrillic-ext`. Taking the first `woff2` URL yields a 3 KB file
 * with no Latin coverage at all — every glyph then renders from a fallback
 * face, and the page still looks fine at a glance. Size alone is the giveaway,
 * and `totalSfntSize` in the WOFF2 header is the stronger signal: it is the
 * uncompressed size of the font the file expands to, so it tracks glyph volume
 * rather than compression luck.
 *
 * **2. Fonts leaking into the installer.**
 * A stylesheet is emitted even when the only component importing it is
 * tree-shaken, so declaring candidate faces in `styles.css` shipped all eight
 * inside the build. This asserts the built bundle contains exactly the faces
 * the app uses — no more — and that the dev-only specimen route is absent.
 *
 * **3. A dev-channel binary shipping as the release.**
 * The two channels differ only by a cargo feature, so the wrong `--features`
 * produces an installer that looks right, is named right, and installs an app
 * that writes to `Cursed (Dev)` and refuses to defend the pointer. Every build
 * stamps its channel marker into the executable (`src/channel.rs`), and this
 * asserts the release binaries carry the user channel's and not the dev one's.
 *
 * The *installers* are not searched: NSIS compresses its payload with LZMA, so
 * a marker inside one is not findable as text and a search would pass on every
 * build including a bad one. The uncompressed `.exe` under `target/` is what
 * gets read, and the installer is checked by name.
 */
import { readdirSync, readFileSync, statSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const fontDir = join(root, "src", "assets", "fonts");
const distAssets = join(root, "dist", "assets");

/** Exactly the faces the app uses. Adding one is a deliberate act. */
const EXPECTED_FONTS = [
  "space-grotesk-600",
  "space-grotesk-700",
  "inter-tight-400",
  "inter-tight-500",
  "jetbrains-mono-400",
];

// Comfortably below the smallest real latin subset we ship (12,840 bytes /
// 32,204 uncompressed) and far above a cyrillic-ext subset (~3-4 KB).
const MIN_FILE_BYTES = 8_000;
const MIN_SFNT_BYTES = 20_000;

/** Strings that only exist in the dev-only specimen module. */
const SPECIMEN_MARKERS = ["CURSED — SPECIMEN", "Deliberate fallback", "Long-text torture"];

let failures = 0;
const fail = (message) => {
  console.error(`  FAIL  ${message}`);
  failures += 1;
};

console.log("Fonts in the source tree");
const sourceFonts = readdirSync(fontDir).filter((f) => f.endsWith(".woff2"));

for (const name of sourceFonts) {
  const bytes = readFileSync(join(fontDir, name));

  if (bytes.subarray(0, 4).toString("latin1") !== "wOF2") {
    fail(`${name} is not a WOFF2 file`);
    continue;
  }

  // WOFF2 header: signature, flavor, length, numTables, reserved, totalSfntSize.
  const totalSfntSize = bytes.readUInt32BE(16);

  if (bytes.length < MIN_FILE_BYTES) {
    fail(
      `${name} is ${bytes.length} bytes, under ${MIN_FILE_BYTES} — almost certainly a non-latin subset`,
    );
  } else if (totalSfntSize < MIN_SFNT_BYTES) {
    fail(
      `${name} expands to ${totalSfntSize} bytes, under ${MIN_SFNT_BYTES} — too few glyphs for a latin subset`,
    );
  } else {
    console.log(`  ok  ${name.padEnd(26)} ${bytes.length} bytes, ${totalSfntSize} uncompressed`);
  }
}

for (const expected of EXPECTED_FONTS) {
  if (!sourceFonts.some((f) => f.startsWith(expected))) {
    fail(`${expected}.woff2 is missing from src/assets/fonts`);
  }
}

// An orphan here is dead weight and usually the leftover half of an abandoned
// swap — the state the tree was in mid-way through choosing this pairing.
for (const name of sourceFonts) {
  if (!EXPECTED_FONTS.some((e) => name.startsWith(e))) {
    fail(`${name} is in src/assets/fonts but no face uses it — delete it or add it to EXPECTED_FONTS`);
  }
}

// ── the shipped binaries belong to the user channel ──────────────
//
// Written out rather than parsed from channel.rs so that changing a marker is
// a deliberate act in two places, the way EXPECTED_FONTS is. The assertion
// below keeps the two from drifting apart silently.
const USER_MARKER = "CURSED-CHANNEL:USER";
const DEV_MARKER = "CURSED-CHANNEL:DEV";

console.log("\nChannel markers");
const channelSource = readFileSync(join(root, "src-tauri", "src", "channel.rs"), "utf8");
for (const marker of [USER_MARKER, DEV_MARKER]) {
  if (!channelSource.includes(`"${marker}"`)) {
    fail(`src/channel.rs no longer defines ${marker} — this guard is searching for a string nothing emits`);
  }
}

// Every release directory a build could have produced. `target/release` is the
// host build — which is the one CI produces, and leaving it out was a guard
// that ran on every push and inspected nothing.
const target = join(root, "src-tauri", "target");
const releaseDirs = [join(target, "release")];
try {
  for (const entry of readdirSync(target)) {
    releaseDirs.push(join(target, entry, "release"));
  }
} catch {
  // No target/ at all: nothing has been built here yet.
}

// Only the binary that ships. `genpacks.exe` is the offline catalog tool and is
// not part of any installer, and `Cursed Dev.exe` is the dev channel's own
// bundle — built on purpose, correctly marked, and never staged for release.
const SHIPPED = "Cursed.exe";
const markerAge = statSync(join(root, "src-tauri", "src", "channel.rs")).mtimeMs;

let checked = 0;
let stale = 0;
for (const dir of releaseDirs) {
  const exe = join(dir, SHIPPED);
  let stat;
  try {
    stat = statSync(exe);
  } catch {
    continue;
  }
  const text = readFileSync(exe, "latin1");

  if (text.includes(DEV_MARKER)) {
    fail(`${exe} is a DEV-CHANNEL build — it must not be released`);
  } else if (text.includes(USER_MARKER)) {
    checked += 1;
    console.log(`  ok  ${USER_MARKER}  ${dir}`);
  } else if (stat.mtimeMs < markerAge) {
    // Built before channel.rs existed, so it cannot carry a marker and its
    // absence proves nothing. Said out loud rather than skipped silently: a
    // guard that quietly inspects nothing reads exactly like one that passed.
    stale += 1;
    console.log(`  --  ${exe}\n      predates the channel marker; rebuild to have it checked`);
  } else {
    // Newer than the marker and still without one: something is emitting a
    // binary this guard cannot vouch for.
    fail(`${exe} carries no channel marker despite being built after channel.rs`);
  }
}
if (checked === 0 && stale === 0) {
  console.log("  --  no release binary built yet; nothing to check");
}

// The staging directory only ever holds what a release publishes.
try {
  for (const name of readdirSync(join(root, "dist-release"))) {
    if (/dev/i.test(name)) fail(`dist-release/${name} is named as a dev-channel artifact`);
  }
} catch {
  // Nothing staged; `npm run release` has not been run since the last clean.
}

let dist;
try {
  dist = readdirSync(distAssets);
} catch {
  console.log("\nNo dist/ — run `npm run build` first to check the bundle.");
  process.exit(failures === 0 ? 0 : 1);
}

console.log("\nFonts in the built bundle");
const shipped = dist.filter((f) => f.endsWith(".woff2"));

for (const name of shipped) {
  // Vite appends a content hash: `inter-tight-400-iW8qmuJY.woff2`.
  const known = EXPECTED_FONTS.some((e) => name.startsWith(e));
  if (known) {
    console.log(`  ok  ${name}`);
  } else {
    fail(`${name} is in the build but is not a face the app uses`);
  }
}

if (shipped.length !== EXPECTED_FONTS.length) {
  fail(`the build ships ${shipped.length} fonts; ${EXPECTED_FONTS.length} were expected`);
}

console.log("\nSpecimen must not reach production");
const js = dist.filter((f) => f.endsWith(".js"));
for (const name of js) {
  const source = readFileSync(join(distAssets, name), "utf8");
  const found = SPECIMEN_MARKERS.filter((m) => source.includes(m));
  if (found.length > 0) {
    fail(`${name} contains the specimen route (${found.join(", ")})`);
  }
}
if (failures === 0) console.log(`  ok  none of ${js.length} bundle(s) mention it`);

console.log(
  failures === 0 ? "\nAll bundle checks passed." : `\n${failures} check(s) failed.`,
);
process.exit(failures === 0 ? 0 : 1);
