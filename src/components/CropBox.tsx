import { useEffect, useRef, useState } from "react";
import { Crop as CropIcon, Undo2 } from "lucide-react";
import { containedRect, frameToImage, imageToFrame, type ContainedRect } from "../lib/contain";

/**
 * Drag a rectangle over the artwork and keep what is inside it.
 *
 * **The backend has been able to crop since the transform was written** — it is
 * the first step of `Transform::apply` — and there has never been a way to ask
 * for one. Every import was the whole picture, so a photograph with the subject
 * off to one side had to be cropped somewhere else and brought back.
 *
 * ## The rectangle is in the coordinates of what is on screen
 *
 * Not of the original file. The picture here may already have been turned,
 * mirrored and cropped once, and asking the user to think in the original's
 * coordinates would be asking them to do coordinate algebra with a mouse.
 * `Transform::composed_crop` on the other side maps this back through the turn
 * and folds it into any crop already in force — see the comment there, which is
 * where the two changes of basis are explained.
 *
 * ## …and in the coordinates of the *picture*, not of this box
 *
 * The frame is square and the image is `object-contain`, so anything that is
 * not square is letterboxed inside it. The pointer fractions were being sent
 * straight through as if the picture filled the frame, which stretched every
 * crop on a non-square image — see `lib/contain`. The selection is now mapped
 * into the picture's own coordinates on the way out, and back onto the frame
 * for drawing, so the box on screen and the crop that happens are the same
 * rectangle.
 *
 * ## Dragging is throttled to the frame rate
 *
 * A `setRect` per `pointermove` re-rendered five absolutely-positioned overlay
 * panels on every event, and pointer events outrun paint — so the selection
 * lagged behind the cursor on exactly the large images this control is for. The
 * live rectangle is kept in a ref and published once per animation frame.
 */
