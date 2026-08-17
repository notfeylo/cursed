# Signing

Two different signatures, for two different problems. They are often confused
and only one of them is free.

| | Update signature | Code signature |
| --- | --- | --- |
| Answers | "did the author publish this installer?" | "does Windows trust this publisher?" |
| Checked by | Cursed itself, before running a downloaded update | SmartScreen, on first run |
| Format | minisign (Ed25519) | Authenticode |
| Cost | nothing | ~$10/month, Azure Trusted Signing |
| Status | **wired, awaiting a key** | not started |

The update signature is the important one and it is the free one, which is why
it comes first. A code signature stops a scary dialog; an update signature stops
a compromised release path from executing arbitrary code on every machine that
has ever installed this app.

---

## Why the checksum is not enough

`SHA256SUMS.txt` is fetched from the same host as the installer it describes.
Anyone able to replace one is able to replace the other, and the app would
cheerfully verify the substitution against itself and run it. TLS proves who
served the bytes; it says nothing about whether those bytes are the ones that
were published.

The private key never leaves the author's machine, so a release path that is
entirely compromised — a stolen GitHub token, a hijacked account — still cannot
produce a file an installed copy of Cursed will execute.

## How it fits together

1. `scripts/sign-release.mjs` signs each versioned installer after `npm run
   release` stages it, writing `Cursed_<version>_<arch>-setup.exe.minisig`
   beside it. Only the versioned names are signed: those are the only names
   `is_our_installer` accepts, so they are the only files the app can ever be
   persuaded to download and run.
2. The release workflow uploads the `.minisig` files with everything else, and
   fails if one is missing.
3. The **public** key is compiled into the binary via `CURSED_UPDATE_PUBLIC_KEY`
   at build time. A public key inside a public binary is what a public key is
   for.
4. `updates::verified_installer` fetches `<asset>.minisig` from the release and
   verifies the download against the compiled-in key **before** the app tears
   itself down. A failure deletes the file, exactly as a checksum failure does.

Verification is `minisign-verify`, not hand-rolled Ed25519 — the same reasoning
as `hash.rs` calling Windows' CNG rather than carrying a SHA-2 implementation.

## The one thing that is not done

**The key does not exist yet.** Nothing in this repository generates it, and
nothing should: a signing key produced by a script somebody ran once is a
signing key nobody knows the provenance of.

### Generate it

On the machine that will hold it — not in CI, not in a container, not on a
shared box:

```bash
npx tauri signer generate -w ~/.cursed/cursed.key
```

It asks for a password. Use one, and put it in a password manager before
pressing Enter. A key with no password is a key that is compromised the moment
the machine it lives on is.

That writes two files:

| File | What it is | Where it goes |
| --- | --- | --- |
| `~/.cursed/cursed.key` | the private key | **stays on that machine**, backed up offline |
| `~/.cursed/cursed.key.pub` | the public key | into the repository's secrets, and into every build |

**Paste `cursed.key.pub` exactly as it is.** What Tauri writes there is not the
key line you may have seen in minisign documentation — it is the whole
`minisign.pub` file, base64-encoded onto one line, and it looks like meaningless
base64 rather than like a key.

`signing::parse_public_key` accepts all three shapes this comes in: that blob,
the two-line file it decodes to, and the bare `RW…` key line on its own. Being
liberal here costs nothing — whatever shape arrives either decodes to the same
32 bytes or does not decode at all — and it removes the failure where the app
builds fine, the release signs fine, and **every update is refused as tampered**
because the key was pasted in the shape the documentation told you to use.

### Put the halves where they belong

Three repository secrets, at
`https://github.com/notfeylo/cursed/settings/secrets/actions`:

| Secret | Value |
| --- | --- |
| `TAURI_SIGNING_PRIVATE_KEY` | the entire contents of `cursed.key` |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | the password chosen above |
| `CURSED_UPDATE_PUBLIC_KEY` | the single line in `cursed.key.pub` |

**The password secret must match the key.** An empty value only works for a key
generated *without* a password. Give a password-protected key an empty secret
and the signer fails with:

