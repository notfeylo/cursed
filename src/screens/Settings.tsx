import { useEffect, useState } from "react";
import { FolderOpen, FolderPlus, Info, Trash2 } from "lucide-react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { ScreenHeader } from "../components/ScreenHeader";
import { UpdatePanel } from "../components/UpdatePanel";
import {
  Button,
  Card,
  Field,
  SectionTitle,
  Select,
  Slider,
  TextInput,
  Toggle,
} from "../components/ui";
import * as ipc from "../lib/ipc";
import type { ApplyMode } from "../lib/types";
import { useStore } from "../store";
import type { ApplyMode as _ApplyMode } from "../lib/types";

export function SettingsScreen() {
  const settings = useStore((s) => s.settings);
  const patch = useStore((s) => s.patchSettings);
  const go = useStore((s) => s.go);
  const setError = useStore((s) => s.setError);
  const refreshActive = useStore((s) => s.refreshActive);

  const bootstrap = useStore((s) => s.bootstrap);

  const [storageDir, setStorageDir] = useState("");
  const [cacheBytes, setCacheBytes] = useState(0);
  const [systemSize, setSystemSize] = useState(32);
  const [confirmRestore, setConfirmRestore] = useState(false);

  const [imported, setImported] = useState<ipc.ImportedPack[]>([]);
  const [importing, setImporting] = useState(false);
  const [importMsg, setImportMsg] = useState<string | null>(null);

  const refreshImported = () => {
    void ipc.listImported().then(setImported).catch(() => undefined);
  };

  useEffect(() => {
    if (!ipc.isDesktop()) return;
    void ipc.getStorageDir().then(setStorageDir).catch(() => undefined);
    void ipc.getCacheSize().then(setCacheBytes).catch(() => undefined);
    void ipc.getCursorBaseSize().then(setSystemSize).catch(() => undefined);
    refreshImported();
  }, []);

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

      <div className="min-h-0 flex-1 overflow-y-auto px-3 pb-4">
        <SectionTitle>Updates</SectionTitle>
        <UpdatePanel autoCheck />

        <SectionTitle>My cursors</SectionTitle>
        <Card>
          <p className="mb-2 text-[11px] text-text-muted">
            Point CursorForge at a folder of cursors you already have.{" "}
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
                    <span className="display shrink-0 text-[9px] text-text-dim">
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
            into CursorForge itself.
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
            hint="The only network request CursorForge ever makes"
          />
        </Card>

        <SectionTitle>Cursor</SectionTitle>
        <Card>
          <div className="pb-2">
            <Slider
              label="CURSOR SIZE"
              suffix="px"
              min={32}
              max={256}
              step={8}
              value={settings.cursorSize ?? systemSize}
              onChange={(v) => void patch({ cursorSize: v })}
            />
            {settings.cursorSize === null ? (
              <p className="mt-1 text-[11px] text-text-dim">
                Following Windows ({systemSize}px). Move the slider to override.
              </p>
            ) : (
              <button
                type="button"
                onClick={() => void patch({ cursorSize: null })}
                className="mt-1 text-[11px] text-accent-hi hover:underline"
              >
                Follow the Windows size again
              </button>
            )}
          </div>

          <Field label="Accent / tint colour">
            <div className="flex items-center gap-2">
              <span
                className="h-8 w-8 shrink-0 rounded-xs border border-border"
                style={{ background: settings.tint }}
              />
              <TextInput
                mono
                value={settings.tint}
                maxLength={7}
                onChange={(v) => void patch({ tint: v })}
                placeholder="#2E8BFF"
              />
            </div>
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
          <Field label="Toggle custom ↔ Windows default">
            <TextInput
              mono
              value={settings.hotkeyToggle}
              onChange={(v) => void patch({ hotkeyToggle: v })}
            />
          </Field>
          <Field label="Open CursorForge">
            <TextInput
              mono
              value={settings.hotkeyOpen}
              onChange={(v) => void patch({ hotkeyOpen: v })}
            />
          </Field>
          <div className="grid grid-cols-5 gap-1 pt-1">
            {settings.hotkeyPresets.map((accelerator, index) => (
              <TextInput
                key={index}
                mono
                value={accelerator}
                onChange={(v) => {
                  const next = [...settings.hotkeyPresets];
                  next[index] = v;
                  void patch({ hotkeyPresets: next });
                }}
              />
            ))}
          </div>
          <p className="pt-1 text-[11px] text-text-dim">
            Preset slots 1–5, in the order they appear under SAVED.
          </p>
        </Card>

        <SectionTitle>Advanced</SectionTitle>
        <Card>
          <Field label="Storage location">
            <div className="flex items-center gap-2">
              <span className="mono min-w-0 flex-1 truncate text-[11px] text-text-dim">
                {storageDir || "%APPDATA%\\CursorForge"}
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

          <Field label="Generated cursor cache">
            <div className="flex items-center gap-2">
              <span className="mono flex-1 text-[11px] text-text-dim">
                {formatBytes(cacheBytes)}
              </span>
              <Button
                variant="ghost"
                onClick={() =>
                  void ipc
                    .clearCache()
                    .then(() => ipc.getCacheSize())
                    .then(setCacheBytes)
                    .catch(() => undefined)
                }
              >
                CLEAR
              </Button>
            </div>
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
                  This puts every pointer back exactly as it was before CursorForge was
                  installed. Your presets are kept.
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
