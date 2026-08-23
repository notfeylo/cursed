# Changelog

What changed, and — where it matters more — what was wrong before.

Versions are dated by their tag. Entries are written for the person the change
happened to, not for the person who made it.

---

## 1.24.0 — 2026-08-23

A quality release. Both of these look like "the cursors got worse", both have
been shipping for a while, and neither was caused by anything that changed
recently.

### The grey shadow around a cut-out, on light backgrounds only

A cursor made with **photo mode** looked clean on a dark desktop and had a grey
rim around it on a white one — a halo that read as a drop shadow the app never
drew.

The edge of a cut-out is not fully opaque; it is part subject, part whatever was
behind it. Photo mode worked out *how much* of each pixel was subject and then
kept the pixel's original colour, which still had the background mixed into it.
Against a dark desktop that leftover is invisible. Against a white one it is a
dirty outline.

The colour is now unmixed from the background at every soft edge, using the
background actually found next to that edge rather than a guess. Cursors made
from photographs sit on any wallpaper without an outline around them.

This only ever affected photo mode. Cursors keyed the ordinary way have had this
correction since the day they had a soft edge at all.

### A pointer that went blocky if you had made it large

If your pointer size came from **Windows' own accessibility setting** rather
than from this app, and you had it above about half-way, every cursor was
blurry and blocky — enlarged, with soft stair-stepped edges.

Cursed built each cursor at eight sizes, the largest being 128 px, because that
is as large as this app's own size slider goes. Windows' pointer-size setting
writes the same value and goes twice as far. Anything past 128 px was Windows
stretching our 128 px picture, with the crude enlargement the shell uses for
that.

Every cursor now carries 192 px and 256 px as well, drawn at those sizes rather
than stretched into them. Large pointers are as sharp as small ones.

- Cursors you import from a photograph or a PNG still will not be enlarged past
  four times their original size — inventing detail that was never in the file
  makes it look worse, not better.
- Animated cursors pick the size above what is being asked for and shrink it,
  rather than picking the one below and stretching it.
- Cached cursors take more room on disk than they did. Nothing in the download
  changed, and the cache rebuilds itself.

---

## 1.23.0 — 2026-08-22

### Turn and mirror your own artwork

An arrow pointing the wrong way used to mean opening an image editor and
importing it again. There is now an **Orientation** row on the custom import
screen: rotate left, rotate right, mirror, flip, and reset.

- Right angles only, on purpose. An arbitrary angle has to be resampled, and at
  32 pixels that softens every edge of the thing you are making.
- **The hotspot turns with the picture.** A click point placed on the tip of an
  arrow is still on the tip after a quarter turn, rather than staying where it
  was on screen and quietly coming to mean the middle of the shaft.
- Animations turn frame by frame, so a turned animation does not jump on its
  second frame.
- Reset puts the artwork back the way it arrived, and brings the click point
  home with it.

### Photo mode could not load on a clean PC

Photo mode reported *"the photo-mode runtime could not be loaded …
LoadLibraryExW failed"* on any Windows that had never had developer tools
installed on it — which is most of them.

The ONNX Runtime is built against the Microsoft Visual C++ Runtime, and Windows
does not include it. Cursed itself never needed it and still does not: it runs
on a bare install of Windows, which is exactly why this only ever showed up in
the one feature that reaches for something else. Every machine the feature was
written on already had those files, because installing a compiler installs them.

Photo mode now carries that runtime with it, per architecture, checked against
the same published checksum and signature as everything else it downloads. The
first-use download grows by about 0.9 MB on 64-bit.

If you already have photo mode installed, installing it again fetches only the
missing piece rather than the whole twenty megabytes.

And when a library genuinely cannot load, the message now says what Windows
said — including which file is missing — instead of four words that fit every
possible cause equally badly.

### Photo mode held half a gigabyte

Removing one background committed **554 MB** and never gave it back, whatever
the size of the picture: a 64x64 image cost exactly as much as a 19-megapixel
one. That is ONNX Runtime's arena allocator, which takes memory as the model
runs and keeps it for the next run — the right default for a server and the
wrong one for something that sits in a tray.

On a machine with room to spare it was waste. On a small one it was fatal, and
it closed the app with nothing in the log to say why.

One cutout now costs **26 MB** instead of 554, and gives it back afterwards. The
model runs about 11 ms slower on a 91 ms inference, which is not a trade worth
thinking twice about.

---

## 1.22.0 — 2026-08-21

