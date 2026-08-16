import { useCallback, useEffect, useRef, useState } from "react";
import { Plus } from "lucide-react";
import { open } from "@tauri-apps/plugin-dialog";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { ScreenHeader } from "../components/ScreenHeader";
import { Button, Select, Slider, TextInput, Toggle } from "../components/ui";
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
  // Whether to cut the background out even when the file already claims
  // transparency. Off by default: re-cutting art somebody already cut loses a
  // soft edge, and `auto` already handles the ordinary case.
  const [forceCut, setForceCut] = useState(false);
  // Kept so the toggle can re-stage the same image rather than asking the user
  // to drop it again — the cut happens at stage time, because the preview has
  // to show what will actually be built.
  const [lastSource, setLastSource] = useState<string | null>(null);
  // The optional second image, used for the link/hover cursor. Staged the same
  // way as the main one, so it goes through the same decode, background removal
  // and validation — a hover cursor is a cursor.
  const [hand, setHand] = useState<ImportedImage | null>(null);
  const [handBusy, setHandBusy] = useState(false);

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
        if (first) {
          setLastSource(first);
          void accept(() => ipc.importImage(first, forceCut ? "force" : "auto"));
        }
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
      filters: [
        {
          name: "Anything with a picture in it",
          extensions: [
            "png", "jpg", "jpeg", "webp", "bmp", "gif", "apng", "ico", "tif", "tiff",
            "cur", "ani",
          ],
        },
      ],
    });
    if (typeof picked === "string") {
      setLastSource(picked);
      void accept(() => ipc.importImage(picked, forceCut ? "force" : "auto"));
    }
  };

  // Re-stages the same file with a different background decision.
  //
  // The cut happens when the image is staged, not when it is built, because
  // the preview and the hotspot picker both show the cut result — changing the
  // setting without re-staging would show one thing and build another.
  const recut = async (next: boolean) => {
    setForceCut(next);
    if (!lastSource) return;
    await accept(() => ipc.importImage(lastSource, next ? "force" : "auto"));
  };

  const browseHand = async () => {
    if (!ipc.isDesktop()) return;
    const picked = await open({
      multiple: false,
      filters: [
        {
          name: "Anything with a picture in it",
          extensions: ["png", "jpg", "jpeg", "webp", "bmp", "gif", "apng", "ico", "tif", "tiff"],
        },
      ],
    });
    if (typeof picked !== "string") return;
    setHandBusy(true);
    try {
      setHand(await ipc.importImage(picked, forceCut ? "force" : "auto"));
    } catch (e) {
      setError(e instanceof Error ? e.message : "That hover image could not be read.");
    } finally {
      setHandBusy(false);
    }
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
        handToken: hand?.token ?? null,
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
            className="display text-[10px] text-text-dim hover:text-text"
          >
            CLEAR
          </button>
        )}
      </ScreenHeader>

      {!image ? (
        <div className="min-h-0 flex-1 overflow-y-auto">
          <DropZone busy={busy} onBrowse={() => void browse()} />
          <CustomLibrary />
          <AcceptedFormats />
        </div>
      ) : (
        <div className="min-h-0 flex-1 overflow-y-auto px-4 py-4">
          {/* Sections, not one long stack.
              Everything used to sit in a single `space-y-1` column — a preview,
              a row of chips, a name field, a toggle, two dropdowns and a warning
              — with nothing to say which control belonged to which decision.
              Three cards: what it looks like, what it is, where it goes. */}
          <div className="grid grid-cols-2 gap-3">
            <HotspotPicker image={image} hotspot={hotspot} onChange={setHotspot} />
            <PreviewLadder previews={previews} />
          </div>

          <div className="panel mt-4 rounded-sm border border-border p-4">
            <div className="mb-3 flex items-baseline justify-between gap-3">
              <span className="display text-[11px] text-text-muted">HOTSPOT</span>
              <span className="mono shrink-0 text-[11px] text-text-dim">
                {hotspot[0].toFixed(2)} · {hotspot[1].toFixed(2)}
              </span>
            </div>
            <div className="flex flex-wrap gap-2">
              {HOTSPOT_PRESETS.map((preset) => (
                <button
                  key={preset.value}
                  type="button"
                  onClick={() => setHotspot(presetHotspot(preset.value, image))}
                  className="rounded-full border border-border px-3 py-1 text-[11px] text-text-muted transition-colors duration-150 hover:border-border-hi hover:text-text focus-visible:outline focus-visible:outline-1 focus-visible:outline-accent"
                >
                  {preset.label}
                </button>
              ))}
            </div>
          </div>

          <div className="panel mt-4 rounded-sm border border-border p-4">
            <span className="display mb-3 block text-[11px] text-text-muted">THE CURSOR</span>

            <TextInput
              value={name}
              onChange={setName}
              placeholder="Name this cursor"
              maxLength={48}
            />

            {/* Said before the toggle, because it changes what the toggle
                means. A refusal is not an error and is not styled as one: the
                import worked, the image is fine, and what did not happen is
                the background removal. */}
            {image?.refusal && (
              <div className="mt-2 rounded-xs border border-border-hi bg-elevated p-3">
                <p className="text-[11px] text-text">{image.refusal}</p>
                {!image.keyable && (
                  <p className="mt-1 text-[11px] text-text-dim">
                    You can still turn this on to try anyway — it will not be stopped, it
                    just probably will not look good.
                  </p>
                )}
              </div>
            )}

            <div className="mt-2">
              <Toggle
                checked={forceCut}
                onChange={(next) => void recut(next)}
                label="Remove the background"
                hint={
                  forceCut
                    ? "Cutting it out, whatever the file already claims"
                    : image && !image.keyable
                      ? "Not attempted — this image is not a flat background"
                      : "Cut automatically, unless the image is already transparent"
                }
              />

              <Toggle
                checked={outline}
                onChange={(next) => void refreshPreviews(next)}
                label="Add contrast outline"
                hint="Keeps a dark cursor visible on a white page"
              />
            </div>
          </div>

          <div className="panel mt-4 rounded-sm border border-border p-4">
            <div className="mb-3 flex items-baseline justify-between gap-3">
              <span className="display text-[11px] text-text-muted">HOVER CURSOR</span>
              {hand && (
                <button
                  type="button"
                  onClick={() => setHand(null)}
                  className="display shrink-0 text-[11px] text-text-dim transition-colors duration-150 hover:text-text"
                >
                  REMOVE
                </button>
              )}
            </div>

            {hand ? (
              <div className="flex items-center gap-4">
                <span className="grid h-14 w-14 shrink-0 place-items-center rounded-xs border border-border bg-bg/40">
                  <img src={hand.dataUri} alt="" className="max-h-12 max-w-12 object-contain" />
                </span>
                <p className="min-w-0 flex-1 text-[11px] leading-relaxed text-text-dim">
                  Used whenever you hover a link or a button. Its hotspot comes from its own
                  artwork, not the pointer&apos;s — a link cursor that inherits the arrow&apos;s
                  would point at the wrong pixel.
                </p>
              </div>
            ) : (
              <>
                <button
                  type="button"
                  onClick={() => void browseHand()}
                  disabled={handBusy}
                  className="tile hover:tile-hover flex h-20 w-full flex-col items-center justify-center gap-2 rounded-sm border border-dashed border-border-hi bg-surface/40 focus-visible:outline focus-visible:outline-1 focus-visible:outline-accent"
                >
                  <Plus size={16} strokeWidth={1.5} className="text-text-muted" />
                  <span className="text-[12px] text-text-muted">
                    {handBusy ? "Reading image…" : "Add a hover image"}
                  </span>
                </button>
                <p className="mt-3 text-[11px] leading-relaxed text-text-dim">
                  Optional. Without one, links use the pack chosen below.
                </p>
              </>
            )}
          </div>

          <div className="panel mt-4 rounded-sm border border-border p-4">
            <span className="display mb-3 block text-[11px] text-text-muted">WHERE IT GOES</span>

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

            {mode === "Blend" && (
              <div className="mt-3">
                <span className="mb-2 block text-[11px] text-text-muted">
                  Pack for the other roles
                </span>
                <Select
                  value={blendPack}
                  onChange={setBlendPack}
                  options={packs.map((pack) => ({ value: pack.id, label: pack.name }))}
                />
              </div>
            )}

            {mode === "All" && (
              <p className="mt-3 text-[11px] leading-relaxed text-danger">
                Every role becomes this image — including the text caret and the busy
                pointer. Blend usually looks better.
              </p>
            )}

            {image.animated && (
              <p className="mt-3 text-[11px] leading-relaxed text-text-dim">
                {image.frameCount} frames · built as an animated cursor at every size.
              </p>
            )}
          </div>

          <div className="mt-4 pb-2">
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
    <div className="p-4">
      {/* One invitation and nothing else inside the target.
          The format list used to sit under the label inside the same button, so
          three unrelated lines of text competed at the centre of the screen and
          the actual instruction was the least prominent of them. The list is
          reference material — it belongs at the bottom of the screen, out of the
          way, not in the middle of the thing you are meant to click. */}
      <button
        type="button"
        onClick={onBrowse}
        aria-busy={busy}
        className="tile hover:tile-hover flex h-44 w-full flex-col items-center justify-center gap-4 rounded-sm border border-dashed border-border-hi bg-surface/40 focus-visible:outline focus-visible:outline-1 focus-visible:outline-accent"
      >
        <span className="grid h-14 w-14 place-items-center rounded-full border border-border-hi text-text-muted">
          <Plus size={22} strokeWidth={1.5} />
        </span>
        <span className="text-[14px] text-text">
          {busy ? "Reading image…" : "Drop an image or GIF"}
        </span>
        <span className="text-[11px] text-text-dim">or click to browse</span>
      </button>
    </div>
  );
}

