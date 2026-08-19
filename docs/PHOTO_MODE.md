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
| `onnxruntime.dll` (x64) | **16,149,344 bytes (15.40 MB)** | MIT |
| **Total, first use, x64** | **≈ 19.76 MB** | |

ARM64 and x86 need their own runtime build and are roughly the same size. The
model is architecture-independent, because ONNX is a portable graph.

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

**The plumbing is built and tested; the artifacts are not yet published.**
`MODEL.sha256` and the runtime hashes are empty, and `verify` refuses an
artifact with no published checksum — so photo mode currently reports itself as
available, and declines to install until the `photo-v1` release exists with
signed artifacts and their hashes compiled in.
