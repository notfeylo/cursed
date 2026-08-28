import { useCallback, useEffect, useRef, useState } from "react";
import { cx } from "./ui";

/**
 * Pick a color by looking at it.
 *
 * **What this replaces, and why it had to go.** Both places that took a color
 * took it as a hex field with a row of twelve swatches. If the color you wanted
 * was one of the twelve you were fine; if it was not, the app was asking you to
 * know that a particular orange is `#FF7A2E`. Nobody knows that. People were
 * leaving to search "blue color code", copying whatever the first result said,
 * and pasting it back — which is a browser trip to answer a question the app was
 * in a far better position to answer itself.
 *
 * So: drag across the gradient, read the code off the bottom. The hex field
 * stays and is still authoritative — somebody with a brand color in hand should
 * type it and be done — but it is no longer the only way in.
 *
 * ## Why HSV is held here rather than derived from the hex
 *
 * Hex is lossy about *where you were*. Drag the saturation to zero and every
 * hue produces `#FFFFFF`; converting that back gives hue 0, so the marker jumps
 * to red and the next drag starts somewhere you did not leave it. Black is
 * worse — it discards hue and saturation both. The gradient's own state is
 * therefore the source of truth while the picker is open, and the hex is what
 * falls out of it.
 */

export type Hsv = { h: number; s: number; v: number };

const HEX = /^#(?:[0-9a-f]{3}|[0-9a-f]{6})$/i;

export function isHex(text: string): boolean {
  return HEX.test(text.trim());
}

/** Expands `#abc` to `#aabbcc` and upper-cases, so stored values compare equal. */
export function normalizeHex(text: string): string {
  const trimmed = text.trim();
  const withHash = trimmed.startsWith("#") ? trimmed : `#${trimmed}`;
  if (!HEX.test(withHash)) return withHash.toUpperCase();
  const body = withHash.slice(1);
  const full =
    body.length === 3
      ? body
          .split("")
          .map((c) => c + c)
          .join("")
      : body;
  return `#${full.toUpperCase()}`;
}

export function hsvToHex({ h, s, v }: Hsv): string {
  const f = (n: number) => {
    const k = (n + h / 60) % 6;
    const value = v - v * s * Math.max(0, Math.min(k, 4 - k, 1));
    return Math.round(value * 255);
  };
  const hex = (n: number) => n.toString(16).padStart(2, "0");
  return `#${hex(f(5))}${hex(f(3))}${hex(f(1))}`.toUpperCase();
}

export function hexToHsv(text: string): Hsv | null {
  if (!HEX.test(text.trim())) return null;
  const body = normalizeHex(text).slice(1);
  const r = parseInt(body.slice(0, 2), 16) / 255;
  const g = parseInt(body.slice(2, 4), 16) / 255;
  const b = parseInt(body.slice(4, 6), 16) / 255;

  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  const span = max - min;

  let h = 0;
  if (span !== 0) {
    if (max === r) h = 60 * (((g - b) / span) % 6);
    else if (max === g) h = 60 * ((b - r) / span + 2);
    else h = 60 * ((r - g) / span + 4);
  }
  if (h < 0) h += 360;
  return { h, s: max === 0 ? 0 : span / max, v: max };
}

/**
 * Drag handling for both the gradient square and the hue strip.
 *
 * `setPointerCapture` is the whole point: without it the drag ends the instant
 * the pointer leaves the element, which for a color area you are dragging to
 * the very corner of is most of the time you actually want it.
 */
