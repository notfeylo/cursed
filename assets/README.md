# `assets/`

Cursor artwork. Two directories that look similar and are not.

## `bundled/` — what ships

Eighty-three hand-made packs, one `.zip` each, holding real `.cur` and `.ani`
files. They are embedded in the binary by `include_bytes!` in
[`../src-tauri/src/bundled.rs`](../src-tauri/src/bundled.rs) and unpacked on
first run, so a machine that has never seen the app arrives with the whole
catalog and no download. They are the reason the installer is ~14 MB rather than
~8.5 MB, and that is the trade the catalog change in 1.18.0 made deliberately:
83 sets somebody drew beat 291 permutations of the same arrow.

**Adding one:** drop the zip here and add a `Bundled { .. }` line to
`bundled.rs`. The slug is the file name; the label is what the UI shows. Extract
straight into a directory named for the slug — the importer names a pack from
its *label*, and for a slug and label that differ only in case
(`sizenwse` / `SizeNWSE`) a Windows filesystem treats them as the same
directory, which is how five packs once deleted themselves on install.

Three rules about what goes **inside** the zip, each of which is now a test in
`bundled.rs` rather than a thing to remember:

- **Name the files for the role, not for the artwork** — `arrow-role.cur`,
  `hand-role.ani`. The importer reads a role out of the whole filename stem and
  checks "hand" before "cursor", so a file called
  `Minecraft Steve Raising Hands--cursor--....cur` installs as the *hand* and
  the pack ends up with no pointer.
- **Ship the cursor files and nothing else.** Download sites ship a `.png`
  preview beside every cursor; the importer reads images too, so both claim the
  same role at the same confidence and directory order decides which one the
  user gets. Readme files go the same way — the importer reads an author out of
  a `.txt` and shows it in the app, which is not always wanted.
- **Check inside the `.ani` as well.** An `.ani` is a RIFF file and carries a
  `LIST INFO` chunk holding the download's title and the name of whoever drew
  it. Deleting the readme does not touch it, a directory listing does not show
  it, and Windows displays it in the file's own properties. Strip the chunk —
  the animation is `anih` plus the `fram` list and does not need it.
- **The label must not end in "Cursor", "Pointer", "Link", "Normal" or "Set".**
  The importer strips those as download-site noise, so the pack installs to a
  directory the launch check never looks at and is re-imported forever.

**Licensing is not uniform and is not hidden.** Two of these are licensed for
redistribution; thirty-four state no licence at all; forty-seven were given for
use here by the person who drew them. Several depict characters owned by other
people.
[`../docs/LICENSES.md`](../docs/LICENSES.md) says exactly which is which and why
they ship anyway. That is the owner's decision, written down rather than left
for somebody to discover.

## `packs/` — generated artwork, kept for review

The SVG masters the app can draw itself, exported so they are **reviewable in a
diff**. Rendering happens in code at runtime; nothing here is read by the
shipped app, and deleting the directory would not change what a user sees. It
exists so a change to the drawing code shows up as a change to a picture rather
than as a change to a number in `art.rs`.

Since 1.18.0 that is exactly one pack — `precision-gap-cross`, the blend base
that fills the roles an imported pack leaves unmapped. The other 215 directories
were the generated catalog, and they went with it.

Regenerate after any change to `packs/art.rs`, `packs/styles.rs` or
`packs/brand.rs`:

```bash
npm run generate:packs        # writes assets/packs from the current code
```

CI runs the same command and fails if the result differs from what is committed,
so the artwork in the repo is always the artwork the code produces. Until 1.18.0
that check pointed one directory too high and silently compared nothing — if you
are reading an old diff and wondering how the committed hand cursor drifted from
the drawn one, that is how.

## Every pack defines all seventeen roles

Both kinds. A pack that leaves a role unmapped is a pack that hands the pointer
back to Windows the moment anything happens — a copy, a download, an app
starting — which reads to the user as the cursor having reverted. The catalog
build fails on a pack missing a role rather than shipping one that does that.
