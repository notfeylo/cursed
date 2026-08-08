import { useCallback, useEffect, useRef, useState } from "react";
import { Plus } from "lucide-react";
import { open } from "@tauri-apps/plugin-dialog";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { ScreenHeader } from "../components/ScreenHeader";
import { Button, Select, TextInput, Toggle } from "../components/ui";
import * as ipc from "../lib/ipc";
import type { ApplyMode, ImportedImage } from "../lib/types";
import { useStore } from "../store";

type Preview = { size: number; dataUri: string };

const HOTSPOT_PRESETS = [
  { label: "Alpha centroid", value: "centroid" },
  { label: "Tip detect", value: "tip" },
  { label: "Center", value: "center" },
  { label: "Top-left", value: "topleft" },
] as const;

export function CustomImport() {
  const settings = useStore((s) => s.settings);
  const packs = useStore((s) => s.packs);
  const go = useStore((s) => s.go);
  const setError = useStore((s) => s.setError);
  const refreshActive = useStore((s) => s.refreshActive);

  const [image, setImage] = useState<ImportedImage | null>(null);
  const [previews, setPreviews] = useState<Preview[]>([]);
  const [hotspot, setHotspot] = useState<[number, number]>([0.5, 0.5]);
  const [name, setName] = useState("");
  const [outline, setOutline] = useState(settings.outline);
  const [mode, setMode] = useState<ApplyMode>(settings.applyMode);
  const [blendPack, setBlendPack] = useState(settings.tint ? "precision-gap-cross" : "");
  const [busy, setBusy] = useState(false);

  const accept = useCallback(
    async (loader: () => Promise<ImportedImage>) => {
      setBusy(true);
      try {
        const staged = await loader();
        setImage(staged);
        setHotspot(staged.suggestedHotspot);
        setPreviews(await ipc.previewCustom(staged.token, outline));
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e));
      } finally {
        setBusy(false);
      }
    },
    [outline, setError],
  );

  // Real drag-and-drop is a native event, not a DOM one — the webview never
  // sees the file, so the bytes never cross the IPC boundary unvalidated.
  useEffect(() => {
    if (!ipc.isDesktop()) return;
    let unlisten: (() => void) | undefined;
    void getCurrentWebview()
      .onDragDropEvent((event) => {
        if (event.payload.type !== "drop") return;
        const [first] = event.payload.paths;
        if (first) void accept(() => ipc.importImage(first));
      })
      .then((fn) => {
        unlisten = fn;
      });
    return () => unlisten?.();
  }, [accept]);

  const browse = async () => {
    if (!ipc.isDesktop()) return;
    const picked = await open({
      multiple: false,
      filters: [{ name: "Images", extensions: ["png", "jpg", "jpeg", "webp", "bmp", "gif"] }],
    });
    if (typeof picked === "string") void accept(() => ipc.importImage(picked));
  };

  const refreshPreviews = async (nextOutline: boolean) => {
    setOutline(nextOutline);
    if (!image) return;
    try {
      setPreviews(await ipc.previewCustom(image.token, nextOutline));
    } catch {
      /* the ladder is cosmetic; a failed refresh is not worth a banner */
    }
  };

  const buildAndApply = async () => {
    if (!image) return;
    setBusy(true);
    try {
      const built = await ipc.buildCustomCursor({
        token: image.token,
        name: name.trim() || "MY CURSOR",
        hotspot,
        outline,
        animationSpeed: settings.animationSpeed,
      });
      await ipc.applyCustomCursor({
        cursorId: built.id,
        applyMode: mode,
        blendPackId: mode === "Blend" ? blendPack || packs[0]?.id || null : null,
        tint: settings.tint,
        size: settings.cursorSize ?? 32,
        outline,
      });
      await refreshActive();
      go("home");
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="screen-in flex h-full flex-col">
      <ScreenHeader title="CUSTOM">
        {image && (
          <button
            type="button"
            onClick={() => {
              setImage(null);
              setPreviews([]);
            }}
            className="display text-[9px] text-text-dim hover:text-text"
          >
            CLEAR
          </button>
        )}
      </ScreenHeader>

      {!image ? (
        <DropZone busy={busy} onBrowse={() => void browse()} />
      ) : (
        <div className="min-h-0 flex-1 overflow-y-auto px-3 py-3">
          <div className="grid grid-cols-2 gap-2">
            <HotspotPicker image={image} hotspot={hotspot} onChange={setHotspot} />
            <PreviewLadder previews={previews} />
          </div>

          <div className="mt-2 flex flex-wrap gap-1">
            {HOTSPOT_PRESETS.map((preset) => (
              <button
                key={preset.value}
                type="button"
                onClick={() => setHotspot(presetHotspot(preset.value, image))}
                className="display rounded-full border border-border px-2 py-1 text-[9px] text-text-dim transition-colors duration-150 hover:border-border-hi hover:text-text"
              >
                {preset.label}
              </button>
            ))}
            <span className="mono ml-auto self-center text-[10px] text-text-dim">
              {hotspot[0].toFixed(2)} · {hotspot[1].toFixed(2)}
            </span>
          </div>

          <div className="mt-3 space-y-1">
            <TextInput value={name} onChange={setName} placeholder="Name this cursor" maxLength={48} />

            <Toggle
              checked={outline}
              onChange={(next) => void refreshPreviews(next)}
              label="Add contrast outline"
              hint="Keeps a dark cursor visible on a white page"
            />

            <div className="pt-1">
              <span className="mb-1 block text-[11px] text-text-muted">Apply to</span>
              <Select<ApplyMode>
                value={mode}
                onChange={setMode}
                options={[
                  { value: "Blend", label: "Blend — my arrow + a catalog pack" },
                  { value: "ArrowOnly", label: "Just the arrow" },
                  { value: "Recommended", label: "Arrow + link + precision" },
                  { value: "All", label: "All 17 roles (same image everywhere)" },
                ]}
              />
            </div>

            {mode === "Blend" && (
              <div className="pt-1">
                <span className="mb-1 block text-[11px] text-text-muted">
                  Pack for the other 16 roles
                </span>
                <Select
                  value={blendPack}
                  onChange={setBlendPack}
                  options={packs.map((pack) => ({ value: pack.id, label: pack.name }))}
                />
              </div>
            )}

            {mode === "All" && (
              <p className="pt-1 text-[11px] text-danger">
                Every role becomes this image — including the text caret and the busy
                pointer. Blend usually looks better.
              </p>
            )}

            {image.animated && (
              <p className="pt-1 text-[11px] text-text-dim">
                {image.frameCount} frames · built as an animated cursor at all eight sizes.
              </p>
            )}
          </div>

          <div className="mt-3 pb-1">
            <Button full onClick={() => void buildAndApply()} disabled={busy}>
              {busy ? "BUILDING" : "BUILD & APPLY"}
            </Button>
            {busy && <div className="mt-2 h-px w-full shimmer" />}
          </div>
        </div>
      )}
    </div>
  );
}

