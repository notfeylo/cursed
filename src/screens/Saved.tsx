import { useState } from "react";
import { Check, Copy, Download, Pencil, Star, Trash2, Upload } from "lucide-react";
import { open, save } from "@tauri-apps/plugin-dialog";
import { ScreenHeader } from "../components/ScreenHeader";
import { Button, TextInput } from "../components/ui";
import * as ipc from "../lib/ipc";
import type { Preset } from "../lib/types";
import { useStore } from "../store";

export function Saved() {
  const presets = useStore((s) => s.presets);
  const packs = useStore((s) => s.packs);
  const active = useStore((s) => s.active);
  const settings = useStore((s) => s.settings);
  const setError = useStore((s) => s.setError);
  const refreshPresets = useStore((s) => s.refreshPresets);
  const refreshActive = useStore((s) => s.refreshActive);

  const [renaming, setRenaming] = useState<string | null>(null);
  const [draftName, setDraftName] = useState("");
  const [busy, setBusy] = useState(false);

  const guard = async (work: () => Promise<unknown>) => {
    setBusy(true);
    try {
      await work();
      await refreshPresets();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const newFromCurrent = () =>
    guard(async () => {
      const base = active.packId ?? packs[0]?.id;
      if (!base) throw new Error("Apply a cursor first, then save it as a preset.");
      await ipc.savePreset({
        id: crypto.randomUUID(),
        name: active.packName ?? "PRESET",
        created: new Date().toISOString(),
        basePack: base,
        overrides: {},
        hoverStyle: settings.hoverStyle,
        tint: active.tint,
        size: active.size,
        outline: settings.outline,
        hotkey: null,
        isDefault: presets.length === 0,
      });
    });

  const importPack = () =>
    guard(async () => {
      const picked = await open({
        multiple: false,
        filters: [{ name: "Cursed pack", extensions: ["cfpack"] }],
      });
      if (typeof picked === "string") await ipc.importCfpack(picked);
    });

  const exportPack = (preset: Preset) =>
    guard(async () => {
      const destination = await save({
        defaultPath: `${preset.name.toLowerCase().replace(/\s+/g, "-")}.cfpack`,
        filters: [{ name: "Cursed pack", extensions: ["cfpack"] }],
      });
      if (destination) await ipc.exportPreset(preset.id, destination);
    });

  const previewFor = (preset: Preset) =>
    packs.find((pack) => pack.id === preset.basePack)?.preview ?? "";

  return (
    <div className="screen-in flex h-full flex-col">
      <ScreenHeader title="SAVED">
        <button
          type="button"
          onClick={() => void importPack()}
          title="Import a .cfpack"
          className="grid h-7 w-7 place-items-center rounded-xs text-text-dim hover:bg-elevated hover:text-text"
        >
          <Upload size={14} />
        </button>
      </ScreenHeader>

      <div className="min-h-0 flex-1 overflow-y-auto px-3 py-3">
        <div className="grid grid-cols-2 gap-2">
          <button
            type="button"
            onClick={() => void newFromCurrent()}
            disabled={busy}
            className="flex aspect-[4/3] flex-col items-center justify-center gap-1 rounded-sm border border-dashed border-border-hi text-text-dim transition-colors duration-150 hover:border-accent hover:text-text"
          >
            <span className="text-lg leading-none">+</span>
            <span className="display text-[10px]">NEW FROM CURRENT</span>
          </button>

          {presets.map((preset) => (
            <div
              key={preset.id}
              className={`relative flex aspect-[4/3] flex-col justify-between rounded-sm border bg-surface p-2 ${
                preset.isDefault ? "border-accent" : "border-border"
              }`}
            >
              {preset.isDefault && (
                <span className="absolute top-0 left-0 h-full w-0.5 rounded-l-sm bg-accent" />
              )}

              <div className="flex items-start gap-2">
                {previewFor(preset) && (
                  // Masked fill, not a drawn image — the thumbnail has to show
                  // the preset's own color, not the base pack's default.
                  <span
                    role="img"
                    aria-label={preset.name}
                    style={{
                      WebkitMaskImage: `url("${previewFor(preset)}")`,
                      maskImage: `url("${previewFor(preset)}")`,
                      WebkitMaskSize: "contain",
                      maskSize: "contain",
                      WebkitMaskPosition: "center",
                      maskPosition: "center",
                      WebkitMaskRepeat: "no-repeat",
                      maskRepeat: "no-repeat",
                      background: preset.tint,
                    }}
                    className="block h-6 w-6 shrink-0"
                  />
                )}
                <div className="min-w-0 flex-1">
                  {renaming === preset.id ? (
                    <TextInput
                      value={draftName}
                      onChange={setDraftName}
                      maxLength={48}
                      placeholder="Name"
                    />
                  ) : (
                    <span className="display block truncate text-[10px] text-text">
                      {preset.name}
                    </span>
                  )}
                  <span className="mono block text-[10px] text-text-dim">
                    {preset.size}px · {preset.tint.toUpperCase()}
                  </span>
                  {preset.hotkey && (
                    <span className="mono block text-[10px] text-accent-hi">
                      {preset.hotkey}
                    </span>
                  )}
                </div>
              </div>

              <div className="flex items-center gap-0.5">
                {renaming === preset.id ? (
                  <IconAction
                    label="Save name"
                    onClick={() =>
                      void guard(async () => {
                        await ipc.savePreset({ ...preset, name: draftName.trim() || preset.name });
                        setRenaming(null);
                      })
                    }
                  >
                    <Check size={12} />
                  </IconAction>
                ) : (
                  <>
                    <button
                      type="button"
                      onClick={() =>
                        void guard(async () => {
                          await ipc.applyPreset(preset.id);
                          await refreshActive();
                        })
                      }
                      className="display mr-auto rounded-xs border border-border px-2 py-1 text-[10px] text-text-muted hover:border-accent hover:text-accent-hi"
                    >
                      APPLY
                    </button>
                    <IconAction
                      label="Set as default"
                      onClick={() => void guard(() => ipc.setDefaultPreset(preset.id))}
                    >
                      <Star size={12} />
                    </IconAction>
                    <IconAction
                      label="Rename"
                      onClick={() => {
                        setRenaming(preset.id);
                        setDraftName(preset.name);
                      }}
                    >
                      <Pencil size={12} />
                    </IconAction>
                    <IconAction
                      label="Duplicate"
                      onClick={() => void guard(() => ipc.duplicatePreset(preset.id))}
                    >
                      <Copy size={12} />
                    </IconAction>
                    <IconAction label="Export" onClick={() => void exportPack(preset)}>
                      <Download size={12} />
                    </IconAction>
                    <IconAction
                      label="Delete"
                      danger
                      onClick={() => void guard(() => ipc.deletePreset(preset.id))}
                    >
                      <Trash2 size={12} />
                    </IconAction>
                  </>
                )}
              </div>
            </div>
          ))}
        </div>

        {presets.length === 0 && (
          <p className="mt-4 text-center text-[11px] text-text-dim">
            A preset stores the whole pointer — pack, color, size and outline — so you can
            switch back to it with one click or a hotkey.
          </p>
        )}
      </div>

      <div className="border-t border-border px-3 py-2">
        <Button full variant="ghost" onClick={() => void importPack()}>
          IMPORT .CFPACK
        </Button>
      </div>
    </div>
  );
}

function IconAction({
  children,
  label,
  onClick,
  danger,
}: {
  children: React.ReactNode;
  label: string;
  onClick: () => void;
  danger?: boolean;
}) {
  return (
    <button
      type="button"
      title={label}
      aria-label={label}
      onClick={onClick}
      className={`grid h-6 w-6 place-items-center rounded-xs transition-colors duration-150 ${
        danger
          ? "text-text-dim hover:bg-danger/15 hover:text-danger"
          : "text-text-dim hover:bg-elevated hover:text-text"
      }`}
    >
      {children}
    </button>
  );
}
