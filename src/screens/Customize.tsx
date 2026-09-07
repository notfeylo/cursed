import { useEffect, useState } from "react";
import { CURSOR_SIZES } from "../lib/sizes";
import { Check, Sparkles } from "lucide-react";
import { ScreenHeader } from "../components/ScreenHeader";
import { Button, Card, Select, Slider, Toggle } from "../components/ui";
import { ColorPicker } from "../components/ColorPicker";
import { useAnimatedPreview } from "../lib/useAnimatedPreview";
import * as ipc from "../lib/ipc";
import type { ApplyMode, HoverStyle } from "../lib/types";
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
 * The catalog used to carry the color swatches and size slider along the
 * bottom, which meant browsing and tweaking fought for the same cramped strip.
 * Choosing a cursor now opens here, where the preview is big enough to judge and
 * every control has space.
 */
export function Customize() {
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
  // `Blend` is normalised to `All` on the way in.
  //
  // Blend exists for a custom image — one drawing over a base pack's other
  // sixteen roles — so this screen does not offer it, and `roles_for` already
  // treats the two as identical for a catalog pack. But it is the *default*
  // setting, so the select below was being handed a value none of its options
  // carried, and a native `<select>` in that state renders blank. The first
  // thing anyone saw under "Apply to" was an empty box.
  const [mode, setMode] = useState<ApplyMode>(
    settings.applyMode === "Blend" ? "All" : settings.applyMode,
  );
  const [hover, setHover] = useState<HoverStyle>(settings.hoverStyle);
  const [busy, setBusy] = useState(false);
  const [done, setDone] = useState(false);

  useEffect(() => {
    if (settings.cursorSize === null) {
      void ipc.getCursorBaseSize().then(setSize).catch(() => undefined);
    }
  }, [settings.cursorSize]);

  // The preview moves if the cursor does.
  //
  // `pack.preview` is one still — the catalog hands back a single frame per pack
  // because it draws a hundred and thirty at once. Half of them are animations,
  // and this is the screen where somebody decides whether they want one, so it
  // asks for the whole thing.
  const frames = useAnimatedPreview(pack?.id ?? null);
  const frame = frames.current ?? pack?.preview ?? "";

  if (!pack) {
    return (
      <div className="screen-in flex h-full flex-col">
        <ScreenHeader title="CUSTOMIZE" back="catalog" />
        <div className="grid flex-1 place-items-center px-6 text-center text-[12px] text-text-dim">
          Pick a cursor from the catalog first.
        </div>
      </div>
    );
  }

  // Imported artwork is somebody's finished image. Recoloring it would flatten
  // it to a silhouette, so the color controls simply do not apply.
  const recolorable = pack.recolorable;

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
        hoverStyle: hover,
      });
      await patchSettings({
        tint,
        cursorSize: size,
        outline,
        applyMode: mode,
        hoverStyle: hover,
      });
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
          {recolorable ? (
            <span
              role="img"
              aria-label={pack.name}
              style={{
                WebkitMaskImage: `url("${frame}")`,
                maskImage: `url("${frame}")`,
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
              src={frame}
              alt={pack.name}
              draggable={false}
              className="relative h-24 w-24 object-contain [image-rendering:pixelated]"
            />
          )}
          <span className="mono absolute right-2 bottom-2 text-[10px] text-text-dim">
            {pack.category}
          </span>
          {/* Said out loud, because a loop can reach a frame that looks like a
              still and "is this one animated?" is the question this screen is
              here to answer. */}
          {frames.animated && (
            <span className="display absolute top-2 right-2 flex items-center gap-1 text-[10px] text-accent-hi">
              <Sparkles size={10} />
              ANIMATED
            </span>
          )}
        </div>

        {recolorable ? (
          <>
            <span className="display mb-1 block text-[10px] text-text-dim">COLOR</span>
            <Card>
              <ColorPicker value={tint} onChange={setTint} swatches={SWATCHES} />
            </Card>
          </>
        ) : (
          <Card>
            <p className="text-[11px] text-text-muted">
              This is an imported cursor, so it keeps its own colors. Size still applies.
            </p>
          </Card>
        )}

        <span className="display mt-3 mb-1 block text-[10px] text-text-dim">
          HOVERING A LINK
        </span>
        <Card>
          <p className="mb-2 text-[11px] text-text-muted">
            Windows shows a different pointer over links and buttons. Many cursors
            come with their own, which is not always the one you want.
          </p>
          <Select<HoverStyle>
            value={hover}
            onChange={setHover}
            options={[
              { value: "Pack", label: "Use this cursor's own hover" },
              { value: "Pointer", label: "Keep my pointer — don't change it" },
              { value: "Mark", label: "Use the Cursed mark" },
            ]}
          />
          <p className="mt-2 text-[11px] text-text-dim">
            {hover === "Pack"
              ? "Whatever artwork this cursor was made with."
              : hover === "Pointer"
                ? "Nothing changes when you hover a link. The pointer you picked stays on screen."
                : "The Cursed mark, in the color you chose above."}
          </p>
        </Card>

        <span className="display mt-3 mb-1 block text-[10px] text-text-dim">SIZE & SHAPE</span>
        <Card>
          <Slider
            label="CURSOR SIZE"
            suffix="px"
            min={10}
            max={128}
            steps={CURSOR_SIZES}
            value={size}
            onChange={setSize}
          />
          <div className="mt-1 flex flex-wrap gap-1">
            {CURSOR_SIZES.map((preset) => (
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

          {recolorable && (
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
