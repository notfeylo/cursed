# Changelog

What changed, and — where it matters more — what was wrong before.

Versions are dated by their tag. Entries are written for the person the change
happened to, not for the person who made it.

---

## 1.21.1 — unreleased

### Updating happens inside the app now

No installer window appears at any point. No wizard, no progress window, no
Next button. The installer runs fully silently and Cursed reports its own
progress; if the app was in the tray when the update started, it goes back to
the tray rather than throwing a window at you.

### Uninstalling no longer deletes your work by default

**If you install Cursed over an existing copy by downloading it from the
website, read this.** The installer shows an "Already Installed" page with
*"Uninstall before installing"* already selected, so pressing Next through the
defaults ran the uninstaller — and the uninstaller offered to delete your
presets and custom cursors with **delete** as the default answer.

That is now the other way round. Keeping your work is the default; deleting it
is a question you have to answer *yes* to. A silent or automated uninstall keeps
your data too, because none of those is a person asking for it to be removed.

The in-app update path was already protected. This is the path that was not.

### Check for updates stopped contradicting itself

Pressing **Check for updates** found the new version, showed the download
button, and then reverted to "You're on the latest version" about a second
later — leaving no way to click through and update.

The button was right and a cache was overwriting it. A manual check returned its
answer to the panel without recording it anywhere, while the panel re-read the
*background* check's result every 1.5 seconds — a result from up to six hours
earlier, when there was nothing newer. Both now write the same place.

The same poll also re-checked GitHub every 1.5 seconds whenever nothing had been
recorded yet, which is forty requests a minute and exhausts the hourly rate
limit in under two minutes.

---

## 1.21.0 — 2026-08-17

### Updating no longer deletes your data

**If you have updated Cursed at any point up to and including 1.20.0, and your
presets, custom cursors or imported packs disappeared, this is why. It was our
bug, not something you did.**

Every in-app update from 1.0.0 through 1.20.0 launched the new installer with no
command line at all. The installer had no way to know it was performing an
update, so it took its fresh-install path: it offered to uninstall the previous
version first, with that option already selected, and running that uninstaller
ran Cursed's uninstall hooks in full. Those hooks are correct for an uninstall
and catastrophic for an update. They

- put the machine back on the stock Windows pointer scheme, and
- asked whether to keep your presets and custom cursors, **with "no" as the
  pre-selected answer**, so pressing Enter deleted `%APPDATA%\Cursed` entirely.

Deleted along with it was `backup\original_scheme.json` — the record of what your
pointers looked like before you ever installed Cursed. That file is the only copy
of that information anywhere on the machine. Once a Cursed cursor is applied,
the pointers it replaced cannot be read back from Windows, so a lost snapshot
cannot be regenerated. If yours was lost, see the next entry for what the app
does about it now.

A guard against exactly this existed in the uninstall hooks and was written to
prevent exactly this outcome. It keys on the installer's `$UpdateMode` flag,
which is set from the command line and from nowhere else — so the guard was
correct, present, and unreachable.

The fix is on the launch side, in `updates::launch`: the installer is now run
with `/UPDATE /P /R /NS`. `/UPDATE` is the one that removes the data loss; the
other three give you a single progress bar instead of a wizard, relaunch the app
afterwards, and stop a second desktop shortcut appearing every time. Six tests
now assert those flags, and `docs/UPDATE_PATH_DIAGNOSIS.md` traces the original
failure line by line through the generated NSIS script.

Two related changes ship with it:

- `install_update` verifies the downloaded installer's checksum **before**
  tearing anything down, rather than closing the app around an installer that
  might never run.
- The MSI is gone. `bundle.targets` is NSIS alone, and CI no longer uploads an
  MSI artifact. No release ever published one, but anyone who downloaded a CI
  build got an installer that installs to a different directory than the real
  one, with its own uninstall entry.

### If your original pointer scheme was lost

