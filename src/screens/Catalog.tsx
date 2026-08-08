import { useEffect, useMemo, useRef, useState } from "react";
import { Search, Sparkles } from "lucide-react";
import { ScreenHeader } from "../components/ScreenHeader";
import { Button, Slider } from "../components/ui";
import * as ipc from "../lib/ipc";
import { CATEGORIES, type Category, type PackSummary } from "../lib/types";
import { useStore } from "../store";

/** Hovering must feel free, so the live preview waits for the pointer to settle. */
const HOVER_DEBOUNCE_MS = 120;

const SWATCHES = [
  "#2E8BFF",
  "#5CB8FF",
  "#8AE9FF",
  "#33D6A6",
  "#7DFF3D",
  "#FF7A2E",
  "#FF4D5E",
  "#FF3DD8",
  "#A24BFF",
  "#EDF1F7",
];

export function Catalog() {
  const packs = useStore((s) => s.packs);
  const settings = useStore((s) => s.settings);
  const go = useStore((s) => s.go);
  const setError = useStore((s) => s.setError);
  const setPreviewing = useStore((s) => s.setPreviewing);
  const refreshActive = useStore((s) => s.refreshActive);
  const patchSettings = useStore((s) => s.patchSettings);

  const [query, setQuery] = useState("");
  const [category, setCategory] = useState<Category | "ALL">("ALL");
  const [tint, setTint] = useState(settings.tint);
  const [size, setSize] = useState(settings.cursorSize ?? 32);
  const [applying, setApplying] = useState<string | null>(null);
  const [applied, setApplied] = useState<string | null>(null);

  const hoverTimer = useRef<number | null>(null);
  const previewedRef = useRef<string | null>(null);

  useEffect(() => {
    if (settings.cursorSize === null) {
      void ipc.getCursorBaseSize().then(setSize).catch(() => undefined);
    }
  }, [settings.cursorSize]);

  // A live preview must never outlive this screen: leaving with the system
  // cursor still overridden would look exactly like a bug.
  useEffect(
    () => () => {
      if (hoverTimer.current) window.clearTimeout(hoverTimer.current);
      if (previewedRef.current) {
        void ipc.clearPreview().catch(() => undefined);
        previewedRef.current = null;
      }
      setPreviewing(false);
    },
    [setPreviewing],
  );

  const visible = useMemo(() => {
    const needle = query.trim().toLowerCase();
    return packs.filter(
      (pack) =>
        (category === "ALL" || pack.category === category) &&
        (needle === "" ||
          pack.name.toLowerCase().includes(needle) ||
          pack.category.toLowerCase().includes(needle)),
    );
  }, [packs, query, category]);

  const cancelHover = () => {
    if (hoverTimer.current) {
      window.clearTimeout(hoverTimer.current);
      hoverTimer.current = null;
    }
  };

  const onHover = (pack: PackSummary) => {
    if (!ipc.isDesktop() || applying) return;
    cancelHover();
    hoverTimer.current = window.setTimeout(() => {
      setPreviewing(true);
      previewedRef.current = pack.id;
      void ipc
        .previewPack({
          packId: pack.id,
          tint,
          size,
          outline: settings.outline,
          applyMode: settings.applyMode,
        })
        .catch(() => undefined);
    }, HOVER_DEBOUNCE_MS);
  };

  const onLeave = () => {
    cancelHover();
    if (!previewedRef.current) return;
    previewedRef.current = null;
    setPreviewing(false);
    void ipc.clearPreview().catch(() => undefined);
  };

  const commit = async (pack: PackSummary) => {
    cancelHover();
    setApplying(pack.id);
    try {
      await ipc.applyPack({
        packId: pack.id,
        tint,
        size,
        outline: settings.outline,
        applyMode: settings.applyMode,
      });
      previewedRef.current = null;
      setApplied(pack.id);
      await refreshActive();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setApplying(null);
      setPreviewing(false);
    }
  };

  const done = async () => {
    await patchSettings({ tint, cursorSize: size });
    go("home");
  };

  return (
    <div className="screen-in flex h-full flex-col">
      <ScreenHeader title="CATALOG">
        <span className="mono text-[10px] text-text-dim">{visible.length}</span>
      </ScreenHeader>

      <div className="border-b border-border px-3 pb-2">
        <div className="relative">
          <Search
            size={13}
            className="pointer-events-none absolute top-1/2 left-2 -translate-y-1/2 text-text-dim"
          />
          <input
            value={query}
            onChange={(e) => setQuery(e.currentTarget.value)}
            placeholder="Search cursors"
            className="h-8 w-full rounded-xs border border-border bg-surface pr-2 pl-7 text-[12px] text-text outline-none transition-colors duration-150 placeholder:text-text-dim focus:border-accent"
          />
        </div>

        <div className="mt-2 flex gap-1 overflow-x-auto pb-1">
          {(["ALL", ...CATEGORIES] as const).map((item) => (
            <button
              key={item}
              type="button"
              onClick={() => setCategory(item)}
              className={`display shrink-0 rounded-full border px-2.5 py-1 text-[9px] transition-colors duration-150 ${
                category === item
                  ? "border-accent bg-accent-dim text-accent-hi"
                  : "border-border text-text-dim hover:border-border-hi hover:text-text-muted"
              }`}
            >
              {item}
            </button>
          ))}
        </div>
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto px-3 py-3" onMouseLeave={onLeave}>
        {visible.length === 0 ? (
          <EmptyState query={query} onReset={() => { setQuery(""); setCategory("ALL"); }} />
        ) : (
          <div className="grid grid-cols-3 gap-2">
            {visible.map((pack) => (
              <Tile
                key={pack.id}
                pack={pack}
                tint={tint}
                busy={applying === pack.id}
                active={applied === pack.id}
                onEnter={() => onHover(pack)}
                onLeave={onLeave}
                onClick={() => void commit(pack)}
              />
            ))}
          </div>
        )}
      </div>

      <div className="border-t border-border bg-bg/95 px-3 py-2 backdrop-blur">
        <div className="mb-2 flex items-center gap-1.5">
          {SWATCHES.map((swatch) => (
            <button
              key={swatch}
              type="button"
              aria-label={swatch}
              onClick={() => setTint(swatch)}
              style={{ background: swatch }}
              className={`h-4 w-4 rounded-full transition-transform duration-150 ${
                tint.toLowerCase() === swatch.toLowerCase()
                  ? "scale-125 ring-1 ring-text ring-offset-2 ring-offset-bg"
                  : "hover:scale-110"
              }`}
            />
          ))}
          <span className="mono ml-auto text-[10px] text-text-dim">{tint.toUpperCase()}</span>
        </div>

        <div className="mb-2">
          <Slider
            label="SIZE"
            suffix="px"
            min={32}
            max={256}
            step={8}
            value={size}
            onChange={setSize}
          />
        </div>

        <Button full onClick={() => void done()}>
          DONE
        </Button>
      </div>
    </div>
  );
}