/**
 * What the importer accepts, in one line.
 *
 * This was a titled block with two labelled rows of chips and a paragraph — a
 * quarter of the screen spent on a list nobody reads twice, and read as clutter
 * rather than as reference. It says the same thing now: the formats, the size
 * limit, and that backgrounds come off by themselves.
 */
function AcceptedFormats() {
  return (
    <div className="border-t border-border px-4 py-3">
      <p className="text-[11px] leading-relaxed text-text-dim">
        <span className="mono text-text-muted">
          PNG · JPEG · WebP · BMP · ICO · TIFF · GIF · APNG
        </span>{" "}
        — up to 20&nbsp;MB. GIF and APNG become animated cursors, and backgrounds
        are removed automatically.
      </p>
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
      <span className="display mb-2 block text-[10px] text-text-dim">ACTUAL SIZE</span>
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

/**
 * Everything the user has already made, as a browsable shelf.
 *
 * Custom used to be a one-shot builder: make a cursor, apply it, and it
 * vanished from view even though the files were still on disk. The only way
 * back to one was to remember it existed. Listing them makes this a library of
 * the user's own work, which is what the screen was always doing anyway.
 */

function CustomLibrary() {
  const [items, setItems] = useState<ipc.CustomEntry[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [open, setOpen] = useState<ipc.CustomEntry | null>(null);
  const [size, setSize] = useState(32);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [busy, setBusy] = useState(false);

  const settings = useStore((s) => s.settings);
  const packs = useStore((s) => s.packs);
  const refreshActive = useStore((s) => s.refreshActive);
  const go = useStore((s) => s.go);

  const load = useCallback(() => {
    if (!ipc.isDesktop()) {
      setItems([]);
      return;
    }
    ipc
      .listCustomCursors()
      .then(setItems)
      .catch((e) =>
        setError(e instanceof Error ? e.message : "Could not read your custom cursors."),
      );
  }, []);

  useEffect(load, [load]);

  const select = (item: ipc.CustomEntry) => {
    setOpen(item);
    setSize(settings.cursorSize ?? 32);
    setConfirmDelete(false);
    setError(null);
  };

  // The same call the builder makes when it finishes, so a saved cursor and a
  // freshly made one take exactly one path to the pointer.
  const apply = async (id: string) => {
    setBusy(true);
    try {
      await ipc.applyCustomCursor({
        cursorId: id,
        applyMode: settings.applyMode,
        blendPackId:
          settings.applyMode === "Blend" ? settings.blendPack || packs[0]?.id || null : null,
        tint: settings.tint,
        size,
        outline: settings.outline,
      });
      await refreshActive();
      go("home");
    } catch (e) {
      setError(e instanceof Error ? e.message : "That cursor could not be applied.");
    } finally {
      setBusy(false);
    }
  };

  const remove = async (id: string) => {
    setBusy(true);
    try {
      await ipc.deleteCustomCursor(id);
      setOpen(null);
      load();
    } catch (e) {
      setError(e instanceof Error ? e.message : "That cursor could not be deleted.");
    } finally {
      setBusy(false);
    }
  };

  if (error && items === null) {
    return (
      <div className="px-4 pb-4">
        <p className="text-[11px] text-danger">{error}</p>
      </div>
    );
  }

  // Loading and empty are different states and must not look the same.
  if (items === null) {
    return (
      <div className="space-y-2 px-4 pb-4">
        <div className="h-2 w-1/3 rounded-full shimmer" />
        <div className="h-16 w-full rounded-sm shimmer" />
      </div>
    );
  }

  if (items.length === 0) {
    return (
      <div className="px-4 pb-4">
        <p className="text-[11px] text-text-dim">
          Anything you make is saved here automatically.
        </p>
      </div>
    );
  }

  // One cursor, opened.
  //
  // A detail panel rather than more controls stuffed into a tile. A 3-column
  // grid has no room for a slider and two buttons, and cramming them in is what
  // made this screen overlap in the first place.
  if (open) {
    return (
      <div className="px-4 pb-4">
        <div className="mb-3 flex items-center gap-2">
          <button
            type="button"
            onClick={() => setOpen(null)}
            className="display shrink-0 rounded-xs px-2 py-1 text-[11px] text-text-muted transition-colors duration-150 hover:text-text focus-visible:outline focus-visible:outline-1 focus-visible:outline-accent"
          >
            BACK
          </button>
          <span
            className="display min-w-0 flex-1 truncate text-[11px] text-text"
            title={open.name}
          >
            {open.name}
          </span>
        </div>

        <div className="panel rounded-sm border border-border p-4">
          {/* A fixed-height stage, so changing the size slider moves nothing
              else on the screen. A preview that grows the layout as you drag is
              how a panel starts overlapping what is under it. */}
          <div className="mb-4 grid h-36 place-items-center rounded-xs border border-border bg-bg/40">
            {open.preview ? (
              <img
                src={open.preview}
                alt=""
                style={{ width: size, height: size }}
                className="object-contain"
              />
            ) : (
              <span className="mono text-[11px] text-text-dim">no preview</span>
            )}
          </div>

          <Slider
            value={size}
            min={10}
            max={128}
            step={2}
            onChange={setSize}
            label="Size"
            suffix="px"
          />

          <div className="mt-3 flex flex-wrap gap-2">
            {[10, 16, 24, 32, 48, 64, 96, 128].map((preset) => (
              <button
                key={preset}
                type="button"
                onClick={() => setSize(preset)}
                className={
                  "mono rounded-full border px-3 py-1 text-[11px] transition-colors duration-150 focus-visible:outline focus-visible:outline-1 focus-visible:outline-accent " +
                  (size === preset
                    ? "border-accent bg-accent-dim/60 text-accent-hi"
                    : "border-border text-text-dim hover:border-border-hi hover:text-text")
                }
              >
                {preset}
              </button>
            ))}
          </div>

          <div className="mt-5 grid grid-cols-2 gap-2">
            <Button variant="ghost" onClick={() => setConfirmDelete(true)} disabled={busy}>
              DELETE
            </Button>
            <Button onClick={() => void apply(open.id)} disabled={busy}>
              {busy ? "APPLYING" : "USE THIS"}
            </Button>
          </div>

          {confirmDelete && (
            <div className="mt-3 rounded-xs border border-danger/40 bg-danger/10 p-3">
              <p className="text-[11px] break-words text-danger">
                Delete this cursor permanently? Its files are removed from disk.
              </p>
              <div className="mt-2 grid grid-cols-2 gap-2">
                <Button variant="ghost" onClick={() => setConfirmDelete(false)}>
                  KEEP
                </Button>
                <Button variant="danger" onClick={() => void remove(open.id)} disabled={busy}>
                  DELETE
                </Button>
              </div>
            </div>
          )}

          {error && <p className="mt-3 text-[11px] break-words text-danger">{error}</p>}
        </div>
      </div>
    );
  }

  return (
    <div className="px-4 pb-4">
      <div className="mb-3 flex items-baseline justify-between">
        <span className="display text-[11px] text-text-muted">YOUR CURSORS</span>
        <span className="mono text-[11px] text-text-dim">{items.length}</span>
      </div>

      <div className="grid grid-cols-3 gap-3">
        {items.map((item) => (
          <button
            key={item.id}
            type="button"
            title={item.name}
            onClick={() => select(item)}
            className="panel group flex min-w-0 flex-col items-center gap-2 rounded-sm border border-border p-3 tile hover:tile-hover focus-visible:outline focus-visible:outline-1 focus-visible:outline-accent"
          >
            <span className="grid h-12 w-12 shrink-0 place-items-center">
              {item.preview ? (
                <img src={item.preview} alt="" className="max-h-12 max-w-12 object-contain" />
              ) : (
                // Never an invisible gap: a tile that failed to render must
                // still be a tile, or the grid silently loses an entry.
                <span className="mono text-[11px] text-text-dim">?</span>
              )}
            </span>
            <span className="w-full truncate text-center text-[11px] text-text-muted">
              {item.name}
            </span>
            {item.animated && <span className="display text-[10px] text-accent-hi">ANIM</span>}
          </button>
        ))}
      </div>
    </div>
  );
}