### Cursor files can be imported

Dropping a `.ani` or `.cur` on the import screen was answered with *"That file
isn't something Cursed can use: only PNG, JPEG, GIF, WebP and BMP images can be
imported"* — from a cursor app, about a cursor. The file picker had offered
`.cur`, `.ani`, `.ico` and `.tif` for months; the decoder had never been able to
read any of them.

All four import now.

- An **`.ani` arrives as an animation**, every frame with the delay it was
  authored with, and plays back in the order its `seq` chunk asks for rather
  than the order the frames happen to be stored in.
- A **`.cur` keeps the hotspot it was made with**, carried through the trim and
  the squaring so it still points at the same pixel of the artwork. Guessing it
  is what makes a converted cursor click slightly to the left of where you point.
- **`.ico` and TIFF** work, which is what the picker had been claiming.
- Monochrome cursors — the crosshairs and I-beams Windows itself ships — are
  drawn almost entirely with the "invert the screen" state, and a reader that
  treats that as transparency returns a perfectly empty picture. Four of
  Windows' own cursors imported as nothing at all. They are read properly now.

All 189 cursor files in `C:\Windows\Cursors` decode, rebuild and re-export.

### A cursor that was already cut out is no longer called a photograph

Importing any downloaded cursor pack put this on screen:

> This looks like a photo. Automatic background removal works on flat
> backgrounds — logos, icons, screenshots — and it will not do a good job here.

Nothing was wrong with the file. Every `.ani` and every cursor PNG arrives with
its background already gone, and the check that decides whether a background
*can* be removed was reading the colour of pixels behind alpha 0 — which is
arbitrary data, reads as a busy high-contrast border, and fails every test.
The still path had always asked "is there anything left to remove" first. The
animated path did not.

It now says what is actually true: nothing to remove, this image is already cut
out.

### Photo mode produces a cutout

Photo mode could be downloaded and verified, and then did nothing: the model was
on the disk and nothing ran it. It runs now — a person, a car, a pet or an
object out of a real photograph, in about a quarter of a second, entirely on
this machine. Settings has the panel that installs and removes it, and the
refusal above offers it directly when the ordinary remover declines a photo.

### Large photographs are no longer refused

Any image over 4,096 pixels on a side was rejected with *"5824x3264, limit is
4096x4096"*. A 24-megapixel camera, an 8K wallpaper and most stock photography
are all over that on one axis, and every one of them is resampled down within
milliseconds of arriving — the limit was costing nothing and refusing ordinary
pictures. The guard is now a budget in pixels, which is what it was defending in
the first place: 40 megapixels in, and a decompression bomb still stopped.

### An update that cannot start no longer takes the app with it

If the installer failed to launch — a scanner holding the file, a quarantine —
the app had already released its hotkeys, removed its tray icon and hidden its
window, so the error was reported to a window nobody could see. From the outside
Cursed simply vanished, with the pointer scheme left undefended for the rest of
the session. Everything is put back now, and the failure is an error message in
a working app.

A second one: on a PC too old to run Cursed, a silent update would stop on a
message box with no window to belong to and wait forever for a click nobody
could find. And a release that publishes no installer for your processor now
says so and offers the releases page, rather than insisting you are up to date.

---

## 1.21.1 — 2026-08-19

### Photo mode: an optional background remover for photographs

The built-in background removal is a flood fill. It is exact and instant on the
artwork this app is for — logos, icons, screenshots, crosshairs — and it cannot
cut out a person. Lit skin sits a few levels from white, so a tolerance wide
enough to remove the background is wide enough to walk into the face; hair is
semi-transparent at the strand level, which needs alpha matting rather than
segmentation. A portrait came back with the whole face removed and only hair and
eyes left, and no amount of tuning fixes that.

So photographs now have their own path, using a learned model — and it is
**optional and downloaded only when you ask for it**, because it is about 20 MB
against an installer of 11. Nothing downloads at launch. Settings has a **Remove
photo mode** button that deletes it again and tells you what you got back.

The model is u2netp (4.36 MB, Apache 2.0) and the runtime is ONNX Runtime
(15.4 MB). Both are checked against a published SHA-256 **and** a signature made
with the release key before the library is ever loaded — a downloaded library
that runs inside the app is a bigger trust decision than an installer you
double-click, and it is treated that way.

Photo mode is unavailable in the offline installer and says so plainly.
`docs/PHOTO_MODE.md` has the sizes, the licences and the verification chain.

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
