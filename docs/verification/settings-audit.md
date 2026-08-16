# The settings audit

**Covers §2.8 of the research brief.** Read against the code on 2026-08-16, at
v1.20.0 plus the working tree.

Four questions per control, and a control that fails one is a control that lies:

1. **What does it change?**
2. **Does it take effect immediately**, or only later, or never?
3. **Does it survive a restart?**
4. **Can the user see that it worked?**

Everything below is traced from `src/screens/Settings.tsx` through
`commands::save_settings` to whatever actually reads the value.

## General

| Control | Changes | Immediate | Persists | Visible |
| --- | --- | --- | --- | --- |
| Launch on Windows startup | the `Run` registry value, via `autostart::apply` | yes | yes | the toggle; and the app starts on next sign-in |
| Start minimized to tray | whether an **autostarted** launch shows a window | next launch | yes | only on next autostart — see below |
| Close button minimizes to tray | whether the X hides or quits | yes | yes | pressing X |
| Show tray icon | `tray::set_visible` | yes | yes | the icon appears or goes |
| Check for updates automatically | the background check at startup and every six hours | yes | yes | the update panel |

**"Start minimized to tray" is honest but easily misread.** It only governs a
launch that came from autostart: a launch the user asked for shows the window
whatever this says, because they just double-clicked the icon. That is the right
behaviour and the label does not say it. Worth rewording; not a lie.

## Cursor

| Control | Changes | Immediate | Persists | Visible |
| --- | --- | --- | --- | --- |
| Cursor size | the pixel size the scheme is rendered at | **yes** — `save_settings` rebuilds when it moves | yes | the pointer changes under the hand |
| Use Windows' size | clears the override, back to `CursorBaseSize` | yes | yes | as above |
| Resize the hand and I-beam too | whether the size control covers those two roles | **yes** — in `appearance_changed` on purpose | yes | the hand changes on hover |
| Accent / tint colour | the colour the scheme is rendered in | **yes** | yes | the pointer changes |
| Contrast outline | the dark keyline at every size | **yes** | yes | the pointer changes |
| Apply to | which of the 17 roles a future apply covers | next apply | yes | the roles change on the next apply |
| **Blend with** | the pack filling roles a custom cursor does not define | next apply | yes | **added this pass — see below** |
| Animation speed | the frame timing baked into an `.ani` when it is written | **no, and it cannot be** | yes | now stated on the control |
| Re-apply on resume from sleep | the watchdog's power-broadcast trigger | yes, via `propagate` | yes | only when it fires |

### Two that failed, and what was done

**"Blend with" was settable nowhere.** `settings.blendPack` decides what fills
the sixteen roles a custom cursor does not define — so on a Blend apply it
decides what fifteen of the user's pointers look like. There was no control for
it in Settings. The import screen has a dropdown that looks like the one, and it
only ever set React state local to that screen; nothing wrote back. The stored
value therefore stayed on its default, `precision-gap-cross`, for every user
since it was introduced, while quietly deciding most of their scheme.

Fixed rather than deleted: a Select in the Cursor group, shown when the apply
mode is Blend. Deleting a setting that is doing real work because nothing could
change it would be the wrong half to remove.

**Animation speed appears to do nothing.** The value is baked into the `.ani`
when the file is written, so moving the slider cannot change a cursor that
already exists — and the user sees a control move, a setting save, and nothing
happen. Not fixable without rebuilding every animated cursor on every change,
which is a lot of work for a slider drag. The control now says what it does:
"Applies to animated cursors built from here on."

That is the honest fix. A control that explains its scope is fine; a control
that silently has none is not.

## Protection

| Control | Changes | Immediate | Persists | Visible |
| --- | --- | --- | --- | --- |
| Protect my cursor | whether the watchdog re-applies on drift | yes, via `propagate` | yes | only when something else changes the scheme |
| Watchdog interval | the poll period, clamped 3–30 s | yes | yes | not directly visible |
| Re-apply after theme change | the `WM_SETTINGCHANGE` trigger | yes | yes | only when it fires |

Three controls whose effect is by definition invisible until something goes
wrong. That is inherent to a watchdog and not a fault; the diagnostics report is
where their state can be read.

## Hotkeys

| Control | Changes | Immediate | Persists | Visible |
| --- | --- | --- | --- | --- |
| Toggle custom ↔ Windows default | a global shortcut, re-registered on save | yes | yes | pressing it |
| Open Cursed | as above | yes | yes | pressing it |
| Preset slots 1–5 | five global shortcuts | yes | yes | pressing them |

`hotkeys::register` runs on every save, so a rebind takes effect without a
restart. A combination Windows refuses — because another app holds it — is
reported by the register call.

## Advanced

| Control | Changes | Immediate | Persists | Visible |
| --- | --- | --- | --- | --- |
| Storage location | nothing; it displays a path and opens the folder | n/a | n/a | Explorer opens |
| **Everything you have made** (back up / restore) | writes or reads a zip | yes | n/a | a line saying what happened |
| Generated cursor cache | deletes `cache\` | yes | n/a | the size drops to 0 B |
| Enable debug logging | the log level, **next launch** | no | yes | stated on the control |
| Diagnostics | nothing; produces a report | n/a | n/a | the report appears |
| Restore Windows default | rewrites all 17 registry values | yes | n/a | the pointer changes |

"Enable debug logging" already says "takes effect on next launch". The log level
is fixed when the logger is built during startup, so this is accurate rather
than a limitation being papered over.

## Grouping and confirmation

Already true, and re-checked rather than assumed:

- Five groups by intent — General, Cursor, Protection, Hotkeys, Advanced — each
  a `SectionTitle` over a `Card`.
- Every non-obvious control carries one line of helper text.
- The destructive controls are visually distinct: **Restore Windows default**
  and **Remove all** are `variant="danger"`, and Restore takes a second click
  through an explicit confirm with the consequence spelled out.
- Restore's copy is now conditional: on a machine whose original scheme was lost
  to the update bug, it says so rather than promising to put back something that
  no longer exists.

## Not done

**Per-group reset.** The brief asks for one and there is none. It is a small
feature and a genuinely useful one — "put the Cursor group back how it was" is
the request behind most of the fiddling — but it needs a per-group definition of
"default" that does not exist in the code today, and inventing one badly means a
reset button that changes something the user did not think was in that group.
