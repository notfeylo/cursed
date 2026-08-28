import { useEffect, useMemo, useRef, useState } from "react";
import { Search, Sparkles } from "lucide-react";
import { ScreenHeader } from "../components/ScreenHeader";
import { Button } from "../components/ui";
import * as ipc from "../lib/ipc";
import { CATEGORIES, type Category, type PackSummary } from "../lib/types";
import { useStore } from "../store";

/** Hovering must feel free, so the live preview waits for the pointer to settle. */
const HOVER_DEBOUNCE_MS = 120;

/**
 * Browsing only.
 *
 * The color swatches and size slider used to live along the bottom of this
 * screen, competing with the grid for a few cramped pixels. They now have a
 * screen of their own, reached by choosing a cursor — so this one does the one
 * job it is good at: showing you what there is.
 */
export function Catalog() {
  const packs = useStore((s) => s.packs);
  const settings = useStore((s) => s.settings);
  const select = useStore((s) => s.select);
  const setPreviewing = useStore((s) => s.setPreviewing);
  const patchSettings = useStore((s) => s.patchSettings);

  const [query, setQuery] = useState("");
  const [category, setCategory] = useState<Category | "ALL">("ALL");
  const [tintPreviews, setTintPreviews] = useState(settings.tintPreviews);

  const tint = settings.tint;
  const size = settings.cursorSize ?? 32;

  const hoverTimer = useRef<number | null>(null);
  const previewedRef = useRef<string | null>(null);

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
    if (!ipc.isDesktop()) return;
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
          hoverStyle: settings.hoverStyle,
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

  /**
   * Choosing a cursor opens it for customizing rather than applying it here.
   *
   * Browsing and tweaking were sharing one cramped strip along the bottom, so
   * neither had room. Picking is now a decision to look closer, not a commitment.
   */
  const choose = (pack: PackSummary) => {
    cancelHover();
    if (previewedRef.current) {
      previewedRef.current = null;
      void ipc.clearPreview().catch(() => undefined);
    }
    setPreviewing(false);
    select(pack);
  };

  return (
    <div className="screen-in flex h-full flex-col">
      <ScreenHeader title="CATALOG">
        <span className="mono text-[10px] text-text-dim">{visible.length}</span>
      </ScreenHeader>

      <div className="border-b border-border px-3 py-2">
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
              className={`display shrink-0 rounded-full border px-2.5 py-1 text-[10px] transition-colors duration-150 ${
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
          <EmptyState
            query={query}
            empty={packs.length === 0}
            onReset={() => {
              setQuery("");
              setCategory("ALL");
            }}
          />
        ) : (
          <div className="grid grid-cols-3 gap-2">
            {visible.map((pack) => (
              <Tile
                key={pack.id}
                pack={pack}
                tint={tint}
                tinted={tintPreviews}
                onEnter={() => onHover(pack)}
                onLeave={onLeave}
                onClick={() => choose(pack)}
              />
            ))}
          </div>
        )}
      </div>

      <div className="border-t border-border bg-bg/95 px-3 py-2 backdrop-blur">
        {/* Color is an option, not the default view. With it off you see each
            cursor as it actually looks, which is the only way to tell two
            hundred of them apart. */}
        <button
          type="button"
          onClick={() => {
            const next = !tintPreviews;
            setTintPreviews(next);
            void patchSettings({ tintPreviews: next });
          }}
          className={`display mb-2 flex w-full items-center justify-between rounded-xs border px-2 py-1.5 text-[10px] transition-colors duration-150 ${
            tintPreviews
              ? "border-accent bg-accent-dim text-accent-hi"
              : "border-border text-text-dim hover:border-border-hi hover:text-text-muted"
          }`}
        >
          <span>{tintPreviews ? "SHOWING TINTED" : "SHOWING TRUE COLORS"}</span>
          <span className="text-text-dim">
            {tintPreviews ? "TAP FOR TRUE COLORS" : "TAP TO TINT"}
          </span>
        </button>

        <p className="text-center text-[11px] text-text-dim">
          Pick one to set its color and size.
        </p>
      </div>
    </div>
  );
}

function Tile({
  pack,
  tint,
  tinted,
  onEnter,
  onLeave,
  onClick,
}: {
  pack: PackSummary;
  tint: string;
  /** Recolor this tile to the tint, rather than showing its own colors. */
  tinted: boolean;
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
      className="group relative flex aspect-square flex-col items-center justify-center rounded-sm border border-border bg-surface p-2 tile hover:tile-hover hover:bg-elevated"
    >
      {pack.animated && (
        <span className="absolute top-1.5 right-1.5 text-accent-hi" title="Animated">
          <Sparkles size={10} />
        </span>
      )}

      {pack.recolorable && tinted ? (
        /*
          Our own artwork is grayscale, so the preview is used as an alpha mask
          over a solid fill: the grid tracks the swatch the user just picked
          with no render round-trip per tile. Drawing the PNG itself would show
          the pack's default color and quietly contradict the swatch row.
        */
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
      ) : (
        /*
          An imported cursor is somebody's finished, full-color artwork. Masking
          it to the accent color would flatten it to a silhouette and throw away
          the very thing that makes it worth importing, so it is drawn as-is.
        */
        <img
          src={pack.preview}
          alt={pack.name}
          draggable={false}
          className="h-10 w-10 object-contain transition-transform duration-150 group-hover:scale-110"
        />
      )}

      <span className="display mt-2 w-full truncate px-0.5 text-center text-[10px] text-text-dim transition-colors duration-150 group-hover:text-text">
        {pack.name}
      </span>

    </button>
  );
}

function EmptyState({
  query,
  empty,
  onReset,
}: {
  query: string;
  /** Nothing has been imported at all, as opposed to nothing matching a filter. */
  empty: boolean;
  onReset: () => void;
}) {
  const go = useStore((s) => s.go);

  // An empty catalog and an over-filtered one look identical but need opposite
  // advice, so they say different things.
  if (empty) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-3 px-6 text-center">
        <p className="text-[12px] text-text">No cursors yet.</p>
        <p className="text-[11px] text-text-dim">
          Import a folder of cursors and they will appear here, exactly as they look.
        </p>
        <Button onClick={() => go("settings")}>IMPORT A FOLDER</Button>
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col items-center justify-center gap-3 px-6 text-center">
      <p className="text-[12px] text-text-muted">
        Nothing matches <span className="mono text-text">{query || "that filter"}</span>.
      </p>
      <Button variant="ghost" onClick={onReset}>
        SHOW EVERYTHING
      </Button>
    </div>
  );
}
