# Licences

## Cursed

MIT License — Copyright (c) 2026 feylo. See `LICENSE` in the repository root, or
the summary under Settings → About.

The application's own artwork — the mark, the pointer, the link hand, the text
I-beam and the `GAP-CROSS` blend base — is original work by feylo, licensed MIT
alongside the source.

**The bundled cursor packs are not.** See below.

## Bundled cursor packs

Thirty-six packs are embedded in the installer and unpacked on first run. Since
the generated catalog was cut back to the single pack that fills unmapped roles,
these are the catalog.

### Licensed for redistribution — 2

| Pack | Licence | Author |
| --- | --- | --- |
| Geared Brass | GPL-3.0 | piraker-grinor |
| Geared Steel | GPL-3.0 | piraker-grinor |

Both archives carry their own `LICENSE.txt` and `COPYRIGHT.txt`, extracted
alongside the cursors. Cursed itself stays MIT; these sit beside it as
separately-licensed data, which is what the GPL calls mere aggregation.

### No stated licence — 34

The remaining packs came from cursor-sharing sites and **state no licence**.
Where a `readme.txt` or `COPYRIGHT.txt` exists it names an author without
granting any right to redistribute; most name nobody at all.

Several depict characters, logos or products owned by third parties, among them:
Batman and the Batarang (DC Comics), Spider-Man and Venom (Marvel), Hello Kitty
and Kuromi (Sanrio), Minecraft items and tools (Mojang/Microsoft), Skyrim
(Bethesda), Hollow Knight and Silksong (Team Cherry), Jujutsu Kaisen and Naruto
(Shueisha), Roblox, Supreme, BMW, Toyota, and the likeness of Cristiano Ronaldo.

**No licence here permits shipping these in an installer.** It is a decision the
project's owner took knowingly, having been given the position above first. It
is written down rather than left unstated so that nobody — a contributor, a
user, or a rights holder — has to discover it.

If a rights holder objects, removal is one list: the `PACKS` array in
`src-tauri/src/bundled.rs` and the matching archive under `assets/bundled/`.
Nothing else in the application depends on which packs are present.

## Bundled fonts

Three families ship inside the application as latin-subset WOFF2 files, each
under the **SIL Open Font License 1.1**:

- **Space Grotesk** — © Florian Karsten
- **Inter Tight** — © The Inter Project Authors
- **JetBrains Mono** — © JetBrains s.r.o. and the JetBrains Mono Project Authors

> Copyright (c) the respective authors.
>
> This Font Software is licensed under the SIL Open Font License, Version 1.1.
> This licence is available with a FAQ at <https://openfontlicense.org>.
>
> PERMISSION & CONDITIONS: Permission is hereby granted, free of charge, to any
> person obtaining a copy of the Font Software, to use, study, copy, merge,
> embed, modify, redistribute, and sell modified and unmodified copies of the
> Font Software, subject to the conditions of the licence, including that
> neither the Font Software nor any of its individual components may be sold on
> its own, and that copies must retain the above copyright notice, this notice,
> and these conditions.
>
> THE FONT SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND.

## Bundled cursor packs

Two complete cursor sets ship inside the installer alongside the generated
catalog. They are **not** our work, and they are here because their authors
licensed them for redistribution:

| Pack | Author | Licence |
| ---- | ------ | ------- |
| Geared Brass | piraker-grinor | GNU GPL v3.0 |
| Geared Steel | piraker-grinor | GNU GPL v3.0 |

Each pack's own `LICENSE.txt` and `COPYRIGHT.txt` are extracted alongside its
cursors into `%APPDATA%\Cursed\imported\`, so the licence travels with the
work. Cursed itself remains MIT; these sit beside it as separately-licensed
data, which is what the GPL calls mere aggregation.

**No pack ships without a stated licence.** Cursors imported from your own
folders stay on your machine and are never redistributed — that is true whether
they are freely licensed or not, and it is why the catalog you see may be larger
than the one a new install starts with.

## Third-party crates and packages

Cursed is built on open-source libraries, each under its own permissive
licence (MIT, Apache-2.0, BSD, MPL-2.0, or Zlib). The authoritative,
version-exact list for any build is produced by `cargo deny list` and
`npm ls --production` and is published with each release.

The principal ones:

| Component            | Purpose                              | Licence          |
| -------------------- | ------------------------------------ | ---------------- |
| Tauri                | Application shell                    | MIT / Apache-2.0 |
| React                | User interface                       | MIT              |
| Tailwind CSS         | Styling                              | MIT              |
| Zustand              | State                                | MIT              |
| lucide-react         | Icons                                | ISC              |
| `windows`            | Win32 bindings                       | MIT / Apache-2.0 |
| `winreg`             | Registry access                      | MIT              |
| `image`              | Image decoding                       | MIT / Apache-2.0 |
| `fast_image_resize`  | Lanczos3 resampling                  | MIT / Apache-2.0 |
| `resvg` / `tiny-skia`| SVG rasterisation                    | MPL-2.0 / BSD-3  |
| `zip`                | `.cfpack` archives                   | MIT              |
| `serde`              | Serialisation                        | MIT / Apache-2.0 |

## Downloaded on request — photo mode

**None of this is in the installer.** It is fetched only when somebody turns
photo mode on, checked against a published SHA-256 *and* a signature made with
this project's release key before it is loaded, and deleted again when photo
mode is removed. `docs/PHOTO_MODE.md` carries the sizes and the reasoning.

| Component | Purpose | Licence |
| -------------------- | ------------------------------------ | ---------------- |
| `u2netp.onnx` (U²-Net) | The learned matte | Apache-2.0 |
| ONNX Runtime | Runs the model | MIT |
| Microsoft Visual C++ Runtime | What the ONNX Runtime is built against | Microsoft distributable code |

The Visual C++ runtime files — `msvcp140.dll`, `msvcp140_1.dll`,
`vcruntime140.dll` and, on x64, `vcruntime140_1.dll` — are Microsoft's, taken
unmodified from the `VC/Redist/MSVC/<version>/<arch>/Microsoft.VC*.CRT`
directory that Visual Studio installs so that applications may deploy them, and
redistributed under the Visual Studio distributable-code terms. They are carried
because the ONNX Runtime imports them and Windows does not include them; a
machine that already has the redistributable never uses these copies.

**No Windows or Microsoft trademark or logo is used or implied, and nothing
Microsoft publishes ships inside the installer.** The Visual C++ runtime above
is the only Microsoft component Cursed ever places on a machine, it arrives only
if photo mode is turned on, and it is there because another dependency requires
it. Cursed is not affiliated with, endorsed by, or sponsored by Microsoft.
