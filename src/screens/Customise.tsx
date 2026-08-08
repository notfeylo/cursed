import { useEffect, useState } from "react";
import { Check } from "lucide-react";
import { ScreenHeader } from "../components/ScreenHeader";
import { Button, Card, Select, Slider, TextInput, Toggle } from "../components/ui";
import * as ipc from "../lib/ipc";
import type { ApplyMode } from "../lib/types";
import { useStore } from "../store";

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

/**
 * One cursor, with room to work on it.
 *
 * The catalog used to carry the colour swatches and size slider along the
 * bottom, which meant browsing and tweaking fought for the same cramped strip.
 * Choosing a cursor now opens here, where the preview is big enough to judge and
 * every control has space.
 */
export function Customise() {
  const pack = useStore((s) => s.selected);
  const settings = useStore((s) => s.settings);
  const packs = useStore((s) => s.packs);
  const go = useStore((s) => s.go);
  const setError = useStore((s) => s.setError);
  const refreshActive = useStore((s) => s.refreshActive);
  const patchSettings = useStore((s) => s.patchSettings);

  const [tint, setTint] = useState(settings.tint);
  const [size, setSize] = useState(settings.cursorSize ?? 32);
  const [outline, setOutline] = useState(settings.outline);
  const [mode, setMode] = useState<ApplyMode>(settings.applyMode);
  const [busy, setBusy] = useState(false);
  const [done, setDone] = useState(false);

  useEffect(() => {
    if (settings.cursorSize === null) {
      void ipc.getCursorBaseSize().then(setSize).catch(() => undefined);
    }
  }, [settings.cursorSize]);

  if (!pack) {
    return (
      <div className="screen-in flex h-full flex-col">
        <ScreenHeader title="CUSTOMISE" back="catalog" />
        <div className="grid flex-1 place-items-center px-6 text-center text-[12px] text-text-dim">
          Pick a cursor from the catalog first.
        </div>
      </div>
    );
  }

  // Imported artwork is somebody's finished image. Recolouring it would flatten
  // it to a silhouette, so the colour controls simply do not apply.
  const recolourable = pack.recolorable;

  const apply = async () => {
    setBusy(true);
    setDone(false);
    try {
      await ipc.applyPack({
        packId: pack.id,
        tint,
        size,
        outline,
        applyMode: mode,
      });
      await patchSettings({ tint, cursorSize: size, outline, applyMode: mode });
      await refreshActive();
      setDone(true);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="screen-in flex h-full flex-col">
      <ScreenHeader title={pack.name} back="catalog" />

      <div className="min-h-0 flex-1 overflow-y-auto px-3 py-3">
        {/* Big enough to actually judge the shape before committing to it. */}
        <div className="circuit relative mb-3 grid h-40 place-items-center overflow-hidden rounded-sm border border-border bg-surface">
          <div
            className="pointer-events-none absolute inset-0 opacity-70"
            style={{
              background:
                "radial-gradient(ellipse 60% 80% at 50% 40%, var(--accent-glow), transparent 70%)",
            }}
          />
          {recolourable ? (
            <span
              role="img"
              aria-label={pack.name}
              style={{
                WebkitMaskImage: `url("${pack.preview}")`,
                maskImage: `url("${pack.preview}")`,
                WebkitMaskSize: "contain",
                maskSize: "contain",
                WebkitMaskPosition: "center",
                maskPosition: "center",
                WebkitMaskRepeat: "no-repeat",
                maskRepeat: "no-repeat",
                background: tint,
              }}
              className="relative block h-24 w-24"
            />
          ) : (
            <img
              src={pack.preview}
              alt={pack.name}
              draggable={false}
              className="relative h-24 w-24 object-contain"
            />
          )}
          <span className="mono absolute right-2 bottom-2 text-[10px] text-text-dim">
            {pack.category}
          </span>
        </div>

        {recolourable ? (
          <>
            <span className="display mb-1 block text-[10px] text-text-dim">COLOUR</span>
            <Card>
              <div className="mb-2 flex flex-wrap items-center gap-1.5">
                {SWATCHES.map((swatch) => (
                  <button
                    key={swatch}
                    type="button"
                    aria-label={swatch}
                    onClick={() => setTint(swatch)}
                    style={{ background: swatch }}
                    className={`h-5 w-5 rounded-full transition-transform duration-150 ${
                      tint.toLowerCase() === swatch.toLowerCase()
                        ? "scale-125 ring-1 ring-text ring-offset-2 ring-offset-surface"
                        : "hover:scale-110"
                    }`}
                  />
                ))}
              </div>
              <TextInput mono value={tint} maxLength={7} onChange={setTint} placeholder="#2E8BFF" />
            </Card>
          </>
        ) : (
          <Card>
            <p className="text-[11px] text-text-muted">
              This is an imported cursor, so it keeps its own colours. Size still applies.
            </p>
          </Card>
        )}

        <span className="display mt-3 mb-1 block text-[10px] text-text-dim">SIZE & SHAPE</span>
        <Card>
          <Slider
            label="CURSOR SIZE"
            suffix="px"
            min={32}
            max={256}
            step={8}
            value={size}
            onChange={setSize}
          />
          <div className="mt-1 flex flex-wrap gap-1">
            {[32, 48, 64, 96, 128].map((preset) => (
              <button
                key={preset}
                type="button"
                onClick={() => setSize(preset)}
                className={`mono rounded-xs border px-2 py-0.5 text-[10px] transition-colors duration-150 ${
                  size === preset
                    ? "border-accent text-accent-hi"
                    : "border-border text-text-dim hover:border-border-hi hover:text-text"
                }`}
              >
                {preset}
              </button>
            ))}
          </div>

          {recolourable && (
            <Toggle
              checked={outline}
              onChange={setOutline}
              label="Contrast outline"
              hint="One dark pixel around the edge, so it stays visible on white"
            />
          )}

          <div className="pt-1">
            <span className="mb-1 block text-[11px] text-text-muted">Apply to</span>
            <Select<ApplyMode>
              value={mode}
              onChange={setMode}
              options={[
                { value: "All", label: "All 17 pointer roles" },
                { value: "Recommended", label: "Arrow + link + precision" },
                { value: "ArrowOnly", label: "Arrow only" },
              ]}
            />
          </div>
        </Card>

        {packs.length > 0 && (
          <p className="mt-3 text-[11px] text-text-dim">
            Changes here are saved as your defaults the moment you apply.
          </p>
        )}
      </div>

      <div className="border-t border-border px-3 py-2">
        <Button full onClick={() => void apply()} disabled={busy}>
          {busy ? "APPLYING" : done ? "APPLIED" : "APPLY THIS CURSOR"}
          {done && !busy && <Check size={13} />}
        </Button>
        {busy && <div className="mt-2 h-px w-full shimmer" />}
        {done && !busy && (
          <button
            type="button"
            onClick={() => go("home")}
            className="display mt-2 w-full py-1 text-[10px] text-text-dim hover:text-text"
          >
            DONE
          </button>
        )}
      </div>
    </div>
  );
}
