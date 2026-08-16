# Two installs of one app

Cursed is developed as two channels installed side by side on the same PC: the
**user channel**, which is an exact copy of what a stranger downloads, and the
**dev channel**, which is the build being iterated on.

The reason is narrow and worth stating plainly. Almost every bug this product
has had was a bug in what happens to a *real install* over time — a scheme that
does not survive a theme change, a snapshot captured a launch too late, an
uninstall that leaves a dropdown full of dead entries. None of that is
observable from `npm run tauri dev`, which starts fresh, writes to a debug data
directory, and never installs anything. The only way to watch the released app
behave over days is to keep one installed, and the only way to keep working
while it is installed is for the working copy to be a second, separate app.

Two copies of one app collide unless every shared name is separated. What
follows is the separation, and the one thing that cannot be separated.

## What each channel is called

Everything in this table is derived from `src-tauri/src/channel.rs` and written
as a literal nowhere else. The values are `#[cfg]`-gated constants resolved at
compile time, not a runtime flag: an environment variable inherited from a
parent process or a stray argument must never be able to talk a shipped binary
into behaving like a dev build.

| | User channel | Dev channel |
| --- | --- | --- |
| Product name | `Cursed` | `Cursed Dev` |
| Bundle identifier | `dev.feylo.cursed` | `dev.feylo.cursed.dev` |
| Data directory | `%APPDATA%\Cursed` | `%APPDATA%\Cursed (Dev)` |
| Pointer-scheme prefix | `Cursed — ` | `Cursed Dev — ` |
| Inherits the `CursorForge` folder | yes | **no** |
| App icon | blue | amber |
| Claims global hotkeys | yes | no |
| Defends the pointer scheme | yes | no |
| Binary marker | `CURSED-CHANNEL:USER` | `CURSED-CHANNEL:DEV` |

Three of those rows are not cosmetic and are worth their own paragraph.

**The dev channel inherits nothing.** The migration in `paths::root` fires
whenever the current data directory is absent, which is true of every dev
channel on its first run. Without the gate, the first `npm run dev:channel` on a
machine still holding a `CursorForge` folder would move the real user's
settings, presets and imported artwork into the dev channel and leave the user
channel to start from nothing. The same rule governs the old scheme prefix: an
uninstall cleans up by prefix, so a dev uninstall that inherited the old prefix
would strip the user channel's older entries out of the Windows Pointers
dropdown.

**Global shortcuts belong to one process per session, first come first served.**
With both channels installed, whichever launched first would take `Ctrl+Alt+1`
and the other would get nothing — no error, no log line, just a hotkey that does
not work. The dev channel does not ask.

**The icons differ by hue, not by a badge.** The tray draws the mark at 16×16,
where a badge is a smudge and a colour is still a colour. Both are rendered from
the same SVG through the same rasteriser (`packs::brand`), so there is no second
mark to maintain.

## The one thing they share

`HKCU\Control Panel\Cursors` is a single set of seventeen values per Windows
user, and both channels write to it, because writing to it is what applying a
cursor *is*. Two consequences follow, and both are silent failures rather than
errors.

**The watchdog fights itself.** `cursor::watchdog` notices when the scheme stops
matching what this build applied and puts it back. Two builds with different
schemes applied each read the other's write as drift, so they revert each other
every few seconds, indefinitely, and the pointer flickers between two cursors
with nothing in either log that looks like a cause.

**The safety snapshot gets overwritten with a lie.** `restore::capture_once` is
idempotent, but only against its own data directory. A second channel starting
up has an empty one, so it captures — and what it captures is whatever the
*first* channel has applied. That channel's "restore the original Windows
pointers" then restores the other channel's cursor, for ever, and nothing about
it looks broken until someone tries it.

### Ownership is a named mutex

