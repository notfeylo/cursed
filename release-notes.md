**Give your dead cursor a new life.**

Replace every Windows pointer role with a crisp, multi-resolution cursor scheme, or turn any image into a real `.cur` / `.ani`.

### Install

Download `CursorForge_1.0.0_x64-setup.exe` and double-click it. Per-user install, **no administrator prompt**, no terminal.

The installer is unsigned, so SmartScreen may say *Windows protected your PC*. Choose **More info** then **Run anyway**, or verify against `SHA256SUMS.txt` first.

### What is in it

- **116 full schemes**, each defining all **17** pointer roles
- Rendered from vectors at **32-256 px** into one multi-resolution file, so they stay sharp at any DPI
- Recoloured at apply time, so any scheme works in any colour
- Drop in a PNG, JPEG, WebP, BMP, GIF or APNG and get a real cursor with a hotspot picker
- **Blend** your own arrow over a catalog scheme for the other sixteen roles
- A watchdog puts your scheme back when Windows resets it
- One-click **Restore Windows Default**, and the uninstaller does it automatically

### What it does not do

It never draws a cursor itself — every pointer is a real cursor file handed to Windows, drawn by the GPU's hardware cursor plane. That is why it adds **zero input latency**.

It does not read or write machine-wide settings, install a driver or service, or inject code into any process. Applications that draw their own pointer (many games using raw input) are unaffected by design.

Collects nothing. No telemetry, no account. MIT licensed.
