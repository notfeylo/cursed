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

// ── the update path, checked without a VM ────────────────────────
//
// Everything below exists because the rows that would really prove the update
// path need a Windows VM rolled back to a clean snapshot, and there is not one.
// These are the parts of it that can be asserted against build output on the
// machine that produced it — which is not the same thing, and is not claimed to
// be. See docs/verification/update-path.md for what is still owed.

console.log("\nThe update path");

/** Every generated `installer.nsi` a build has left under target/. */
const generatedNsis = [];
const findNsis = (dir, depth = 0) => {
  if (depth > 6) return;
  let entries;
  try {
    entries = readdirSync(dir, { withFileTypes: true });
  } catch {
    return;
  }
  for (const entry of entries) {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) findNsis(path, depth + 1);
    else if (entry.name === "installer.nsi") generatedNsis.push(path);
  }
};
findNsis(target);

if (generatedNsis.length === 0) {
  console.log("  --  no generated installer.nsi under target/; run `npm run tauri build` first");
} else {
  for (const path of generatedNsis) {
    const nsi = readFileSync(path, "utf8");
    const where = path.replace(root, "").replace(/^[\\/]/, "");

    // The reinstall page is what runs the previous version's uninstaller. In
    // update mode the template is supposed to jump straight over it, and this
    // is the line that does it — the one whose absence, from the *caller's*
    // side, cost every user of every version through 1.20.0 their data.
    const shortCircuit = nsi.indexOf("$UpdateMode = 1");
    const runsUninstaller = nsi.indexOf("ExecWait '$R1'");

    if (shortCircuit === -1) {
      fail(`${where} has no $UpdateMode short-circuit; an update would take the reinstall path`);
    } else if (runsUninstaller !== -1 && shortCircuit > runsUninstaller) {
      fail(`${where} runs the old uninstaller before it checks $UpdateMode`);
    } else {
      console.log(`  ok  ${where} skips the reinstall page in update mode`);
    }

    // The hooks are inlined into the generated script, so their guards are
    // checkable here in the form they actually ship rather than as source.
    const guards = (nsi.match(/\$UpdateMode = 1/g) ?? []).length;
    if (guards < 3) {
      fail(
        `${where} carries ${guards} $UpdateMode guards; the template's own plus both uninstall hooks is at least 3`,
      );
    }
  }
}

// The flags, in the binary that will be asked to pass them.
//
// Only `/UPDATE` is searched for. It is the flag that removes the data loss and
// it is distinctive enough that finding it means something. `/P`, `/R` and `/NS`
// are two and three characters long and occur by chance in any megabyte of
// compiled code, so a search for them would pass on every build including a
// broken one — which is the same worthless guard as searching a compressed
// installer for a channel marker. The unit test in updates.rs asserts the whole
// list reaches the spawned command; this asserts the strings survived the
// release profile's `strip` and LTO.
const UPDATE_FLAG = "/UPDATE";
let flagChecked = 0;
for (const dir of releaseDirs) {
  const exe = join(dir, SHIPPED);
  let stat;
  try {
    stat = statSync(exe);
  } catch {
    continue;
  }
  if (stat.mtimeMs < markerAge) continue; // same staleness rule as above
  if (readFileSync(exe, "latin1").includes(UPDATE_FLAG)) {
    flagChecked += 1;
    console.log(`  ok  ${UPDATE_FLAG} is in ${dir}`);
  } else {
    fail(`${exe} does not contain ${UPDATE_FLAG} — an update from it would run the uninstaller`);
  }
}
if (flagChecked === 0) {
  console.log("  --  no current release binary to search for the installer flags");
}

// Exactly one kind of installer, and it is NSIS.
const bundleDirs = releaseDirs.map((dir) => join(dir, "bundle"));
let sawBundle = false;
for (const bundle of bundleDirs) {
  let kinds;
  try {
    kinds = readdirSync(bundle);
  } catch {
    continue;
  }
  sawBundle = true;
  const where = bundle.replace(root, "").replace(/^[\\/]/, "");
  const extra = kinds.filter((kind) => kind !== "nsis");
  if (extra.length > 0) {
    // An MSI is the one that matters — it installs to a different directory
    // under its own uninstall entry, so a machine ends up with two copies of
    // Cursed and neither uninstaller knows about the other.
    fail(`${where} contains ${extra.join(", ")} beside nsis; only NSIS ships`);
  } else {
    console.log(`  ok  ${where} builds nsis alone`);
  }
}
if (!sawBundle) console.log("  --  nothing bundled yet");

// And nothing staged for release is an MSI, whatever produced it.
try {
  for (const name of readdirSync(join(root, "dist-release"))) {
    if (name.toLowerCase().endsWith(".msi")) fail(`dist-release/${name} is an MSI`);
  }
} catch {
  // Nothing staged.
}

// ── nothing in the source tree is mojibake ───────────────────────
//
// A comment in Cargo.toml lost its em-dash and its section sign somewhere
// before v1.19.0 and carried runs of question marks in their place until they
// were found by eye. That is what a UTF-8 file looks like after a round trip
// through a console codepage that cannot represent what it holds — twice, in
// that case, which is why the em-dash came back as eight characters and the
// section sign as four.
//
// It is invisible in review, survives every compiler and linter, and the only
// thing that ever notices is a person reading the line. So a run of three or
// more question marks anywhere in the tracked source is treated as damage, as
// are the classic UTF-8-read-as-CP1252 sequences.
//
// The patterns are written as escapes rather than as themselves so that this
// file is not the first thing its own scan reports.
console.log("\nNo mojibake in the source");
const MOJIBAKE = new RegExp(
  [
    "[?]{3,}", //                               a punctuation mark, twice destroyed
    "\u00e2\u20ac", //                        the head of nearly every CP1252 corruption
    "\u00c2[\u00a7\u00b0\u00a9\u00ae]", // a Latin-1 punctuation mark, doubled
    "\u00c3[\u0192\u201a\u00a9]", //        a Latin-1 letter, doubled
    "\u00ef\u00bb\u00bf", //                 a BOM read as three characters
  ].join("|"),
);
const TEXT_EXTENSIONS = [".rs", ".ts", ".tsx", ".toml", ".json", ".md", ".mjs", ".nsh", ".yml", ".css"];
const SKIP_DIRS = new Set(["node_modules", "target", "dist", "dist-release", ".git", "assets"]);

let scanned = 0;
const scanText = (dir) => {
  let entries;
  try {
    entries = readdirSync(dir, { withFileTypes: true });
  } catch {
    return;
  }
  for (const entry of entries) {
    if (entry.isDirectory()) {
      if (!SKIP_DIRS.has(entry.name)) scanText(join(dir, entry.name));
      continue;
    }
    if (!TEXT_EXTENSIONS.some((ext) => entry.name.endsWith(ext))) continue;
    const path = join(dir, entry.name);
    const text = readFileSync(path, "utf8");
    scanned += 1;
    for (const [index, line] of text.split("\n").entries()) {
      if (MOJIBAKE.test(line)) {
        fail(`${path.replace(root, "").replace(/^[\\/]/, "")}:${index + 1} looks like mojibake: ${line.trim()}`);
      }
    }
  }
};
scanText(root);
console.log(`  ok  ${scanned} source files scanned`);

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