```
incorrect updater private key password: Wrong password for that key
```

Which is what happened on the first attempt at v1.21.0, and cost a
twenty-five-minute build to find out, because signing is the last step. It no
longer does: `node scripts/sign-release.mjs --selftest` signs eight bytes and
exits, and the release workflow runs it immediately after checking the secrets
exist — existing and being correct being different things.

Run it yourself before tagging, on a machine with the key:

```bash
TAURI_SIGNING_PRIVATE_KEY="$(cat ~/.cursed/cursed.key)" \
TAURI_SIGNING_PRIVATE_KEY_PASSWORD="…" \
CURSED_UPDATE_PUBLIC_KEY="$(cat ~/.cursed/cursed.key.pub)" \
  node scripts/sign-release.mjs --selftest
```

It also checks the two halves are **a pair**, by verifying a signature it just
made against the public key you are about to compile in. That is the one failure
that does not fail a build: regenerating a key changes both halves, and updating
only the private one produces a release that signs perfectly, publishes
perfectly, and is then **refused by every installed copy for ever** — with no
error anywhere except on other people's machines.

If the password is lost, the key is not recoverable — generate a new pair and
update both `TAURI_SIGNING_PRIVATE_KEY` and `CURSED_UPDATE_PUBLIC_KEY`. That is
free while no release has shipped signed with the old one, and expensive
afterwards: see **Rotation**.

The public one is a secret only in the sense that GitHub Actions calls every
variable a secret. It is not sensitive and is published inside every binary.

That is the whole procedure. The release workflow refuses to build without all
three, which is the right place to enforce it — once, at the point of
publishing, rather than on every user's machine afterwards.

### Until then

A build made without `CURSED_UPDATE_PUBLIC_KEY` has nothing to verify against,
so it falls back to checksum-only and says so in the log on every update. That
fallback is deliberate. The alternative is a build that refuses every update
because it has nothing to check with, which would turn one missing build-time
variable into a fleet that can never be updated again — including out of the
state that caused it.

`crate::signing::describe()` is the sentence the app uses for whichever
guarantee is actually in force. It is never allowed to claim the stronger one.

## Rotation

A key is rotated because it leaked, or because it might have. Both are urgent
and the procedure is the same.

The constraint that makes this awkward: **an installed copy verifies with the
key it was built with.** Publishing a release signed with a new key means every
existing install refuses it — correctly, because from their side it is exactly
what an attack looks like.

So rotation is two releases:

1. **The bridge.** Build a release with the *old* key still in
   `CURSED_UPDATE_PUBLIC_KEY` and sign it with the *old* private key, so
   existing installs accept it. Nothing else about it needs to change.
2. **The turn.** Once enough of the fleet is on the bridge release, set
   `CURSED_UPDATE_PUBLIC_KEY` to the new public key and
   `TAURI_SIGNING_PRIVATE_KEY` to the new private key, and publish. Installs on
   the bridge accept it because the bridge was built with the old key and this
   release is signed with — no. **The bridge must carry the new public key and
   be signed with the old private key.** That is the whole trick: a build
   verifies the *next* update with the key compiled into it, and is itself
   verified with the key its predecessor carried.

Written out, because it is easy to get backwards:

| Release | Compiled-in public key | Signed with |
| --- | --- | --- |
| current | old | old |
| bridge | **new** | **old** |
| next | new | **new** |

Anyone who skips the bridge is stuck and has to reinstall by hand from the
website. That is the cost of a rotation and the reason to keep the key
somewhere it will not need one.

## Code signing, later

Azure Trusted Signing is about $10/month and is what clears SmartScreen. The
recommendation is to do it **after** the update path has shipped and been
verified on a VM, and once downloads justify the cost — a warning dialog is an
annoyance, and an unverifiable update is a vulnerability. Fix the second one
first.

It does not replace anything above. Authenticode is checked by Windows when a
user runs an installer; the minisign signature is checked by Cursed before it
ever gets that far.
