# `docs/`

Everything worth writing down that is not code.

| Document | What it answers |
| --- | --- |
| [`REPO_MAP.md`](REPO_MAP.md) | **Start here.** Where everything lives, one screen, for someone opening the repository cold. |
| [`ARCHITECTURE.md`](ARCHITECTURE.md) | *How* the app works: the three cursor layers, the watchdog, and the Win32 behaviour that cost real debugging time. |
| [`CURSOR_FORMAT.md`](CURSOR_FORMAT.md) | The `.cur` and `.ani` byte layouts, including the fields Windows reuses for something other than their names — the hotspot lives in `wPlanes`/`wBitCount`, and `biHeight` is doubled. |
| [`LICENSES.md`](LICENSES.md) | What every bundled pack and font is licensed under, including the thirty-four packs that state no licence and ship anyway, and why. |
| [`PRIVACY.md`](PRIVACY.md) | What leaves the machine. One request, to GitHub, when checking for an update. |
| [`TERMS.md`](TERMS.md) | The terms shown in the app. |
| [`verification/`](verification/) | One record per release: what was checked, what was not, and what could not be. |
| [`PRD.md`](PRD.md) | The original brief, as written before the first commit. History, not instruction — it is here because the code cites its section numbers, and several of its decisions were deliberately overruled. |

## On the verification records

They exist to make the difference between *checked*, *not checked* and *could
not be checked* survive past the day of the release. A release note says what
changed; a record says what is actually known to work, and — more usefully —
what is not. They are written whether or not the release ships, and an
unrunnable check is never recorded as a passing one.

[`verification/README.md`](verification/README.md) holds the standing gate that
runs before a tag.
