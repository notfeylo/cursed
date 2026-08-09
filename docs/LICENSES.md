# Licences

## Cursed

MIT License — Copyright (c) 2026 feylo. See `LICENSE` in the repository root, or
the summary under Settings → About.

All bundled cursor artwork is original work by feylo, licensed MIT alongside the
source.

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

**No Windows or Microsoft trademark, logo, or asset is used, bundled, or
implied.** Cursed is not affiliated with, endorsed by, or sponsored by
Microsoft.
