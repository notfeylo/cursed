import { useCallback, useEffect, useRef, useState } from "react";
import { Button, Slider } from "./ui";
import * as ipc from "../lib/ipc";
import type { ImportedImage } from "../lib/types";

/**
 * Cutting a background out by hand.
 *
 * This exists because the automatic path is now allowed to refuse. A refusal
 * without somewhere to go is a dead end — the app says it cannot do this and
 * offers the user no way to do it themselves — so the two shipped together.
 *
 * ## Everything happens on a canvas, and the alpha is the only thing edited
 *
 * The colour channels are never touched. A brush that painted colour would be a
 * paint program; what this does is decide, per pixel, how much of the image is
 * there. That keeps every stroke reversible in principle and makes "apply"
 * nothing more than reading the canvas back out as a PNG.
 *
 * ## Undo is not optional
 *
 * A brush without undo is worse than no brush: one bad drag and the import is
 * ruined with no way back. Every stroke pushes the whole alpha channel onto a
 * bounded stack before it starts. Whole-buffer snapshots are wasteful and are
 * the version that is obviously correct — a 512×512 mask is 256 KB, twenty of
 * them is five megabytes, and the alternative is a diff format with its own
 * bugs sitting between the user and their work.
 *
 * ## The slider previews with the real matte
 *
 * Dragging it asks the backend to key the original at that tolerance. It is a
 * round trip per change rather than something approximated here, because an
 * editor that previews with a different algorithm than it applies is worse than
 * one with no preview at all.
 */