Cursed used to capture the machine's pointers on first run and treat "no
snapshot on disk" as "first run". After the bug above, that is no longer a safe
assumption: a machine can have no snapshot *and* a Cursed cursor already applied,
and capturing then would record one of our own cursors as your original — making
"Restore Windows defaults" permanently, invisibly wrong.

So it no longer captures in that state. It records the snapshot as lost, falls
back to the stock Windows scheme for restore purposes, and tells you once, in
Settings, that this happened. Restore still works and still gives you a normal
Windows pointer; it just cannot promise to give you back a customisation you had
before Cursed, because that information is gone.

### Background removal says no when it means no

Importing a photograph used to produce a cutout. Not a good one — an
unrecognisable blob, most of the image eaten, the subject destroyed — but the
app returned it as though it had worked.

Automatic background removal is a flood fill with a tolerance. That is right for
a flat background and has no correct answer on a photograph: a gradient, grass,
a shadow with no edge to stop at, and a vignette that leaves the corners
disagreeing with each other. Too tight and it stops at the first blade of grass;
too loose and it walks straight through the subject. There is no value in
between.

So it no longer tries. The image is scored first, and one that cannot be keyed
is left exactly as it arrived — byte for byte — with a sentence saying why and a
button offering to open the editor. You can still overrule it and have it try
anyway.

**And there is now an editor**: a checkerboard preview, erase and restore
brushes, undo, a tolerance slider that re-cuts live, and reset to the original.

Two more things fixed on the way:

- A small logo on a large canvas came back with **no background removed at
  all**. The old code refused to key anything that cleared more than 97% of the
  image, on the reasoning that clearing almost everything means the subject was
  the background — which is true of a destroyed photograph and false of the most
  common shape of cursor art there is.
- A compressed image came back with a **pale halo** traced around the artwork.
  That is JPEG ringing surviving the cut; it is now taken back out.

### Everything else

- Every state file — settings, presets, the applied-cursor descriptor, the
  window position and the original-scheme snapshot — is now written through one
  store that writes to a temporary file, flushes, `sync_all`s to the hardware,
  and only then renames. Each keeps a `.bak` beside it, and a file that will not
  parse is set aside rather than overwritten. Previously three of the five did a
  rename without a flush, one wrote straight over the top, and one treated a file
  it could not parse as an empty one — which is a deletion, delayed until the
  next save.
- Cursed is developed as two side-by-side installs, a user channel and a dev
  channel, so the released build can be watched over days on the same machine
  that is building the next one. Nothing about this is visible in the release
  build; `docs/CHANNELS.md` explains it.

---

## 1.20.0 — 2026-08-11

- An update that stopped part-way through reported itself as a smaller update
  rather than as a failure.
- The tag workflow could publish a draft release over the top of the release it
  was tagging.

## 1.19.0 — 2026-08-11

- A photograph imported straight from a phone came in on its side; the
  orientation tag cameras write is now honoured, and tested.
- Downscaled photographs looked muddy. Resampling now happens in linear light.
- Cutting a background out left crumbs and isolated spots behind; they are swept
  up now.
- CI was linting a different compiler than the one that ships, and checking a
  directory it never wrote to.
- The 215 generated packs the catalog no longer contains were dropped from the
  repository.

## 1.18.0 — 2026-08-11

- **An update no longer runs the uninstaller's hooks.** This is the fix that was
  believed to close the data-loss hole above. It added the `$UpdateMode` guard,
  which is correct and, as of this release, still unreachable — see 1.21.0.
- The catalog is 36 bundled packs rather than 291 generated ones.
- The size control decides the size, rather than inheriting whatever size a
  cursor happened to be applied at, and the corrected value is written back once
  instead of being corrected on every launch.
- Animated cursors follow the size control.
- Whether the hand and the I-beam grow with the pointer is now a choice, and it
  is off by default: they mark the thing being pointed at, and a 128 px hand
  covers it.
- The matte stopped eating textured subjects, and detail survives a downscale.
- The site was rebuilt: scroll-driven motion, an orbit, an FAQ, a real header and
  footer.

