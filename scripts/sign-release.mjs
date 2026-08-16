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
import { readdirSync, readFileSync, writeFileSync, unlinkSync, existsSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const staging = join(root, "dist-release");

const privateKey = process.env.TAURI_SIGNING_PRIVATE_KEY?.trim();
if (!privateKey) {
  console.error(
    "TAURI_SIGNING_PRIVATE_KEY is not set. See docs/SIGNING.md — this script\n" +
      "does not generate a key, and must not.",
  );
  process.exit(1);
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
    execFileSync(
      process.platform === "win32" ? "npx.cmd" : "npx",
      ["tauri", "signer", "sign", "--private-key", privateKey, file],
      {
        stdio: ["ignore", "pipe", "pipe"],
        env: {
          ...process.env,
          // Passed through rather than on the command line: a password in argv
          // is readable by every other process on the machine.
          TAURI_SIGNING_PRIVATE_KEY_PASSWORD:
            process.env.TAURI_SIGNING_PRIVATE_KEY_PASSWORD ?? "",
        },
      },
    );
  } catch (e) {
    // Never echo the error verbatim: the private key was passed as an argument
    // and some CLIs put their whole argv in a failure message.
    console.error(`  FAIL  ${name} could not be signed (exit ${e.status ?? "?"})`);
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