export function MatteEditor({
  image,
  onApply,
  onCancel,
}: {
  image: ImportedImage;
  /** Called with the edited image, already re-staged. */
  onApply: (next: ImportedImage) => void;
  onCancel: () => void;
}) {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const originalRef = useRef<ImageData | null>(null);
  const undoRef = useRef<Uint8ClampedArray[]>([]);

  const [session, setSession] = useState<ipc.MatteSession | null>(null);
  const [tolerance, setTolerance] = useState(0);
  const [brush, setBrush] = useState(24);
  const [mode, setMode] = useState<"erase" | "restore">("erase");
  const [busy, setBusy] = useState(false);
  const [canUndo, setCanUndo] = useState(false);
  const [failed, setFailed] = useState<string | null>(null);

  /** Draws a data URI onto the canvas, replacing whatever is there. */
  const paint = useCallback(async (dataUri: string) => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const context = canvas.getContext("2d", { willReadFrequently: true });
    if (!context) return;

    const bitmap = new Image();
    await new Promise<void>((resolve, reject) => {
      bitmap.onload = () => resolve();
      bitmap.onerror = () => reject(new Error("the preview could not be drawn"));
      bitmap.src = dataUri;
    });
    canvas.width = bitmap.width;
    canvas.height = bitmap.height;
    context.clearRect(0, 0, canvas.width, canvas.height);
    context.drawImage(bitmap, 0, 0);
  }, []);

  // Open on the staged image, showing what the automatic path produced.
  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const opened = await ipc.openMatteEditor(image.token);
        if (cancelled) return;
        setSession(opened);
        setTolerance(opened.suggestedTolerance);
        await paint(image.dataUri);

        // The untouched pixels, kept so a restore stroke has something to put
        // back. Read once: it never changes.
        const canvas = canvasRef.current;
        const context = canvas?.getContext("2d", { willReadFrequently: true });
        if (!canvas || !context) return;
        const source = new Image();
        await new Promise<void>((resolve) => {
          source.onload = () => resolve();
          source.onerror = () => resolve();
          source.src = opened.originalDataUri;
        });
        const scratch = document.createElement("canvas");
        scratch.width = canvas.width;
        scratch.height = canvas.height;
        const scratchContext = scratch.getContext("2d", { willReadFrequently: true });
        scratchContext?.drawImage(source, 0, 0, canvas.width, canvas.height);
        originalRef.current =
          scratchContext?.getImageData(0, 0, canvas.width, canvas.height) ?? null;
      } catch (e) {
        if (!cancelled) setFailed(e instanceof Error ? e.message : String(e));
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [image.token, image.dataUri, paint]);

  /** Snapshots the alpha channel before a stroke begins. */
  const pushUndo = () => {
    const context = canvasRef.current?.getContext("2d", { willReadFrequently: true });
    const canvas = canvasRef.current;
    if (!context || !canvas) return;
    const frame = context.getImageData(0, 0, canvas.width, canvas.height);
    const alpha = new Uint8ClampedArray(frame.data.length / 4);
    for (let i = 0; i < alpha.length; i += 1) alpha[i] = frame.data[i * 4 + 3] ?? 0;
    undoRef.current.push(alpha);
    // Bounded. A long session must not grow without limit, and twenty strokes
    // back is further than anyone reaches.
    if (undoRef.current.length > 20) undoRef.current.shift();
    setCanUndo(true);
  };

  const undo = () => {
    const previous = undoRef.current.pop();
    const canvas = canvasRef.current;
    const context = canvas?.getContext("2d", { willReadFrequently: true });
    if (!previous || !canvas || !context) return;
    const frame = context.getImageData(0, 0, canvas.width, canvas.height);
    for (let i = 0; i < previous.length; i += 1) frame.data[i * 4 + 3] = previous[i] ?? 0;
    context.putImageData(frame, 0, 0);
    setCanUndo(undoRef.current.length > 0);
  };

  /**
   * One dab of the brush.
   *
   * Alpha only, and squared falloff from the centre so an edge worked by hand
   * does not end up harder than the antialiased one beside it.
   */
  const dab = (clientX: number, clientY: number) => {
    const canvas = canvasRef.current;
    const context = canvas?.getContext("2d", { willReadFrequently: true });
    const original = originalRef.current;
    if (!canvas || !context) return;

    const box = canvas.getBoundingClientRect();
    const cx = Math.round(((clientX - box.left) / box.width) * canvas.width);
    const cy = Math.round(((clientY - box.top) / box.height) * canvas.height);
    // The brush is sized against what the user sees, not against the pixel
    // buffer, so it stays the same size under the hand at any zoom.
    const radius = Math.max(1, Math.round((brush / 2) * (canvas.width / box.width)));

    const x0 = Math.max(0, cx - radius);
    const y0 = Math.max(0, cy - radius);
    const x1 = Math.min(canvas.width - 1, cx + radius);
    const y1 = Math.min(canvas.height - 1, cy + radius);
    if (x1 < x0 || y1 < y0) return;

    const frame = context.getImageData(x0, y0, x1 - x0 + 1, y1 - y0 + 1);
    for (let y = y0; y <= y1; y += 1) {
      for (let x = x0; x <= x1; x += 1) {
        const dx = x - cx;
        const dy = y - cy;
        const d = Math.sqrt(dx * dx + dy * dy);
        if (d > radius) continue;
        const strength = 1 - (d / radius) ** 2;
        const local = ((y - y0) * (x1 - x0 + 1) + (x - x0)) * 4;

        const here = frame.data[local + 3] ?? 0;
        if (mode === "erase") {
          frame.data[local + 3] = Math.round(here * (1 - strength));
        } else if (original) {
          // Restore puts back what was there, colour and all — a pixel the
          // matte cleared had its colour zeroed with its alpha, so restoring
          // the alpha alone would paint transparent black.
          const global = (y * canvas.width + x) * 4;
          const target = original.data[global + 3] ?? 0;
          const wanted = Math.round(here + (target - here) * strength);
          if (wanted > here) {
            frame.data[local] = original.data[global] ?? 0;
            frame.data[local + 1] = original.data[global + 1] ?? 0;
            frame.data[local + 2] = original.data[global + 2] ?? 0;
            frame.data[local + 3] = wanted;
          }
        }
      }
    }
    context.putImageData(frame, x0, y0);
  };

  const painting = useRef(false);

  /** Re-keys the original at a new tolerance. Discards brushwork, and says so. */
  const retone = async (next: number) => {
    setTolerance(next);
    if (!session) return;
    setBusy(true);
    try {
      const preview = await ipc.previewMatte(image.token, next);
      pushUndo();
      await paint(preview);
    } catch (e) {
      setFailed(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const reset = async () => {
    if (!session) return;
    pushUndo();
    await paint(session.originalDataUri);
  };

  /**
   * Hands the edited pixels back as a new staged image.
   *
   * Through the ordinary import path with `keep`, so the user's alpha is not
   * re-keyed on the way in and nothing about this bypasses the validation every
   * other import goes through.
   */
  const apply = async () => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    setBusy(true);
    try {
      const blob = await new Promise<Blob | null>((resolve) =>
        canvas.toBlob((b) => resolve(b), "image/png"),
      );
      if (!blob) throw new Error("the edited image could not be read back");
      const bytes = Array.from(new Uint8Array(await blob.arrayBuffer()));
      onApply(await ipc.importImageBytes(bytes, "keep"));
    } catch (e) {
      setFailed(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex flex-col bg-bg/95 p-4">
      <div className="mb-3 flex items-baseline justify-between">
        <span className="display text-[11px] text-text-muted">CUT IT OUT YOURSELF</span>
        <button
          type="button"
          onClick={onCancel}
          className="display text-[10px] text-text-dim hover:text-text"
        >
          CANCEL
        </button>
      </div>

      {/* The checkerboard is the point: on a flat ground, a hole and a white
          subject look identical, which is exactly what the user is here to
          tell apart. */}
      <div
        className="min-h-0 flex-1 overflow-hidden rounded-xs border border-border"
        style={{
          backgroundImage:
            "linear-gradient(45deg, #2a2a2e 25%, transparent 25%), " +
            "linear-gradient(-45deg, #2a2a2e 25%, transparent 25%), " +
            "linear-gradient(45deg, transparent 75%, #2a2a2e 75%), " +
            "linear-gradient(-45deg, transparent 75%, #2a2a2e 75%)",
          backgroundSize: "16px 16px",
          backgroundPosition: "0 0, 0 8px, 8px -8px, -8px 0",
          backgroundColor: "#232327",
        }}
      >
        <canvas
          ref={canvasRef}
          className="h-full w-full touch-none object-contain"
          style={{ cursor: "crosshair", imageRendering: "pixelated" }}
          onPointerDown={(e) => {
            painting.current = true;
            e.currentTarget.setPointerCapture(e.pointerId);
            pushUndo();
            dab(e.clientX, e.clientY);
          }}
          onPointerMove={(e) => {
            if (painting.current) dab(e.clientX, e.clientY);
          }}
          onPointerUp={() => {
            painting.current = false;
          }}
          onPointerLeave={() => {
            painting.current = false;
          }}
        />
      </div>

      {failed && <p className="mt-2 text-[11px] text-danger">{failed}</p>}

      <div className="mt-3 space-y-3">
        <div className="grid grid-cols-2 gap-2">
          <Button
            variant={mode === "erase" ? "primary" : "ghost"}
            onClick={() => setMode("erase")}
          >
            ERASE
          </Button>
          <Button
            variant={mode === "restore" ? "primary" : "ghost"}
            onClick={() => setMode("restore")}
          >
            RESTORE
          </Button>
        </div>

        <Slider label="BRUSH" suffix="px" min={4} max={96} value={brush} onChange={setBrush} />

        {session && (
          <>
            <Slider
              label="TOLERANCE"
              min={session.minTolerance}
              max={session.maxTolerance}
              value={tolerance}
              onChange={(v) => void retone(v)}
            />
            <p className="text-[11px] text-text-dim">
              Moving this re-cuts from the original, so brush strokes are replaced. Undo
              brings them back.
            </p>
          </>
        )}

        <div className="grid grid-cols-3 gap-2">
          <Button variant="ghost" onClick={undo} disabled={!canUndo}>
            UNDO
          </Button>
          <Button variant="ghost" onClick={() => void reset()} disabled={busy}>
            RESET
          </Button>
          <Button onClick={() => void apply()} disabled={busy}>
            {busy ? "WORKING" : "APPLY"}
          </Button>
        </div>
      </div>
    </div>
  );
}
