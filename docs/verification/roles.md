# The seventeen roles, checked

**Covers the programmatic half of §2.6 of the research brief.** Updated in place.

```bash
npm run check:roles
```

Reads every value under `HKCU\Control Panel\Cursors`, expands it, checks the
file is there, and reads its first bytes to see what it actually is.

## Why this comes before screenshotting a browser

The complaint is always shaped the same way: "the cursor doesn't change in
Firefox." The instinct is that the application is refusing to cooperate, and the
instinct is nearly always wrong.

Windows falls back to its own pointer for any role it cannot load, **silently**,
with nothing written to any log. A path it cannot resolve, a file an uninstaller
deleted, a `.cur` whose `idType` says icon, an `.ani` that is not a RIFF — every
one of those produces exactly the symptom of an application ignoring the scheme,
and none of them is that.

So the seventeen entries are checked first. Only once they are all loadable is
"this application does its own thing" a conclusion rather than a guess.

The header is read rather than trusted from the extension. A file named `.cur`
that is really a PNG exists perfectly, passes any check that only asks whether
it is there, and loads as nothing.

## 2026-08-16 — development machine, v1.20.0 + working tree

| | |
| --- | --- |
| Machine | Windows 11 Home, build 26200, x86_64 |
| Applied | an imported pack blended over `precision-gap-cross` |
| Roles set | **17 of 17** |
| Faults | **0** |

Every role resolved to a file that exists and carries a valid `.cur` header.
Eleven point into `imported\`, six into the rendered `cache\`. Nothing pointed
at a missing file and nothing pointed at something that was not a cursor.

On this machine, at this moment, a role that does not follow in an application
is that application's decision.

## What this does not cover

**The visual half of §2.6 is not done, and one part of it cannot be done the way
the brief describes.**

The brief asks for a screenshot of each cursor role in each of five browsers.
Windows draws the pointer on the hardware cursor plane — that is the whole
architectural premise of this app, and it is why added input latency is zero.
The consequence is that the pointer **is not in the framebuffer**, so an ordinary
screen capture does not contain it. `PrintWindow`, `BitBlt` and every
screenshot tool that uses them return a frame with no cursor in it.

Capturing it means compositing it in deliberately: `GetCursorInfo` for the
handle and the position, `DrawIconEx` to paint it into the captured bitmap. That
is a real thing to build, it is not a screenshot, and what it proves is that
`GetCursorInfo` returned the cursor we expected — which is a different and
weaker claim than "a person looking at Firefox saw our arrow".

The honest split:

| | Status |
| --- | --- |
| All 17 registry entries resolve and load | done, above |
| A composited capture harness (`GetCursorInfo` + `DrawIconEx`) | not built |
| Chrome, Edge, Brave, Opera GX, Firefox, by eye | **owner, five minutes each** |

The five-minute version is worth more than the harness: open each browser, hover
a link, hover text, hover a resizable border, and watch during a page load. Five
browsers, five roles, one person, no tooling. If one of them does not follow,
run `npm run check:roles` first — the answer is usually there.
