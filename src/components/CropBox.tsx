import { useRef, useState } from "react";
import { Crop as CropIcon, Undo2 } from "lucide-react";

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
  const [start, setStart] = useState<[number, number] | null>(null);
  const [rect, setRect] = useState<[number, number, number, number] | null>(null);

  const at = (e: React.PointerEvent): [number, number] => {
    const box = frame.current?.getBoundingClientRect();
    if (!box || box.width === 0 || box.height === 0) return [0, 0];
    return [
      Math.min(1, Math.max(0, (e.clientX - box.left) / box.width)),
      Math.min(1, Math.max(0, (e.clientY - box.top) / box.height)),
    ];
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

  const shown = rect;
  // A rectangle a couple of pixels across is a mis-click, not a crop.
  const usable = shown !== null && shown[2] - shown[0] > 0.02 && shown[3] - shown[1] > 0.02;

  return (
    <div>
      <div
        ref={frame}
        onPointerDown={(e) => {
          if (busy) return;
          e.currentTarget.setPointerCapture(e.pointerId);
          const point = at(e);
          setStart(point);
          setRect(sorted(point, point));
        }}
        onPointerMove={(e) => {
          if (!start || e.buttons !== 1) return;
          setRect(sorted(start, at(e)));
        }}
        onPointerUp={() => setStart(null)}
        className="relative w-full cursor-crosshair touch-none select-none overflow-hidden rounded-xs border border-border bg-bg"
        style={{ aspectRatio: "1 / 1" }}
      >
        {/* `contain` and a square frame, so the fractions the pointer produces
            are fractions of the *image* — with `cover` or a free aspect ratio
            they would be fractions of the box, and the crop would be taken from
            somewhere the user did not point at. */}
        <img
          src={src}
          alt=""
          draggable={false}
          className="pointer-events-none absolute inset-0 h-full w-full object-contain"
        />

        {shown && (
          <>
            {/* Everything outside the selection, dimmed. Four panels rather
                than one box-shadow so it survives any theme. */}
            <div className="pointer-events-none absolute inset-0">
              <div
                className="absolute bg-black/55"
                style={{ left: 0, top: 0, right: 0, height: `${shown[1] * 100}%` }}
              />
              <div
                className="absolute bg-black/55"
                style={{ left: 0, bottom: 0, right: 0, height: `${(1 - shown[3]) * 100}%` }}
              />
              <div
                className="absolute bg-black/55"
                style={{
                  left: 0,
                  width: `${shown[0] * 100}%`,
                  top: `${shown[1] * 100}%`,
                  height: `${(shown[3] - shown[1]) * 100}%`,
                }}
              />
              <div
                className="absolute bg-black/55"
                style={{
                  right: 0,
                  width: `${(1 - shown[2]) * 100}%`,
                  top: `${shown[1] * 100}%`,
                  height: `${(shown[3] - shown[1]) * 100}%`,
                }}
              />
            </div>
            <div
              className="pointer-events-none absolute border border-accent"
              style={{
                left: `${shown[0] * 100}%`,
                top: `${shown[1] * 100}%`,
                width: `${(shown[2] - shown[0]) * 100}%`,
                height: `${(shown[3] - shown[1]) * 100}%`,
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