function Tile({
  pack,
  tint,
  busy,
  active,
  onEnter,
  onLeave,
  onClick,
}: {
  pack: PackSummary;
  tint: string;
  busy: boolean;
  active: boolean;
  onEnter: () => void;
  onLeave: () => void;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onMouseEnter={onEnter}
      onMouseLeave={onLeave}
      onClick={onClick}
      title={pack.name}
      className={`group relative flex aspect-square flex-col items-center justify-center rounded-sm border bg-surface p-2 transition-all duration-150 ease-[cubic-bezier(0.16,1,0.3,1)] ${
        active
          ? "border-accent glow"
          : "border-border hover:-translate-y-px hover:border-border-hi hover:bg-elevated"
      }`}
    >
      {pack.animated && (
        <span className="absolute top-1.5 right-1.5 text-accent-hi" title="Animated">
          <Sparkles size={10} />
        </span>
      )}

      {/*
        The preview PNG is used as an alpha mask over a solid fill rather than
        drawn directly, so the grid tracks the swatch the user just picked
        without a render round-trip per tile. Drawing the PNG itself would show
        the pack's own default colour and quietly contradict the swatch row.
      */}
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
        className="block h-9 w-9 transition-transform duration-150 group-hover:scale-110"
      />

      <span className="display mt-2 truncate text-[8px] text-text-dim transition-colors duration-150 group-hover:text-text">
        {pack.name}
      </span>

      {busy && <span className="absolute inset-x-2 bottom-1.5 h-px shimmer" />}
    </button>
  );
}

function EmptyState({ query, onReset }: { query: string; onReset: () => void }) {
  return (
    <div className="flex h-full flex-col items-center justify-center gap-3 px-6 text-center">
      <p className="text-[12px] text-text-muted">
        Nothing matches <span className="mono text-text">{query || "that filter"}</span>.
      </p>
      <p className="text-[11px] text-text-dim">
        Try <span className="display text-text-muted">PRECISION</span> for crosshairs, or
        drop your own PNG under CUSTOM.
      </p>
      <Button variant="ghost" onClick={onReset}>
        SHOW EVERYTHING
      </Button>
    </div>
  );
}
