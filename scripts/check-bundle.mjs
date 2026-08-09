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