export function CropBox({
  src,
  cropped,
  busy,
  onCrop,
  onClear,
}: {
  src: string;
  /** Whether a crop is currently in force, so "remove" can be offered honestly. */
  cropped: boolean;
  busy: boolean;
  onCrop: (rect: [number, number, number, number]) => void;
  onClear: () => void;
}) {
  const frame = useRef<HTMLDivElement>(null);
  const start = useRef<[number, number] | null>(null);
  const pending = useRef<[number, number, number, number] | null>(null);
  const raf = useRef<number | null>(null);

  const [rect, setRect] = useState<[number, number, number, number] | null>(null);
  const [natural, setNatural] = useState<{ width: number; height: number } | null>(null);

  // A new picture is a new shape, and keeping the old measurement would map the
  // first drag through the previous image's aspect ratio.
  useEffect(() => {
    setNatural(null);
    setRect(null);
    start.current = null;
  }, [src]);

  useEffect(
    () => () => {
      if (raf.current !== null) cancelAnimationFrame(raf.current);
    },
    [],
  );

  /** Where the picture sits inside the square frame. */
  const shape = (): ContainedRect =>
    containedRect(
      frame.current?.getBoundingClientRect() ?? { width: 0, height: 0 },
      natural,
    );

  /** A pointer event as a point on the picture, 0–1 on each axis. */
  const at = (e: React.PointerEvent): [number, number] => {
    const box = frame.current?.getBoundingClientRect();
    if (!box || box.width === 0 || box.height === 0) return [0, 0];
    return frameToImage(
      [(e.clientX - box.left) / box.width, (e.clientY - box.top) / box.height],
      shape(),
    );
  };

  // Sorted on the way out, so dragging up-and-left gives the same rectangle as
  // dragging down-and-right rather than an inside-out one.
  const sorted = (a: [number, number], b: [number, number]) =>
    [
      Math.min(a[0], b[0]),
      Math.min(a[1], b[1]),
      Math.max(a[0], b[0]),
      Math.max(a[1], b[1]),
    ] as [number, number, number, number];

  /** Publishes the in-progress rectangle at most once per frame. */
  const schedule = (next: [number, number, number, number]) => {
    pending.current = next;
    if (raf.current !== null) return;
    raf.current = requestAnimationFrame(() => {
      raf.current = null;
      if (pending.current) setRect(pending.current);
    });
  };

  const shown = rect;
  // A rectangle a couple of pixels across is a mis-click, not a crop.
  const usable = shown !== null && shown[2] - shown[0] > 0.02 && shown[3] - shown[1] > 0.02;

  // The selection, put back onto the frame so it is drawn over the picture it
  // was measured against rather than over the letterbox bars.
  const drawn = (() => {
    if (!shown) return null;
    const box = shape();
    const [x0, y0] = imageToFrame([shown[0], shown[1]], box);
    const [x1, y1] = imageToFrame([shown[2], shown[3]], box);
    return { x0, y0, x1, y1 };
  })();

  return (
    <div>
      <div
        ref={frame}
        onPointerDown={(e) => {
          if (busy) return;
          e.currentTarget.setPointerCapture(e.pointerId);
          const point = at(e);
          start.current = point;
          setRect(sorted(point, point));
        }}
        onPointerMove={(e) => {
          if (!start.current || e.buttons !== 1) return;
          schedule(sorted(start.current, at(e)));
        }}
        onPointerUp={(e) => {
          start.current = null;
          // Released explicitly: a capture left in place keeps sending this
          // element events after the drag, and on a touch screen it stops the
          // buttons below from taking the next tap.
          if (e.currentTarget.hasPointerCapture(e.pointerId)) {
            e.currentTarget.releasePointerCapture(e.pointerId);
          }
          // Land on the exact final position rather than wherever the last
          // animation frame happened to leave it.
          if (pending.current) {
            setRect(pending.current);
            pending.current = null;
          }
        }}
        onPointerCancel={() => {
          start.current = null;
        }}
        className="relative w-full cursor-crosshair touch-none select-none overflow-hidden rounded-xs border border-border bg-bg"
        style={{ aspectRatio: "1 / 1" }}
      >
        {/* `contain` and a square frame, so the picture is never distorted. What
            it *is* stretched by is the mapping above, which is why the pointer
            fractions are converted rather than used raw. */}
        <img
          src={src}
          alt=""
          draggable={false}
          onLoad={(e) =>
            setNatural({
              width: e.currentTarget.naturalWidth,
              height: e.currentTarget.naturalHeight,
            })
          }
          className="pointer-events-none absolute inset-0 h-full w-full object-contain"
        />

        {drawn && (
          <>
            {/* Everything outside the selection, dimmed. Four panels rather
                than one box-shadow so it survives any theme. */}
            <div className="pointer-events-none absolute inset-0">
              <div
                className="absolute bg-black/55"
                style={{ left: 0, top: 0, right: 0, height: `${drawn.y0 * 100}%` }}
              />
              <div
                className="absolute bg-black/55"
                style={{ left: 0, bottom: 0, right: 0, height: `${(1 - drawn.y1) * 100}%` }}
              />
              <div
                className="absolute bg-black/55"
                style={{
                  left: 0,
                  width: `${drawn.x0 * 100}%`,
                  top: `${drawn.y0 * 100}%`,
                  height: `${(drawn.y1 - drawn.y0) * 100}%`,
                }}
              />
              <div
                className="absolute bg-black/55"
                style={{
                  right: 0,
                  width: `${(1 - drawn.x1) * 100}%`,
                  top: `${drawn.y0 * 100}%`,
                  height: `${(drawn.y1 - drawn.y0) * 100}%`,
                }}
              />
            </div>
            <div
              className="pointer-events-none absolute border border-accent"
              style={{
                left: `${drawn.x0 * 100}%`,
                top: `${drawn.y0 * 100}%`,
                width: `${(drawn.x1 - drawn.x0) * 100}%`,
                height: `${(drawn.y1 - drawn.y0) * 100}%`,
              }}
            />
          </>
        )}
      </div>

      <div className="mt-2 flex flex-wrap gap-2">
        <button
          type="button"
          disabled={!usable || busy}
          onClick={() => {
            if (shown) onCrop(shown);
            setRect(null);
          }}
          className="flex items-center gap-2 rounded-full border border-border px-3 py-1 text-[11px] text-text-muted transition-colors duration-150 hover:border-border-hi hover:text-text focus-visible:outline focus-visible:outline-1 focus-visible:outline-accent disabled:pointer-events-none disabled:opacity-40"
        >
          <CropIcon size={13} strokeWidth={1.5} />
          Crop to selection
        </button>
        <button
          type="button"
          disabled={!cropped || busy}
          onClick={() => {
            setRect(null);
            onClear();
          }}
          className="flex items-center gap-2 rounded-full border border-border px-3 py-1 text-[11px] text-text-muted transition-colors duration-150 hover:border-border-hi hover:text-text focus-visible:outline focus-visible:outline-1 focus-visible:outline-accent disabled:pointer-events-none disabled:opacity-40"
        >
          <Undo2 size={13} strokeWidth={1.5} />
          Use the whole picture
        </button>
      </div>
    </div>
  );
}