## 1.17.0 — 2026-08-10

- Uninstalling leaves nothing behind — no files, no registry keys, no scheme
  entries, no autostart, no shortcuts, and not the WebView2 folder keyed by
  bundle identifier that almost every app of this kind forgets. There is a script
  that asserts it.

## 1.16.0 — 2026-08-10

- Builds for every Windows a release can actually run on: x64, ARM64 and 32-bit,
  plus an offline installer with the WebView2 runtime embedded.
- Cursors are shown live on hover, colour updates as you choose it, scrolling
  glides, and the matte finds dark backgrounds.

## 1.13.0 — 2026-08-09

- 291 built-in packs, and background removal became a choice rather than
  something that happened to your image.

## 1.12.2 — 2026-08-09

- The update button worked and looked like it did nothing.

## 1.12.1 — 2026-08-09

- The size control scaled the hand and the I-beam along with the pointer.

## 1.12.0 — 2026-08-09

- Complete cursor sets ship where the licence allows it.

## 1.11.0 — 2026-08-09

- Small sizes actually apply, and the cut-out leaves nothing behind.

## 1.10.0 — 2026-08-09

- The whole app is themed, backgrounds are cut properly, and a custom cursor can
  be opened again after it is built.

## 1.9.1 — 2026-08-09

- A custom cursor covers the busy roles too, not only the arrow.

## 1.9.0 — 2026-08-09

- Your cursor is kept everywhere, any background can be cut out, and custom
  cursors have a library of their own.

## 1.8.3 — 2026-08-09

- Animated cursors animate the moment they are applied, rather than 30 seconds
  later when the watchdog reloaded them.

## 1.8.2 — 2026-08-09

- The mark's glow became a fold and a cast shadow.

## 1.8.1 — 2026-08-09

- The supplied mark, traced from the artwork, replaced the drawn approximation.

## 1.8.0 — 2026-08-09

- A new identity: the sigil mark across the app, the icons and the site; Space
  Grotesk, Inter Tight and JetBrains Mono; a build-detail panel in About and a
  dev-only specimen sheet.
- The repository moved to `notfeylo/cursed`.

## 1.7.0 — 2026-08-08

- Logging is always on at info level, so the one time a problem reaches a user
  there is something on disk to read. The setting controls verbosity now, not
  existence.
- The running version is visible in the app, and there is one command that sets
  it everywhere.

## 1.6.1 — 2026-08-08

- A diagnostics report, and tests that pin the empty-catalog bug shut.

## 1.6.0 — 2026-08-08

- The built-in cursors ship with the app, and complete schemes can be imported.

## 1.5.1 — 2026-08-08

- The legal documents are readable in the app.

## 1.5.0 — 2026-08-08

- Automatic background update checks, and a home screen worth looking at.

## 1.4.1 — 2026-08-08

- Uninstalling could not restore cursors, and the imported list showed
  everything rather than only imports.

## 1.3.0 — 2026-08-08

- Renamed to Cursed. The catalog was deduplicated and the updater fixed.

## 1.2.0 — 2026-08-08

- Cursors are shown in their own colours; tinting became an option.
- A successful apply reported itself as an error.
- Imported artwork was tinted flat.
- BOMs written into the config files by our own tooling are stripped.
- Your own cursors can be imported, and updates are somewhere you can find them.

## 1.1.0 — 2026-08-07

- 216 schemes, a verified updater, a new mark, and a hardened command surface.
- The landing page, deployed to Vercel.
- Memory is handed back when the window hides to the tray.

## 1.0.0 — 2026-08-07

- The first release: the three-layer pointer engine, hand-written `.cur` and
  `.ani` writers, 116 parametric packs covering all 17 pointer roles, cursors
  built from your own images, the tray, hotkeys and autostart, and the six
  screens.

---

### On the gaps

There is no 1.14.0 or 1.15.0, and no 1.4.0. Those numbers were burned during
release-script work and never shipped; nothing is missing from this file.
