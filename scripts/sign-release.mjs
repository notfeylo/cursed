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
import { createHash, createPublicKey, verify as verifySignature } from "node:crypto";
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
      TAURI_SIGNING_PRIVATE_KEY_PASSWORD: password(),
    },
  });
}

/**
 * The key's password, with whitespace-only treated as none.
 *
 * **A single space is not an empty password**, and that distinction cost this
 * release two twenty-five minute builds. The secret held one space; the key had
 * no password; the signer reported "Wrong password for that key", which is
 * accurate and reads like the key is wrong rather than the secret.
 *
 * Nobody has ever meant a password of pure whitespace. A real one is left
 * exactly as it is — including any spaces inside it — because trimming a
 * genuine password is a worse failure than this one, and silent.
 */
function password() {
  const raw = process.env.TAURI_SIGNING_PRIVATE_KEY_PASSWORD ?? "";
  if (raw.length > 0 && raw.trim().length === 0) {
    console.warn(
      "  note  TAURI_SIGNING_PRIVATE_KEY_PASSWORD is whitespace only; treating it as\n" +
        "        no password. Set it to a real password, or delete the secret.",
    );
    return "";
  }
  return raw;
}


/**
 * Checks that `CURSED_UPDATE_PUBLIC_KEY` is the other half of the key that just
 * signed something.
 *
 * **This is the failure that does not fail the build.** Regenerating a key pair
 * changes both halves. Update the private one and forget the public one and the
 * release signs perfectly, publishes perfectly, and then every installed copy
 * refuses every update for ever — because the key compiled into them verifies
 * nothing the new private key produces. There is no error at build time, no
 * error at publish time, and the only symptom is on other people's machines.
 *
 * So the pair is checked here, against a signature actually produced moments
 * ago, using Node's own Ed25519 rather than a dependency.
 *
 * Returns a reason to fail, or `null` when the pair matches.
 */
function publicKeyMismatch(signedFile, signaturePath) {
  const publicKeyText = process.env.CURSED_UPDATE_PUBLIC_KEY?.trim();
  if (!publicKeyText) {
    return "CURSED_UPDATE_PUBLIC_KEY is not set";
  }

  // The same three shapes `signing::parse_public_key` accepts: the two-line
  // minisign.pub file, the bare key line, and Tauri's base64 of the whole file.
  let text = publicKeyText;
  if (!text.startsWith("untrusted comment:") && !text.startsWith("RW")) {
    text = Buffer.from(text, "base64").toString("utf8").trim();
  }
  const keyLine = text.startsWith("untrusted comment:") ? text.split("\n")[1] : text;
  if (!keyLine) return "CURSED_UPDATE_PUBLIC_KEY is not a minisign public key";

  const key = Buffer.from(keyLine.trim(), "base64");
  // 2 bytes algorithm, 8 bytes key id, 32 bytes of Ed25519 public key.
  if (key.length !== 42) return "CURSED_UPDATE_PUBLIC_KEY is not a minisign public key";

  const lines = readMinisig(signaturePath).split("\n");
  const sigBlob = Buffer.from(lines[1] ?? "", "base64");
  if (sigBlob.length !== 74) return "the signature just produced is malformed";

  // The key id is the cheap half of the check and gives the clearest message:
  // a mismatch here is unambiguously the wrong key rather than corrupt bytes.
  const keyId = key.subarray(2, 10);
  const sigKeyId = sigBlob.subarray(2, 10);
  if (!keyId.equals(sigKeyId)) {
    return `the public key is for a different key pair (public key id ${keyId.toString("hex")}, signature key id ${sigKeyId.toString("hex")})`;
  }

  // And the signature itself, so a doctored key id cannot pass.
  //
  // Algorithm "ED" means the signature is over BLAKE2b-512 of the file rather
  // than the file, which is minisign's prehashed mode and Tauri's default.
  const algorithm = sigBlob.subarray(0, 2).toString("latin1");
  const contents = readFileSync(signedFile);
  const message =
    algorithm === "ED" ? createHash("blake2b512").update(contents).digest() : contents;

  // Ed25519 public keys reach Node as SPKI DER; the prefix is fixed.
  const spki = Buffer.concat([
    Buffer.from("302a300506032b6570032100", "hex"),
    key.subarray(10),
  ]);
  const ok = verifySignature(
    null,
    message,
    createPublicKey({ key: spki, format: "der", type: "spki" }),
    sigBlob.subarray(10),
  );
  return ok ? null : "the public key does not verify a signature made by the private key";
}

/**
 * Reads what Tauri wrote and returns plain minisign text.
 *
 * The CLI writes `<file>.sig` holding the minisign signature file base64'd a
 * second time, because its own updater decodes that layer before verifying.
 * Both callers here need the unwrapped form, and having only one of them do it
 * is how the pair check below reported every signature as malformed, including
 * the correct ones.
 */
function readMinisig(sigPath) {
  const raw = readFileSync(sigPath, "utf8").trim();
  return raw.startsWith("untrusted comment:")
    ? raw
    : Buffer.from(raw, "base64").toString("utf8");
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
    const mismatch = publicKeyMismatch(probe, `${probe}.sig`);
    if (mismatch) {
      console.error(`::error::${mismatch}`);
      console.error("Regenerating a key changes BOTH halves. Update");
      console.error("CURSED_UPDATE_PUBLIC_KEY as well as TAURI_SIGNING_PRIVATE_KEY,");
      console.error("or the release will sign and then every update will be refused.");
      process.exit(1);
    }
    console.log("The signing key and password work, and the public key matches it.");
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

  const minisig = readMinisig(produced);
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
