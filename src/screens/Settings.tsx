import { useEffect, useRef, useState } from "react";
import { CURSOR_SIZES } from "../lib/sizes";
import { FolderOpen, FolderPlus, Info, Trash2 } from "lucide-react";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import { ScreenHeader } from "../components/ScreenHeader";
import { UpdatePanel } from "../components/UpdatePanel";
import { ColorPicker } from "../components/ColorPicker";
import { HotkeyField } from "../components/HotkeyField";
import {
  Button,
  Card,
  Field,
  SectionTitle,
  Select,
  Slider,
  Toggle,
} from "../components/ui";
import * as ipc from "../lib/ipc";
import type { ApplyMode } from "../lib/types";
import { useStore } from "../store";

/** A starting point, not the whole choice — the gradient above them is. */
const SWATCHES = [
  "#EDF1F7",
  "#2E8BFF",
  "#5CB8FF",
  "#8AE9FF",
  "#33D6A6",
  "#7DFF3D",
  "#FFD23D",
  "#FF7A2E",
  "#FF4D5E",
  "#FF3DD8",
  "#A24BFF",
  "#8A94A6",
];
import type { ApplyMode as _ApplyMode } from "../lib/types";

export function SettingsScreen() {
  const settings = useStore((s) => s.settings);
  const patch = useStore((s) => s.patchSettings);
  const go = useStore((s) => s.go);
  const setError = useStore((s) => s.setError);
  const refreshActive = useStore((s) => s.refreshActive);

  const bootstrap = useStore((s) => s.bootstrap);
  const packs = useStore((s) => s.packs);

  const [storageDir, setStorageDir] = useState("");
  const [systemSize, setSystemSize] = useState(32);
  const [confirmRestore, setConfirmRestore] = useState(false);

  /* The accent color, held locally while it is being dragged.
   *
   * Every pointer move through the gradient is a new color, and saving each one
   * would be a settings write and a re-render of the cursor per frame. The
   * picker shows the color immediately; the save follows once the hand stops. */
  const [tintDraft, setTintDraft] = useState(settings.tint);
  const tintTimer = useRef<number | undefined>(undefined);

  useEffect(() => {
    setTintDraft(settings.tint);
  }, [settings.tint]);

  useEffect(() => () => window.clearTimeout(tintTimer.current), []);

  const setTint = (hex: string) => {
    setTintDraft(hex);
    window.clearTimeout(tintTimer.current);
    tintTimer.current = window.setTimeout(() => void patch({ tint: hex }), 180);
  };

  const [imported, setImported] = useState<ipc.ImportedPack[]>([]);
  const [importing, setImporting] = useState(false);
  const [importMsg, setImportMsg] = useState<string | null>(null);
  const [schemeStatus, setSchemeStatus] = useState<ipc.SchemeStatus | null>(null);
  const [backupBusy, setBackupBusy] = useState(false);
  const [backupMsg, setBackupMsg] = useState<string | null>(null);

  const refreshImported = () => {
    void ipc.listImported().then(setImported).catch(() => undefined);
  };

  useEffect(() => {
    if (!ipc.isDesktop()) return;
    void ipc.getStorageDir().then(setStorageDir).catch(() => undefined);
    void ipc.getCursorBaseSize().then(setSystemSize).catch(() => undefined);
    void ipc.getSchemeStatus().then(setSchemeStatus).catch(() => undefined);
    refreshImported();
  }, []);

  /** Shown once, to the users an earlier version's update bug took something from. */
  const originalLost = schemeStatus?.originalLost === true;
  const showSchemeLoss = originalLost && schemeStatus?.acknowledged === false;

  const dismissSchemeLoss = () => {
    setSchemeStatus((s) => (s ? { ...s, acknowledged: true } : s));
    // Optimistic: the banner is gone from the screen either way, and a failed
    // write means it comes back on the next launch, which is the harmless
    // direction to fail in.
    void ipc.acknowledgeSchemeLoss().catch(() => undefined);
  };

  const importFolder = async () => {
    const folder = await openDialog({ directory: true, multiple: false });
    if (typeof folder !== "string") return;

    setImporting(true);
    setImportMsg(null);
    try {
      const report = await ipc.importCursorFolder(folder);
      setImportMsg(
        report.imported === 0
          ? "No cursors found in that folder."
          : `Imported ${report.imported} cursor${report.imported === 1 ? "" : "s"}` +
              (report.skipped ? `, skipped ${report.skipped}.` : ".") +
              (report.problems.length ? ` ${report.problems[0]}` : ""),
      );
      refreshImported();
      // The catalog list is loaded once at startup, so it has to be refetched
      // or the new cursors would not appear until the next launch.
      await bootstrap();
    } catch (e) {
      setImportMsg(e instanceof Error ? e.message : String(e));
    } finally {
      setImporting(false);
    }
  };

  const backUp = async () => {
    const suggested = await ipc.suggestedBackupName().catch(() => "cursed-backup.zip");
    const dest = await saveDialog({
      defaultPath: suggested,
      filters: [{ name: "Backup", extensions: ["zip"] }],
    });
    if (typeof dest !== "string") return;

    setBackupBusy(true);
    setBackupMsg(null);
    try {
      const report = await ipc.exportAllData(dest);
      setBackupMsg(
        `Saved ${report.files} file${report.files === 1 ? "" : "s"} ` +
          `(${formatBytes(report.bytes)}) to ${report.path}.`,
      );
    } catch (e) {
      setBackupMsg(e instanceof Error ? e.message : String(e));
    } finally {
      setBackupBusy(false);
    }
  };

  const restoreData = async () => {
    const src = await openDialog({
      multiple: false,
      filters: [{ name: "Backup", extensions: ["zip"] }],
    });
    if (typeof src !== "string") return;

    setBackupBusy(true);
    setBackupMsg(null);
    try {
      const report = await ipc.importAllData(src);
      setBackupMsg(
        `Restored ${report.files} file${report.files === 1 ? "" : "s"}` +
          (report.skipped ? `, skipped ${report.skipped}.` : ".") +
          (report.problems.length ? ` ${report.problems[0]}` : "") +
          " Restart Cursed for all of it to take effect.",
      );
      // The parts that can be re-read without a restart, are.
      await bootstrap();
      await refreshActive();
    } catch (e) {
      setBackupMsg(e instanceof Error ? e.message : String(e));
    } finally {
      setBackupBusy(false);
    }
  };

  const restore = async () => {
    try {
      await ipc.restoreWindowsDefault();
      await refreshActive();
      setConfirmRestore(false);
      go("home");
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  return (
    <div className="screen-in flex h-full flex-col">
      <ScreenHeader title="SETTINGS">
        <button
          type="button"
          onClick={() => go("about")}
          title="About"
          className="grid h-7 w-7 place-items-center rounded-xs text-text-dim hover:bg-elevated hover:text-text"
        >
          <Info size={14} />
        </button>
      </ScreenHeader>

      <div className="min-h-0 flex-1 overflow-y-auto px-3 py-4">
        {showSchemeLoss && (
          <div className="mt-3 rounded-xs border border-danger/50 bg-danger/5 p-3">
            <p className="text-[11px] font-medium text-danger">
              Your original pointer scheme was lost
            </p>
            <p className="mt-1 text-[11px] text-text-muted">
              Updating Cursed used to run the previous version's uninstaller, which deleted{" "}
              everything in its data folder — including the record of what your pointers looked
              like before Cursed was installed. That record cannot be rebuilt: once a Cursed
              cursor is applied, the pointers it replaced are no longer anywhere on the machine
              to read back.
            </p>
            <p className="mt-1 text-[11px] text-text-muted">
              Restore Windows default still works and still gives you a normal Windows pointer.
              It just cannot give you back a pointer you had customized before Cursed. This was
              our bug, it is fixed in this version, and it will not happen again.
            </p>
            <div className="mt-2 flex justify-end">
              <Button variant="ghost" onClick={dismissSchemeLoss}>
                UNDERSTOOD
              </Button>
            </div>
          </div>
        )}

        <SectionTitle>Updates</SectionTitle>
        <UpdatePanel autoCheck />

        <SectionTitle>My cursors</SectionTitle>
        <Card>
          <p className="mb-2 text-[11px] text-text-muted">
            Point Cursed at a folder of cursors you already have.{" "}
            <span className="mono">.cur</span>, <span className="mono">.ani</span>,{" "}
            <span className="mono">.png</span> and <span className="mono">.zip</span> all work,
            and files named <span className="mono">Name--cursor</span> /{" "}
            <span className="mono">Name--pointer</span> are paired automatically.
          </p>

          <Button full onClick={() => void importFolder()} disabled={importing}>
            <FolderPlus size={13} />
            {importing ? "IMPORTING" : "IMPORT A FOLDER"}
          </Button>
          {importing && <div className="mt-2 h-px w-full shimmer" />}
          {importMsg && <p className="mt-2 text-[11px] text-text-muted">{importMsg}</p>}

          {imported.length > 0 && (
            <>
              <div className="mt-3 flex items-center justify-between">
                <span className="display text-[10px] text-text-dim">
                  {imported.length} IMPORTED
                </span>
                <button
                  type="button"
                  onClick={() =>
                    void ipc
                      .deleteAllImported()
                      .then(() => {
                        refreshImported();
                        setImportMsg(null);
                        return bootstrap();
                      })
                      .catch(() => undefined)
                  }
                  className="text-[11px] text-danger hover:underline"
                >
                  Remove all
                </button>
              </div>
              <div className="mt-1 max-h-40 space-y-0.5 overflow-y-auto">
                {imported.map((pack) => (
                  <div
                    key={pack.id}
                    className="flex items-center gap-2 rounded-xs px-1 py-1 hover:bg-elevated"
                  >
                    <span className="min-w-0 flex-1 truncate text-[11px] text-text-muted">
                      {pack.name}
                    </span>
                    <span className="display shrink-0 text-[10px] text-text-dim">
                      {pack.category}
                    </span>
                    <button
                      type="button"
                      aria-label={`Remove ${pack.name}`}
                      onClick={() =>
                        void ipc
                          .deleteImported(pack.id)
                          .then(() => {
                            refreshImported();
                            return bootstrap();
                          })
                          .catch(() => undefined)
                      }
                      className="shrink-0 text-text-dim hover:text-danger"
                    >
                      <Trash2 size={11} />
                    </button>
                  </div>
                ))}
              </div>
            </>
          )}

          <p className="mt-2 text-[11px] text-text-dim">
            Imported cursors stay on this machine. They are never uploaded, and never bundled
            into Cursed itself.
          </p>
        </Card>

        <SectionTitle>General</SectionTitle>
        <Card>
          <Toggle
            checked={settings.launchOnStartup}
            onChange={(v) => void patch({ launchOnStartup: v })}
            label="Launch on Windows startup"
            hint="Starts silently, straight to the tray"
          />
          <Toggle
            checked={settings.startMinimized}
            onChange={(v) => void patch({ startMinimized: v })}
            label="Start minimized to tray"
          />
          <Toggle
            checked={settings.closeToTray}
            onChange={(v) => void patch({ closeToTray: v })}
            label="Close button minimizes to tray"
            hint="Quit from the tray menu to exit fully"
          />
          <Toggle
            checked={settings.showTrayIcon}
            onChange={(v) => void patch({ showTrayIcon: v })}
            label="Show tray icon"
          />
          <Toggle
            checked={settings.autoCheckUpdates}
            onChange={(v) => void patch({ autoCheckUpdates: v })}
            label="Check for updates automatically"
            hint="The only network request Cursed ever makes"
          />
        </Card>

        <SectionTitle>Cursor</SectionTitle>
        <Card>
          <div className="pb-2">
            <Slider
              label="POINTER SIZE"
              suffix="px"
              min={10}
              max={128}
              steps={CURSOR_SIZES}
              value={settings.cursorSize ?? systemSize}
              onChange={(v) => void patch({ cursorSize: v })}
            />
            <p className="mt-1 text-[11px] text-text-dim">
              {settings.cursorSize === null
                ? `Matching the Windows setting, which is ${systemSize} pixels. Move the slider to choose your own.`
                : `${settings.cursorSize} pixels across. The normal Windows size is 32.`}
            </p>
            {settings.cursorSize !== null && (
              <button
                type="button"
                onClick={() => void patch({ cursorSize: null })}
                className="mt-1 text-[11px] text-accent-hi hover:underline"
              >
                Go back to matching Windows
              </button>
            )}

            {/* Said plainly, because otherwise a hand that did not grow reads as
                the slider being broken rather than as a choice — and the switch
                sits directly underneath, so the answer is where the question
                gets asked. */}
            {(settings.cursorSize ?? systemSize) > 32 && (
              <p className="mt-2 text-[11px] leading-relaxed text-text-dim">
                {settings.scaleAllRoles
                  ? "The hand and the text cursor grow with the pointer. At this size the text cursor can be hard to place between two letters."
                  : "The hand and the text cursor stay at 32 pixels. A big pointer is easy to follow; a big text cursor is hard to place between two letters."}
              </p>
            )}

            <div className="mt-3">
              <Toggle
                checked={settings.scaleAllRoles}
                onChange={(v) => void patch({ scaleAllRoles: v })}
                label="Resize the hand and text cursor as well"
                hint="The hand appears over links; the text cursor is the thin bar you click text with"
              />
            </div>
          </div>

          <Field
            label="Accent color"
            hint="Used for the pointer, the link hand and the text caret"
          >
            <ColorPicker value={tintDraft} onChange={setTint} swatches={SWATCHES} />
          </Field>

          <Toggle
            checked={settings.outline}
            onChange={(v) => void patch({ outline: v })}
            label="Contrast outline"
            hint="One dark pixel around the artwork, at every size"
          />

          <Field label="Apply to">
            <Select<ApplyMode>
              value={settings.applyMode}
              onChange={(v) => void patch({ applyMode: v })}
              options={[
                { value: "Blend", label: "Blend — custom arrow over a pack" },
                { value: "ArrowOnly", label: "Arrow only" },
                { value: "Recommended", label: "Recommended (3 roles)" },
                { value: "All", label: "All 17 roles" },
              ]}
            />
          </Field>

          {/* The pack that fills the sixteen roles a custom cursor does not
              define. It was settable nowhere: the import screen has its own
              dropdown that only ever set local state, so this value stayed on
              its default for every user, for every build, while still deciding
              what fifteen of their pointers looked like. */}
          {settings.applyMode === "Blend" && (
            <Field
              label="Blend with"
              hint="Fills the roles a custom cursor does not define"
            >
              <Select
                value={settings.blendPack}
                onChange={(v) => void patch({ blendPack: v })}
                options={packs.map((pack) => ({ value: pack.id, label: pack.name }))}
              />
            </Field>
          )}

          <div className="py-2">
            <Slider
              label="ANIMATION SPEED"
              suffix="×"
              min={0.5}
              max={2}
              step={0.1}
              value={settings.animationSpeed}
              onChange={(v) => void patch({ animationSpeed: v })}
            />
            {/* Said out loud because it is otherwise a control that appears to
                do nothing: the speed is baked into an .ani when it is written,
                so moving this cannot change a cursor that already exists. */}
            <p className="mt-2 text-[11px] text-text-dim">
              Applies to animated cursors built from here on. Cursors you have already made
              keep the speed they were built with.
            </p>
          </div>

          <Toggle
            checked={settings.reapplyOnResume}
            onChange={(v) => void patch({ reapplyOnResume: v })}
            label="Re-apply on resume from sleep"
          />
        </Card>

        <SectionTitle>Protection</SectionTitle>
        <Card>
          <Toggle
            checked={settings.watchdogEnabled}
            onChange={(v) => void patch({ watchdogEnabled: v })}
            label="Protect my cursor"
            hint="Puts the scheme back if Windows resets it"
          />
          <Field label="Watchdog interval">
            <Select
              value={String(settings.watchdogIntervalSecs)}
              onChange={(v) => void patch({ watchdogIntervalSecs: Number(v) })}
              options={[3, 5, 10, 30].map((n) => ({ value: String(n), label: `${n} seconds` }))}
            />
          </Field>
          <Toggle
            checked={settings.reapplyAfterThemeChange}
            onChange={(v) => void patch({ reapplyAfterThemeChange: v })}
            label="Re-apply after theme change"
          />
        </Card>

        <SectionTitle>Hotkeys</SectionTitle>
        <Card>
          <p className="pb-2 text-[11px] text-text-muted">
            Click a shortcut and press the keys you want. These work anywhere in
            Windows, not just while Cursed is open.
          </p>
          <Field label="Switch between your cursor and the Windows default">
            <HotkeyField
              label="Switch between your cursor and the Windows default"
              value={settings.hotkeyToggle}
              onChange={(v) => void patch({ hotkeyToggle: v })}
            />
          </Field>
          <Field label="Open Cursed">
            <HotkeyField
              label="Open Cursed"
              value={settings.hotkeyOpen}
              onChange={(v) => void patch({ hotkeyOpen: v })}
            />
          </Field>
          <Field
            label="Saved cursors"
            hint="One shortcut per saved cursor, in the order they appear under SAVED"
          >
            <div className="space-y-1">
              {settings.hotkeyPresets.map((accelerator, index) => (
                <div key={index} className="flex items-center gap-2">
                  <span className="mono w-4 shrink-0 text-[11px] text-text-dim">
                    {index + 1}
                  </span>
                  <div className="min-w-0 flex-1">
                    <HotkeyField
                      label={`Saved cursor ${index + 1}`}
                      value={accelerator}
                      onChange={(v) => {
                        const next = [...settings.hotkeyPresets];
                        next[index] = v;
                        void patch({ hotkeyPresets: next });
                      }}
                    />
                  </div>
                </div>
              ))}
            </div>
          </Field>
        </Card>

        <SectionTitle>Advanced</SectionTitle>
        <Card>
          <Field label="Storage location">
            <div className="flex items-center gap-2">
              <span className="mono min-w-0 flex-1 truncate text-[11px] text-text-dim">
                {storageDir || "%APPDATA%\\Cursed"}
              </span>
              <button
                type="button"
                onClick={() => void ipc.openStorageDir().catch(() => undefined)}
                title="Open folder"
                className="grid h-7 w-7 shrink-0 place-items-center rounded-xs border border-border text-text-dim hover:border-border-hi hover:text-text"
              >
                <FolderOpen size={13} />
              </button>
            </div>
          </Field>

          <Field label="Everything you have made">
            <p className="mb-2 text-[11px] text-text-muted">
              One zip with your settings, presets, custom cursors, imported packs and the
              record of your original Windows pointers. Restoring writes those files back and
              leaves anything newer alone.
            </p>
            <div className="grid grid-cols-2 gap-2">
              <Button onClick={() => void backUp()} disabled={backupBusy}>
                {backupBusy ? "WORKING" : "BACK UP"}
              </Button>
              <Button variant="ghost" onClick={() => void restoreData()} disabled={backupBusy}>
                RESTORE
              </Button>
            </div>
            {backupMsg && <p className="mt-2 text-[11px] text-text-muted">{backupMsg}</p>}
          </Field>

          <Toggle
            checked={settings.debugLogging}
            onChange={(v) => void patch({ debugLogging: v })}
            label="Enable debug logging"
            hint="Local rotating file; takes effect on next launch"
          />


          <div className="pt-3">
            {confirmRestore ? (
              <div className="space-y-2">
                <p className="text-[11px] text-danger">
                  {originalLost
                    ? "This puts every pointer back to the Windows default. The record of what " +
                      "your pointers were before Cursed was lost to a bug in an earlier " +
                      "version, so that is what restore can give back. Your presets are kept."
                    : "This puts every pointer back exactly as it was before Cursed was " +
                      "installed. Your presets are kept."}
                </p>
                <div className="grid grid-cols-2 gap-2">
                  <Button variant="ghost" onClick={() => setConfirmRestore(false)}>
                    CANCEL
                  </Button>
                  <Button variant="danger" onClick={() => void restore()}>
                    RESTORE
                  </Button>
                </div>
              </div>
            ) : (
              <Button full variant="danger" onClick={() => setConfirmRestore(true)}>
                RESTORE WINDOWS DEFAULT
              </Button>
            )}
          </div>
        </Card>
      </div>
    </div>
  );
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}
