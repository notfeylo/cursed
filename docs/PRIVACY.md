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

The uninstaller restores your original pointer scheme from
`backup/original_scheme.json` before removing anything.

## If this ever changes

If optional telemetry is ever added, it will be **opt-in only**, anonymous, and
documented here before the release that contains it.
