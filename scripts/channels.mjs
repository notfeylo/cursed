/**
 * What the two channels are doing on this machine right now.
 *
 *   npm run channels
 *
 * Cursed is developed as two installs: the build being iterated on, and an exact
 * copy of what a stranger downloads, side by side. Almost everything they touch
 * is named per channel and cannot collide — but the pointer scheme is one set of
 * seventeen registry values per Windows user, and only one channel may defend
 * it. When the cursor is not what you expect, the first question is which of the
 * two is holding it, and nothing on screen answers that.
 *
 * Read-only. This reports; it never claims, releases or repairs anything.
 */
import { execFileSync } from "node:child_process";
import { existsSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";

const CHANNELS = [
  { name: "user", product: "Cursed", data: "Cursed", exe: "Cursed.exe" },
  { name: "dev", product: "Cursed Dev", data: "Cursed (Dev)", exe: "Cursed Dev.exe" },
];

/** `reg`/`tasklist` failing is an answer ("not there"), not an error. */
const quiet = (file, args) => {
  try {
    return execFileSync(file, args, { encoding: "utf8", stdio: ["ignore", "pipe", "ignore"] });
  } catch {
    return "";
  }
};

/** `    ValueName    REG_SZ    the value` -> the value. */
function value(text, name) {
  const line = text.split(/\r?\n/).find((l) => l.trim().startsWith(name + "  "));
  if (!line) return null;
  const parts = line.trim().split(/\s{2,}/);
  return parts.length >= 3 ? parts.slice(2).join("  ") : null;
}

// ── installed? ───────────────────────────────────────────────────
//
// Matched on DisplayName rather than on the identifier: the uninstall key's
// name is the installer's business and has changed shape between Tauri
// versions, but the display name is ours and is asserted by channel.rs.
const uninstall = quiet("reg", [
  "query",
  "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall",
  "/s",
]);
const installed = new Map();
{
  let key = null;
  for (const line of uninstall.split(/\r?\n/)) {
    if (line.startsWith("HKEY_")) key = { path: line.trim() };
    else if (key && line.trim().startsWith("DisplayName")) {
      key.displayName = line.trim().split(/\s{2,}/).slice(2).join("  ");
    } else if (key && line.trim().startsWith("DisplayVersion")) {
      key.version = line.trim().split(/\s{2,}/).slice(2).join("  ");
      installed.set(key.displayName, key);
    }
  }
}

const appdata = process.env.APPDATA ?? "";
const running = (exe) => quiet("tasklist", ["/fi", `imagename eq ${exe}`, "/nh"]).includes(exe);

/** Bytes under a directory, so an empty data folder is visibly empty. */
function size(dir) {
  let total = 0;
  const walk = (d) => {
    for (const entry of readdirSync(d, { withFileTypes: true })) {
      const full = join(d, entry.name);
      if (entry.isDirectory()) walk(full);
      else
        try {
          total += statSync(full).size;
        } catch {
          /* vanished mid-walk */
        }
    }
  };
  try {
    walk(dir);
  } catch {
    return null;
  }
  return total;
}

console.log("Channels\n");
for (const channel of CHANNELS) {
  const entry = installed.get(channel.product);
  const data = appdata ? join(appdata, channel.data) : null;
  const bytes = data && existsSync(data) ? size(data) : null;

  console.log(`  ${channel.product}  (${channel.name} channel)`);
  console.log(`    installed  ${entry ? `yes, ${entry.version ?? "unknown version"}` : "no"}`);
  console.log(`    running    ${running(channel.exe) ? "yes" : "no"}`);
  console.log(
    `    data       ${
      bytes === null ? "none" : `${data}  (${(bytes / 1_048_576).toFixed(1)} MB)`
    }`,
  );
  console.log("");
}

// ── the one thing they share ─────────────────────────────────────
const shared = quiet("reg", ["query", "HKCU\\Software\\Cursed"]);
const owner = value(shared, "PointerOwnerChannel");
const ownerPid = value(shared, "PointerOwnerPid");
const since = value(shared, "PointerOwnerSince");
const snapshotOwner = value(shared, "OriginalSchemeChannel");
const snapshotPath = value(shared, "OriginalSchemePath");

console.log("The pointer\n");
console.log(
  owner
    ? `  last claimed by the ${owner} channel (pid ${
        ownerPid ? parseInt(ownerPid, 16) || ownerPid : "?"
      })${since ? ` at ${since}` : ""}`
    : "  never claimed — neither channel has run on this account",
);
// Advisory, and worth saying so: the record is written on claim and not cleared
// on exit, so it names the last holder, which may be a process that has since
// quit. The lock itself is a named mutex and is always accurate; this is not.
console.log("  (the record is advisory — a channel that has quit still appears here)");

console.log("\nThe original Windows scheme\n");
if (!snapshotOwner) {
  console.log("  not captured yet");
} else {
  const readable = snapshotPath && existsSync(snapshotPath);
  console.log(`  captured by the ${snapshotOwner} channel`);
  console.log(`  ${snapshotPath ?? "(no path recorded)"}${readable ? "" : "   MISSING"}`);
  if (!readable) {
    // Not fatal: each channel copies the snapshot into its own data directory
    // at first run, so the other channel's copy still restores correctly. It
    // does mean a channel installed from here on has nothing to adopt.
    console.log("  the file is gone; a newly installed channel would capture afresh");
  }
}
console.log("");
