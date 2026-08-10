# Privacy Policy

**Cursed** — last updated 2026-08-07.

## Cursed collects nothing

No analytics. No telemetry. No crash reporting. No account. No identifiers.
No profiling. Nothing about you or your computer is measured, stored remotely,
or transmitted.

## Network access

Cursed makes exactly one kind of network request, and only if you leave it
enabled:

- **Update check** — a request to the GitHub Releases API for this project, to
  see whether a newer version exists. It sends nothing but the request itself.

You can turn this off under **Settings → General → Check for updates
automatically**. With it off, Cursed makes no network requests at all and
works fully offline, including its fonts, artwork, and these documents.

## Where your data lives

Everything Cursed creates stays on your computer, in
`%APPDATA%\Cursed`:

| Path                            | Contents                                  |
| ------------------------------- | ----------------------------------------- |
| `settings.json`                 | Your preferences                          |
| `presets.json`                  | Your saved presets                        |
| `custom/`                       | Cursors built from your own images        |
| `cache/`                        | Rendered catalog cursors                  |
| `backup/original_scheme.json`   | Your pointer settings from before install |
| `logs/`                         | Debug logs, only if you enable them       |

You can open this folder at any time from **Settings → Advanced**, and delete
any of it. Imported images are decoded in memory and are never copied anywhere
except as the cursor files you explicitly build.

## Uninstalling

Uninstalling returns this PC to the state it was in before Cursed was
installed. In order:

1. Your original pointer scheme is restored from
   `backup/original_scheme.json`, before anything is deleted — so the restore
   still has the snapshot it needs, and no pointer is left naming a file that is
   about to be removed.
2. Every cursor scheme Cursed added is removed from the registry, along with
   the launch-on-sign-in entry.
3. The WebView2 data folder is deleted. This is
   `%LOCALAPPDATA%\dev.feylo.cursed` — it holds Cursed's window, not your data,
   and it is typically around 70 MB. Most apps built this way leave it behind.
4. `%APPDATA%\Cursed` is deleted.

Step 4 is the one you are asked about. The uninstaller offers to **keep your
presets and custom cursors**; the default is to remove them, because
uninstalling should mean the app is gone rather than leaving a folder you never
hear about again. Choosing to keep them leaves `%APPDATA%\Cursed` in place with
your presets and the cursors you built, minus the disposable cache and logs.

`scripts/verify-uninstall.ps1` in the repository checks all of the above and is
run before a release: it asserts every one of the seventeen pointer roles is
byte-identical to its pre-install value, and that no file, registry key, scheme
or shortcut remains.

## If this ever changes

If optional telemetry is ever added, it will be **opt-in only**, anonymous, and
documented here before the release that contains it.