`cursor::crosschannel` arbitrates with `Local\dev.feylo.cursed.pointer` — the one
name both channels must agree on, deliberately not channel-scoped. `Local\`
rather than `Global\` keeps it per-session, so a second signed-in Windows user
still gets their own.

A mutex, rather than a lock file, because a mutex is the one lock Windows cleans
up on our behalf. A lock file has to record a pid and a timestamp and then be
second-guessed — is that process alive, is that timestamp stale, was the machine
reset — and every one of those questions is a way to deadlock a developer's own
machine. A mutex abandoned by a dying process is reported as `WAIT_ABANDONED` to
the next waiter, which simply takes it. Killing the holder is not a special case;
it is the normal path.

Only the user channel asks for it, and it asks on every watchdog tick as well as
at startup. That is what makes handover work: quit the released copy and the dev
build does *not* take over — it never asks — but quit and relaunch either one and
ownership settles without restarting anything else.

Applying a cursor because someone clicked one is always allowed on both
channels. That is a direct instruction from the person at the keyboard, and both
write the same seventeen values. The lock governs the *unprompted* writes: the
watchdog's reverts, which are the ones that fight.

### First capture wins, permanently

The channel that captured the machine's true original scheme records the fact in
`HKCU\Software\Cursed`, and no later channel may claim it. Only the first
channel to run on a machine ever sees the true pre-Cursed pointers; every later
claim would be a claim about a scheme Cursed itself applied.

A second channel starting up reads that record and copies the file's contents
verbatim into its own data directory. **Copying rather than sharing is
deliberate.** A single shared file would be deleted by whichever channel is
uninstalled first, taking the other channel's only record of the machine's real
pointers with it. Two identical copies survive either uninstall, and the
contents never change after capture, so they cannot drift apart.

Uninstalling removes only that channel's half of the shared record, and the key
itself only when nothing of the other channel's is left in it.

## Working with them

```bash
npm run dev:channel        # run the dev channel against the vite dev server
npm run build:dev          # build and verify the dev channel's installer
npm run channels           # what both channels are doing on this machine
```

`npm run channels` is the answer to "why is the cursor not what I just applied":
it prints which channels are installed, which are running, how large each data
directory is, which channel last claimed the pointer, and which one captured the
original scheme.

`npm run build:dev` builds and then asserts that what came out is what was asked
for. Two things have to be true at once and neither is checked by the build
itself: the cargo feature `dev-channel` must be on, and
`src-tauri/dev.tauri.conf.json` must be merged over the normal config. The
feature without the config produces an app that writes to `Cursed (Dev)` while
installing *over the released copy* — replacing the thing being tested with the
thing being tested against. The config without the feature produces the reverse:
a separate install that shares the released app's data directory and fights it
for the pointer.

If the icons have never been generated on this machine, `npm run
generate:icon:dev` renders the amber set into `src-tauri/icons/dev/`.

## The guards

| Guard | Catches |
| --- | --- |
| `npm run check:bundle` | A release binary carrying `CURSED-CHANNEL:DEV`, one built after `channel.rs` and carrying no marker at all, and anything dev-named staged in `dist-release/`. A binary older than `channel.rs` cannot carry a marker, so it is reported as stale and skipped rather than failed — silently skipping it would read exactly like passing. |
| `npm run build:dev` | A dev build that is missing either half of its invocation. |
| `channel.rs` tests | `dev.tauri.conf.json` disagreeing with the dev constants, including icons still pointing at the shipped set. |
| `paths.rs` tests | The two channels sharing a data root, or the dev channel inheriting the old folder. |
| `scheme.rs` tests | The two channels sharing a scheme prefix. |
| `brand.rs` tests | The two icons sharing an accent colour, or the dev icon becoming a second mark rather than a tint. |

The marker check reads the uncompressed `.exe` under `target/`, never the
installer: NSIS compresses its payload with LZMA, so a marker inside one is not
findable as text and searching it would pass on every build, including a bad one.

Run the test suite for both channels — a `#[cfg]`-gated constant means the
default run never compiles the other channel's code:

```bash
cargo test                          # from src-tauri
cargo test --features dev-channel
```
