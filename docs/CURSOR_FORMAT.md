# The `.cur` and `.ani` binary formats

Notes taken while writing `build/cur_writer.rs` and `build/ani_writer.rs`. Both
writers are hand-rolled against these layouts rather than delegated to a crate,
because the available ones do not carry a hotspot per resolution — which is the
one thing a multi-resolution cursor needs most.

All integers are little-endian.

---

## `.cur`

A cursor and an icon are the same container. Three parts, in order.

### 1. `ICONDIR` — 6 bytes

| Offset | Size | Field         | Value                          |
| ------ | ---- | ------------- | ------------------------------ |
| 0      | 2    | `idReserved`  | `0`                            |
| 2      | 2    | `idType`      | **`2`** — 1 is an icon         |
| 4      | 2    | `idCount`     | number of images               |

> **Mistake #1:** writing `idType = 1`. It produces a file that often still
> loads — as an *icon*, with no hotspot — so the cursor clicks from its top-left
> corner and the bug reads as "the click doesn't land where I point".

### 2. `ICONDIRENTRY` — 16 bytes, one per image

| Offset | Size | Field            | Value                             |
| ------ | ---- | ---------------- | --------------------------------- |
| 0      | 1    | `bWidth`         | width, **`0` means 256**          |
| 1      | 1    | `bHeight`        | height, **`0` means 256**         |
| 2      | 1    | `bColorCount`    | `0` for anything above 8 bpp      |
| 3      | 1    | `bReserved`      | `0`                               |
| 4      | 2    | `wPlanes`        | **hotspot X**                     |
| 6      | 2    | `wBitCount`      | **hotspot Y**                     |
| 8      | 4    | `dwBytesInRes`   | length of this image's data       |
| 12     | 4    | `dwImageOffset`  | offset from the start of the file |

> **Mistake #2, and the big one:** in a cursor, `wPlanes` and `wBitCount` do not
> mean planes and bit count. They are **reused to carry the hotspot**. An icon
> writer puts `1` and `32` there; the result is a cursor whose hotspot is at
> (1, 32) — a few pixels down and right of where the user aimed. This is the
> single most common defect in hand-rolled converters, and it is invisible until
> someone tries to click something small.
>
> The real plane count and bit depth live in the `BITMAPINFOHEADER` below.

Sizes are one byte, so 256 does not fit and is encoded as `0`.

### 3. Image data — `BITMAPINFOHEADER` + XOR mask + AND mask

```
BITMAPINFOHEADER   40 bytes
XOR mask           width × height × 4   BGRA, bottom-up
AND mask           stride × height      1 bpp, bottom-up
```

| Offset | Size | Field             | Value                                  |
| ------ | ---- | ----------------- | -------------------------------------- |
| 0      | 4    | `biSize`          | `40`                                   |
| 4      | 4    | `biWidth`         | width                                  |
| 8      | 4    | `biHeight`        | **height × 2**                         |
| 12     | 2    | `biPlanes`        | `1`                                    |
| 14     | 2    | `biBitCount`      | `32`                                   |
| 16     | 4    | `biCompression`   | `0` (`BI_RGB`)                         |
| 20     | 4    | `biSizeImage`     | XOR bytes + AND bytes                  |
| 24     | 8    | pixels-per-metre  | `0`, `0`                               |
| 32     | 8    | palette counts    | `0`, `0`                               |

> **Mistake #3:** writing the real height. `biHeight` is **doubled** because the
> XOR (colour) and AND (transparency) masks are stacked into one image, and the
> header describes the pair rather than the picture.

**XOR mask** — BGRA, not RGBA, and rows run **bottom-up**: the last row of the
image is written first.

**AND mask** — 1 bit per pixel, also bottom-up. A **set bit means transparent**
("leave the screen alone"); the most significant bit of each byte is the
leftmost pixel. Rows are padded to a **4-byte boundary**:

```
stride = ceil(width / 32) × 4
```

> **Mistake #4:** forgetting the padding. At widths that are not multiples of
> 32 — 48, 96, 160, 192 are all in the shipped ladder — each row lands one to
> three bytes early and the mask shears diagonally across the image.
>
> **Mistake #5:** skipping the AND mask entirely because 32-bit cursors are
> drawn from their alpha channel. Usually true; not always. Some remote-desktop
> paths and legacy shells fall back to the mask, and when they do an
> all-zero mask paints a black box around the pointer.

---

## `.ani`

A RIFF container. The structural point worth stating plainly: **each frame is a
complete, valid `.cur` file** — header, directory, DIB and all. Not raw pixels,
not a stripped-down record. So the same writer produces both, and a bug cannot
exist in one path and not the other.

```
RIFF <size> ACON
  anih <36>          header
  rate <4 × steps>   per-step delays, in jiffies   (optional)
  seq  <4 × steps>   playback order                (optional)
  LIST <size> INFO   INAM / IART                   (optional)
  LIST <size> fram
    icon <size> <a whole .cur file>   × N
```

### `anih` — 36 bytes

| Offset | Field       | Value                                   |
| ------ | ----------- | --------------------------------------- |
| 0      | `cbSize`    | `36`                                    |
| 4      | `cSteps`    | steps in the animation                  |
| 8      | `cFrames`   | distinct frames                         |
| 12     | `cx`        | `0` when frames are icon data           |
| 16     | `cy`        | `0`                                     |
| 20     | `cBitCount` | `0`                                     |
| 24     | `cPlanes`   | `0`                                     |
| 28     | `jifRate`   | default delay, in jiffies               |
| 32     | `flags`     | `AF_ICON` (`0x1`), `+ AF_SEQUENCE` (`0x2`) |

`cx`, `cy`, `cBitCount` and `cPlanes` are zero because each embedded `.cur`
already describes its own dimensions and depth.

### Timing

A **jiffy is 1/60 second**:

```
jiffies = round(ms × 60 / 1000), clamped to 1..=100
```

Zero would make the shell spin; a very large value stalls the pointer.

`rate` and `seq` are only written when the frames are not evenly timed —
`AF_SEQUENCE` is set alongside them.

### Chunk alignment

Every RIFF chunk is word-aligned. A chunk with an odd payload gets a trailing
pad byte, and **the pad is not counted in the chunk's size field** but *is*
counted in the parent's.

### Caps

`.ani` has **no directory of resolutions** — the format simply cannot hold more
than one size. So a multi-resolution animated cursor means one file per size,
chosen at apply time from `CursorBaseSize` and the primary monitor's DPI.

Frames are capped at 60 and total duration at 4 seconds: beyond that the shell's
own animation cost becomes visible, which is a design decision rather than a
format limit.

---

## Verification

Every structural assertion above is a guess until Windows agrees. Both writers
are covered by round-trip tests that write real bytes to disk and call
`LoadImageW(…, IMAGE_CURSOR, …)`; the runtime does the same check before any
generated file can be installed. A cursor that fails to load surfaces as a clear
error rather than an invisible pointer.