function useDrag(onMove: (fx: number, fy: number) => void) {
  const ref = useRef<HTMLDivElement>(null);

  const emit = useCallback(
    (clientX: number, clientY: number) => {
      const box = ref.current?.getBoundingClientRect();
      if (!box || box.width === 0 || box.height === 0) return;
      onMove(
        Math.min(1, Math.max(0, (clientX - box.left) / box.width)),
        Math.min(1, Math.max(0, (clientY - box.top) / box.height)),
      );
    },
    [onMove],
  );

  const onPointerDown = (e: React.PointerEvent<HTMLDivElement>) => {
    e.currentTarget.setPointerCapture(e.pointerId);
    emit(e.clientX, e.clientY);
  };
  const onPointerMove = (e: React.PointerEvent<HTMLDivElement>) => {
    if (e.buttons !== 1) return;
    emit(e.clientX, e.clientY);
  };

  return { ref, onPointerDown, onPointerMove };
}

export function ColorPicker({
  value,
  onChange,
  swatches,
}: {
  /** The color in effect. Hex, with or without `#`. */
  value: string;
  /** Called with a complete `#RRGGBB` on every change, including mid-drag. */
  onChange: (hex: string) => void;
  swatches?: readonly string[];
}) {
  const [hsv, setHsv] = useState<Hsv>(() => hexToHsv(value) ?? { h: 214, s: 0.82, v: 1 });
  const [draft, setDraft] = useState(() => normalizeHex(value));
  const [typing, setTyping] = useState(false);

  // Adopt a color set from outside — a preset being applied, a swatch pressed —
  // but never while the field is being typed into. Rewriting a half-typed value
  // under the caret is the bug this whole field was rebuilt to stop.
  useEffect(() => {
    if (typing) return;
    const next = normalizeHex(value);
    setDraft(next);
    // Only when it genuinely differs, so a round trip through our own
    // `onChange` cannot drag the marker off the spot the pointer is holding.
    if (next !== hsvToHex(hsv)) {
      const parsed = hexToHsv(next);
      if (parsed) setHsv(parsed);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [value, typing]);

  const commit = (next: Hsv) => {
    setHsv(next);
    const hex = hsvToHex(next);
    setDraft(hex);
    onChange(hex);
  };

  const area = useDrag((fx, fy) => commit({ ...hsv, s: fx, v: 1 - fy }));
  const hue = useDrag((fx) => commit({ ...hsv, h: fx * 360 }));

  /** Arrow keys, because a color area you can only reach with a mouse is not reachable. */
  const nudge = (e: React.KeyboardEvent, axis: "sv" | "h") => {
    const step = e.shiftKey ? 0.1 : 0.02;
    let next: Hsv | null = null;
    if (axis === "h") {
      if (e.key === "ArrowLeft") next = { ...hsv, h: (hsv.h - (e.shiftKey ? 30 : 6) + 360) % 360 };
      if (e.key === "ArrowRight") next = { ...hsv, h: (hsv.h + (e.shiftKey ? 30 : 6)) % 360 };
    } else {
      const clamp = (n: number) => Math.min(1, Math.max(0, n));
      if (e.key === "ArrowLeft") next = { ...hsv, s: clamp(hsv.s - step) };
      if (e.key === "ArrowRight") next = { ...hsv, s: clamp(hsv.s + step) };
      if (e.key === "ArrowUp") next = { ...hsv, v: clamp(hsv.v + step) };
      if (e.key === "ArrowDown") next = { ...hsv, v: clamp(hsv.v - step) };
    }
    if (next) {
      e.preventDefault();
      commit(next);
    }
  };

  const pure = hsvToHex({ h: hsv.h, s: 1, v: 1 });
  const current = hsvToHex(hsv);

  return (
    <div className="space-y-2">
      {/* Saturation left to right, brightness top to bottom, over the pure hue.
          The two gradients are stacked rather than computed per pixel: a canvas
          would be the same picture and a frame slower to react. */}
      <div
        ref={area.ref}
        role="slider"
        tabIndex={0}
        aria-label="Color saturation and brightness"
        aria-valuetext={current}
        onPointerDown={area.onPointerDown}
        onPointerMove={area.onPointerMove}
        onKeyDown={(e) => nudge(e, "sv")}
        className="relative h-28 w-full cursor-crosshair touch-none rounded-xs border border-border outline-none focus-visible:border-accent"
        style={{
          background: `linear-gradient(to top, #000, transparent), linear-gradient(to right, #fff, ${pure})`,
        }}
      >
        <Marker left={hsv.s} top={1 - hsv.v} color={current} />
      </div>

      <div
        ref={hue.ref}
        role="slider"
        tabIndex={0}
        aria-label="Color hue"
        aria-valuemin={0}
        aria-valuemax={360}
        aria-valuenow={Math.round(hsv.h)}
        onPointerDown={hue.onPointerDown}
        onPointerMove={hue.onPointerMove}
        onKeyDown={(e) => nudge(e, "h")}
        className="relative h-3.5 w-full cursor-ew-resize touch-none rounded-xs border border-border outline-none focus-visible:border-accent"
        style={{
          background:
            "linear-gradient(to right, #F00 0%, #FF0 17%, #0F0 33%, #0FF 50%, #00F 67%, #F0F 83%, #F00 100%)",
        }}
      >
        <Marker left={hsv.h / 360} top={0.5} color={pure} />
      </div>

      <div className="flex items-center gap-2">
        <span
          aria-hidden
          className="h-8 w-8 shrink-0 rounded-xs border border-border"
          style={{ background: isHex(draft) ? draft : current }}
        />
        <input
          value={draft}
          maxLength={7}
          spellCheck={false}
          placeholder="#2E8BFF"
          aria-label="Color code"
          onFocus={() => setTyping(true)}
          onChange={(e) => {
            const text = e.currentTarget.value;
            setDraft(text);
            // Applied the moment it becomes a color, so the pointer updates as
            // the last digit lands rather than only on blur.
            if (isHex(text)) {
              const full = normalizeHex(text);
              const parsed = hexToHsv(full);
              if (parsed) setHsv(parsed);
              onChange(full);
            }
          }}
          onBlur={() => {
            setTyping(false);
            // An incomplete entry reverts rather than being saved as nonsense.
            setDraft(isHex(draft) ? normalizeHex(draft) : normalizeHex(value));
          }}
          className={cx(
            "mono h-8 w-full rounded-xs border border-border bg-bg px-2 text-[12px] text-text outline-none transition-colors duration-150 placeholder:text-text-dim focus:border-accent",
            draft.length > 0 && !isHex(draft) && "border-danger",
          )}
        />
      </div>

      {swatches && swatches.length > 0 && (
        <div className="flex flex-wrap items-center gap-1.5 pt-0.5">
          {swatches.map((swatch) => (
            <button
              key={swatch}
              type="button"
              aria-label={swatch}
              title={swatch}
              onClick={() => {
                const full = normalizeHex(swatch);
                const parsed = hexToHsv(full);
                if (parsed) setHsv(parsed);
                setDraft(full);
                onChange(full);
              }}
              style={{ background: swatch }}
              className={cx(
                "h-5 w-5 rounded-full transition-transform duration-150",
                normalizeHex(swatch) === normalizeHex(draft)
                  ? "scale-125 ring-1 ring-text ring-offset-2 ring-offset-surface"
                  : "hover:scale-110",
              )}
            />
          ))}
        </div>
      )}
    </div>
  );
}

/**
 * The ring showing where you are.
 *
 * White ring, black hairline outside it. One or the other is always visible:
 * a white ring alone disappears at the top-left corner of the gradient, which
 * is exactly where somebody picking a near-white is working.
 */
function Marker({ left, top, color }: { left: number; top: number; color: string }) {
  return (
    <span
      aria-hidden
      className="pointer-events-none absolute block h-3 w-3 -translate-x-1/2 -translate-y-1/2 rounded-full border-2 border-white shadow-[0_0_0_1px_rgba(0,0,0,0.55)]"
      style={{ left: `${left * 100}%`, top: `${top * 100}%`, background: color }}
    />
  );
}