function presetHotspot(
  preset: (typeof HOTSPOT_PRESETS)[number]["value"],
  image: ImportedImage,
): [number, number] {
  switch (preset) {
    case "center":
      return [0.5, 0.5];
    case "topleft":
      return [0, 0];
    case "tip":
      return [0, 0];
    case "centroid":
    default:
      return image.suggestedHotspot;
  }
}

function DropZone({ busy, onBrowse }: { busy: boolean; onBrowse: () => void }) {
  return (
    <div className="flex flex-1 items-center justify-center p-4">
      <button
        type="button"
        onClick={onBrowse}
        className="flex h-full w-full flex-col items-center justify-center gap-3 rounded-sm border border-dashed border-border-hi bg-surface/40 transition-colors duration-150 hover:border-accent hover:bg-elevated/40"
      >
        <span className="grid h-12 w-12 place-items-center rounded-full border border-border-hi text-text-muted">
          <Plus size={20} />
        </span>
        <span className="text-[12px] text-text-muted">
          {busy ? "Reading image…" : "Drop a PNG — or click to browse"}
        </span>
        <span className="text-[11px] text-text-dim">
          PNG · JPEG · WebP · BMP · GIF — up to 20 MB
        </span>
      </button>
    </div>
  );
}

/**
 * The hotspot is stored normalised, so dragging the crosshair here means the
 * same thing at 32 px and at 256 px.
 */
function HotspotPicker({
  image,
  hotspot,
  onChange,
}: {
  image: ImportedImage;
  hotspot: [number, number];
  onChange: (next: [number, number]) => void;
}) {
  const boxRef = useRef<HTMLDivElement | null>(null);
  const [dragging, setDragging] = useState(false);

  // The listener effect must not depend on `onChange`, or every pointermove
  // would tear down and re-attach the listeners mid-drag. A ref keeps the
  // callback current without making it a dependency.
  const onChangeRef = useRef(onChange);
  onChangeRef.current = onChange;

  const set = useCallback((clientX: number, clientY: number) => {
    const box = boxRef.current?.getBoundingClientRect();
    if (!box || box.width === 0 || box.height === 0) return;
    onChangeRef.current([
      Math.min(1, Math.max(0, (clientX - box.left) / box.width)),
      Math.min(1, Math.max(0, (clientY - box.top) / box.height)),
    ]);
  }, []);

  useEffect(() => {
    if (!dragging) return;
    const move = (e: PointerEvent) => set(e.clientX, e.clientY);
    const up = () => setDragging(false);
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
    return () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
    };
  }, [dragging, set]);

  return (
    <div
      ref={boxRef}
      onPointerDown={(e) => {
        setDragging(true);
        set(e.clientX, e.clientY);
      }}
      className="relative aspect-square overflow-hidden rounded-sm border border-border bg-[repeating-conic-gradient(#131722_0_25%,#0B0D12_0_50%)] bg-[length:12px_12px]"
    >
      <img
        src={image.dataUri}
        alt=""
        draggable={false}
        className="h-full w-full object-contain [image-rendering:pixelated]"
      />
      <span
        className="pointer-events-none absolute h-px w-full bg-accent/70"
        style={{ top: `${hotspot[1] * 100}%` }}
      />
      <span
        className="pointer-events-none absolute h-full w-px bg-accent/70"
        style={{ left: `${hotspot[0] * 100}%` }}
      />
      <span
        className="pointer-events-none absolute h-2.5 w-2.5 -translate-x-1/2 -translate-y-1/2 rounded-full border border-white bg-accent"
        style={{ left: `${hotspot[0] * 100}%`, top: `${hotspot[1] * 100}%` }}
      />
    </div>
  );
}

/** The eight sizes at 1:1, so what you see is exactly what gets installed. */
function PreviewLadder({ previews }: { previews: Preview[] }) {
  return (
    <div className="rounded-sm border border-border bg-surface p-2">
      <span className="display mb-2 block text-[9px] text-text-dim">ACTUAL SIZE</span>
      <div className="flex flex-wrap items-end gap-2">
        {previews.length === 0
          ? [32, 48, 64].map((size) => (
              <div key={size} className="h-8 w-8 rounded-xs bg-elevated shimmer" />
            ))
          : previews.map((preview) => (
              <img
                key={preview.size}
                src={preview.dataUri}
                alt={`${preview.size} pixels`}
                title={`${preview.size}px`}
                width={Math.min(preview.size, 64)}
                height={Math.min(preview.size, 64)}
                className="shrink-0"
              />
            ))}
      </div>
    </div>
  );
}
