# Photo mode

An optional learned matte for photographs, downloaded on request.

## Why there are two background removers

The classical path in `build::matte` is a flood fill with a tolerance. It is
exact, instant, and correct for what this app is actually for: logos, icons,
screenshots, game crosshairs. It cannot cut out a person.

That is not a tuning problem. Lit skin on a studio-lit face runs a few levels
from white, so a tolerance wide enough to key the background is wide enough to
walk into the face — and hair is semi-transparent at the strand level, which is
alpha matting rather than segmentation. `verification/background-removal.md`
records the failure that settled it: a portrait came back as fifty disconnected
islands with the whole face removed.

So the split is honest rather than reluctant:

| Input | Path |
| --- | --- |
| Logos, icons, flat art, screenshots, crosshairs | **classical** — instant, exact, nothing to download |
| Photographs, people, hair, textured subjects | **photo mode**, or an honest refusal |

## What downloads

Nothing at launch. Nothing without being asked.

| Artifact | Size | Licence |
| --- | --- | --- |
| `u2netp.onnx` | **4,574,861 bytes (4.36 MB)** | Apache 2.0 |
| `onnxruntime-x64.dll` | **16,149,344 bytes (15.40 MB)** | MIT |
| `onnxruntime-arm64.dll` | **16,261,432 bytes (15.51 MB)** | MIT |
| `onnxruntime-x86.dll` | **10,884,640 bytes (10.38 MB)** | MIT |

| Architecture | First-use download |
| --- | --- |
| x64 | **19.76 MB** |
| ARM64 | **19.87 MB** |
| x86 | **14.74 MB** |

The model is architecture-independent, because ONNX is a portable graph.

**32-bit is pinned to ONNX Runtime 1.22.0**, because Microsoft stopped
publishing a `win-x86` build after it — 1.29.0 ships x64 and ARM64 only. The
alternative was dropping photo mode for 32-bit users, which is a worse answer
for a feature that works fine there.

Every figure above was measured from the actual artifact, not estimated. The
80 MB figure on the ONNX Runtime release page is the full SDK — headers, import
libraries, tooling — of which only the one DLL ships.

### The model

**u2netp**, the small variant of U²-Net, from
[`xuebinqin/U-2-Net`](https://github.com/xuebinqin/U-2-Net) under **Apache 2.0**,
taken from the `rembg` release assets, which is the same artifact `rembg` ships.

Chosen over MODNet deliberately. MODNet is portrait-specific and its README now
places its models under Apache 2.0 as well — the non-commercial framing many
people remember is not in the current text — but its pretrained weights'
training-data provenance is undocumented, and more importantly people import
logos and crosshairs far more often than faces. General salient-object detection
matches that distribution; a portrait matter does not. MODNet remains a clean
second model to add if portraits turn out to be common.

## How it is verified

This downloads a **library and loads it into this process**. That is a bigger
trust decision than the installer, which at least passes SmartScreen and the
user's own double-click. It gets a stricter version of the same treatment:

1. Fetched over WinHTTP, through the OS certificate store, like every other
   request this app makes.
2. **SHA-256** compared against the hash compiled into the build.
3. **minisign signature** verified with the release key — the same key that
   signs installers. See [`SIGNING.md`](SIGNING.md).
4. Only then written, and only then loaded.

A failure at any step deletes the file and falls back to the classical path. A
build with **no published checksum refuses outright** rather than loading a
library on the strength of its filename.

The release tag is **pinned**, never `latest`: a copy of the app compiled today
must keep fetching the artifact it was tested against, even after a later
release publishes a different runtime.

## Where it lives, and removing it

`%APPDATA%\Cursed\models\`.

Settings has a **Remove photo mode** button that deletes both files and reports
the space reclaimed. A twenty-megabyte download the user cannot get rid of is a
bad citizen.

## The offline build

`Cursed-Setup-Offline-x64.exe` exists so air-gapped machines work. Photo mode is
the one feature that cannot work there, because it is defined by fetching
something. The `offline-build` cargo feature compiles it out, and the UI says:

> Photo mode needs a one-time download and isn't available in the offline build.

Not a spinner, not a silent failure.

## What is still true of the result

The model produces a **soft alpha matte**, which is fed through the existing
despill and edge-refinement stages rather than replacing them — the fringe
correction that fixed the JPEG halo applies to a learned matte as much as to a
keyed one.

And the fragmentation safety net in `matte::survivor_is_coherent` applies to the
model's output too. A model can fail; the net is not classical-only.

## Status

**Published and installable.** The `photo-v1` release carries all four artifacts
and a `.minisig` for each, and their SHA-256 hashes are compiled into the build.

| Artifact | SHA-256 |
| --- | --- |
| `u2netp.onnx` | `309c8469258dda742793dce0ebea8e6dd393174f89934733ecc8b14c76f4ddd8` |
| `onnxruntime-x64.dll` | `69d8e6d3879a3b4001cdc74c8ed9ccc7e7f799a5b847059738323404519ec471` |
| `onnxruntime-arm64.dll` | `7c7df2cefd6910f50f44792e8f8f71b371bf9675f9273e70a9277eb92e4d75ed` |
| `onnxruntime-x86.dll` | `f898b430bb6130b8c1394f98ea1c6f4134752919cf96601da27537a8b9458fdb` |

Every signature was verified against the public key locally before the release
was published.

**Wired end to end.** The `ort` session, the pre- and post-processing, the
`Cut::Photo` path through the pipeline, the Settings panel that installs and
removes it, and the offer on the refusal banner are all in place. Photo mode
installs, runs and produces a cutout.

### How it runs

| Stage | What happens |
| --- | --- |
| Load | `ort::init_from` resolves the downloaded DLL on **first use**, never at launch |
| Prepare | resize to 320x320, composite onto black, ImageNet normalisation, NCHW |
| Run | one session, cached between images; the first of the seven outputs is the fused map |
| Post | normalise the map against its own extremes, scale it back to the image, multiply into alpha |
| Check | `matte::survivor_is_coherent` — a model that shreds the subject is reverted, exactly as a bad flood fill is |

Measured on the development machine (x64, release build): **236-952 ms** per
image end to end, including decode, for sources from 1,200x800 to 5,824x3,264.
An animation is one inference **per frame**, because the subject moves.

### The one thing the full-resolution re-cut must not do

`prepare_master` runs its cut on a proxy capped at `WORKING_CAP`, then crops the
original to the subject and cuts again at full resolution. **The learned matte
is not re-run there.** The model answers "which part of this picture is the
subject", and the crop has already made the subject the whole picture — asked
again it says "all of it", hands back a fully opaque region, and that region
replaces the good cut that produced the crop. The symptom was exact and
reproducible: 78% of a photograph removed, and then the original returned
uncut. The proxy's alpha is scaled onto the full-resolution pixels instead,
which is the part that was worth having.

### Removing it while it is loaded

Windows will not delete a DLL that is mapped into a running process, and after
one cutout this one is. `remove()` drops the session, deletes what it can, and
records a marker for anything left; photo mode reports itself uninstalled from
that moment and the next launch sweeps up the file. Without that the Remove
button silently frees nothing and the size on screen never drops.
