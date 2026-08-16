/**
 * Signs every staged installer with the release key.
 *
 *     node scripts/sign-release.mjs           (after `npm run release`)
 *
 * Run by `.github/workflows/release.yml`, which refuses to build at all without
 * the secrets this reads. It can be run by hand too, on a machine that has the
 * private key — see `docs/SIGNING.md`.
 *
 * ## What gets signed, and what does not
 *
 * Only the versioned installers, `Cursed_<version>_<arch>-setup.exe`. Those are
 * the exact names `is_our_installer` in `src-tauri/src/updates.rs` will accept,
 * which makes them the only files an installed copy of the app can ever be
 * persuaded to download and run. The unversioned aliases and the offline build
 * are for humans clicking a link on the website; nothing verifies a signature on
 * those because nothing executes them unattended.
 *
 * ## Why the output is renamed
 *
 * Tauri's signer writes `<file>.sig`, and what it puts inside is the minisign
 * signature file base64-encoded a second time — its own updater base64-decodes
 * before verifying. This project does not use that updater, so the extra layer
 * is stripped here and the result is written as `<file>.minisig`: the plain
 * minisign format, exactly what `minisign -Sm` would have produced, and exactly
 * what `minisign_verify::Signature::decode` reads.
 *
 * The unwrapping is detected rather than assumed. If a future CLI version writes
 * the minisign text directly, the check below sees that it already starts with
 * `untrusted comment:` and passes it through unchanged.
 */
import { execFileSync } from "node:child_process";
import {
  existsSync,
  mkdtempSync,
  readdirSync,
  readFileSync,
  rmSync,
  unlinkSync,
  writeFileSync,
} from "node:fs";
import { createRequire } from "node:module";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const staging = join(root, "dist-release");

/**
 * The Tauri CLI's own entry point, run through this Node rather than through
 * `npx`.
 *
 * **Not `npx`, and not `npx.cmd`.** Node 20 and later refuse to spawn a `.cmd`
 * or `.bat` through `child_process` without `shell: true` — the fix for
 * CVE-2024-27980, where a batch file's arguments could be re-parsed by cmd.exe.
 * `execFileSync("npx.cmd", …)` therefore throws `EINVAL` before anything runs,
 * and because nothing ran there is no exit status: the failure arrives as
 * `undefined`, which is how the first attempt at this reported "exit ?" for
 * three files in 150 milliseconds.
 *
 * Turning on `shell: true` would fix the spawn and reintroduce exactly the
 * argument-parsing problem that restriction exists to prevent, on a command
 * line that used to carry a private key. Resolving the CLI's JavaScript and
 * handing it to `process.execPath` avoids the shell altogether.
 */
const tauriCli = createRequire(join(root, "package.json")).resolve(
  "@tauri-apps/cli/tauri.js",
);

/**
 * Signs one file in place, leaving `<file>.sig` beside it.
 *
 * The key and its password go through the environment, never argv. The CLI
 * reads both from there by documented default; argv is readable by every other
 * process on the machine and is what ends up quoted in a crash report. Keeping
 * the key out of it is also what lets `describe` print an error in full, which
 * is the difference between diagnosing a failure in one build and in three.
 */
function sign(file) {
  execFileSync(process.execPath, [tauriCli, "signer", "sign", file], {
    stdio: ["ignore", "pipe", "pipe"],
    env: {
      ...process.env,
      TAURI_SIGNING_PRIVATE_KEY: privateKey,
      TAURI_SIGNING_PRIVATE_KEY_PASSWORD:
        process.env.TAURI_SIGNING_PRIVATE_KEY_PASSWORD ?? "",
    },
  });
}

/** Why a spawn failed, including whether it started at all. */
function describe(e) {
  return [
    e.status === undefined ? `did not start (${e.code ?? e.message})` : `exit ${e.status}`,
    e.stderr?.toString().trim(),
    e.stdout?.toString().trim(),
  ]
    .filter(Boolean)
    .join(" — ");
}

const privateKey = process.env.TAURI_SIGNING_PRIVATE_KEY?.trim();
if (!privateKey) {
  console.error(
    "TAURI_SIGNING_PRIVATE_KEY is not set. See docs/SIGNING.md — this script\n" +
      "does not generate a key, and must not.",
  );
  process.exit(1);
}

/**
 * `--selftest` proves the key and password work, without needing a build.
 *
 * Signing is the last step of a twenty-five minute release build, so every
 * mistake in a secret costs a full build to discover and another to confirm the
 * fix. This signs eight bytes in a temporary directory and exits, which turns
 * "wrong password" from a twenty-six minute failure into a thirty second one.
 *
 * Run early in the release workflow, immediately after the secrets are checked
 * for existence — because existing and being correct are different things, and
 * only one of them was being checked.
 */
if (process.argv.includes("--selftest")) {
  const scratch = mkdtempSync(join(tmpdir(), "cursed-signtest-"));
  const probe = join(scratch, "Cursed_0.0.0_x64-setup.exe");
  writeFileSync(probe, "selftest");
  try {
    sign(probe);
    console.log("The signing key and password work.");
    process.exit(0);
  } catch (e) {
    console.error(`::error::the signing key cannot sign: ${describe(e)}`);
    console.error("Check TAURI_SIGNING_PRIVATE_KEY and TAURI_SIGNING_PRIVATE_KEY_PASSWORD.");
    console.error("See docs/SIGNING.md. A key generated with a password needs that");
    console.error("password in the secret; an empty secret only works for a key that");
    console.error("was generated without one.");
    process.exit(1);
  } finally {
    rmSync(scratch, { recursive: true, force: true });
  }
}

if (!existsSync(staging)) {
  console.error("dist-release/ does not exist; run `npm run release` first.");
  process.exit(1);
}

/** The names the updater is capable of downloading, and no others. */
const SIGNABLE = /^Cursed_\d+\.\d+\.\d+_(x64|arm64|x86)-setup\.exe$/;

const targets = readdirSync(staging).filter((name) => SIGNABLE.test(name));
if (targets.length === 0) {
  console.error("dist-release/ holds no versioned installers to sign.");
  process.exit(1);
}

let failures = 0;

for (const name of targets) {
  const file = join(staging, name);
  const produced = `${file}.sig`;
  const wanted = `${file}.minisig`;

  try {
    sign(file);
  } catch (e) {
    console.error(`  FAIL  ${name}: ${describe(e)}`);
    failures += 1;
    continue;
  }

  if (!existsSync(produced)) {
    console.error(`  FAIL  ${name} produced no signature`);
    failures += 1;
    continue;
  }

  const raw = readFileSync(produced, "utf8").trim();
  let minisig;
  if (raw.startsWith("untrusted comment:")) {
    minisig = raw;
  } else {
    minisig = Buffer.from(raw, "base64").toString("utf8");
  }

  if (!minisig.startsWith("untrusted comment:")) {
    console.error(`  FAIL  ${name}'s signature is not in minisign format after unwrapping`);
    failures += 1;
    continue;
  }

  writeFileSync(wanted, `${minisig.trimEnd()}\n`, "utf8");
  unlinkSync(produced);
  console.log(`  ok    ${name}.minisig`);
}

console.log(
  failures === 0
    ? `\nSigned ${targets.length} installer(s).`
    : `\n${failures} of ${targets.length} could not be signed.`,
);
process.exit(failures === 0 ? 0 : 1);
