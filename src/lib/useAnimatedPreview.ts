import { useEffect, useRef, useState } from "react";
import * as ipc from "./ipc";
import { useStore } from "../store";

/**
 * The frames of one pack's arrow, played in a loop.
 *
 * ## Why this is not just an `<img>`
 *
 * The obvious way to animate a preview is to hand the browser one animated file
 * and let it play. That would mean encoding a GIF or an APNG on the Rust side:
 * GIF costs a 256-colour palette and one-bit alpha, which is exactly what a
 * cursor's soft edges cannot survive, and there is no APNG encoder in the tree.
 * Both would also mean a second animator that has to agree with the one that
 * builds the actual `.ani`.
 *
 * So the frames arrive as ordinary PNG data URIs with their real delays, and the
 * swap happens here. Full RGBA, no new dependency, and the timing is the
 * timing the file itself carries.
 *
 * ## Timing
 *
 * `setTimeout` per frame rather than one interval, because an `.ani` may give
 * every frame a different delay — a long hold followed by a fast run is a common
 * shape, and an interval averages that into something the artist did not draw.
 */
export function useAnimatedPreview(packId: string | null): {
  /** The frame to draw, or `null` before the frames have loaded. */
  current: string | null;
  /** Whether this pack actually has more than one frame. */
  animated: boolean;
} {
  const [frames, setFrames] = useState<ipc.PreviewFrame[]>([]);
  const [index, setIndex] = useState(0);

  // A live system-cursor preview freezes the rest of the UI's motion, and this
  // is motion the `no-motion` class cannot reach — swapping an `<img>` src is
  // not a CSS animation. Held still explicitly instead.
  const previewing = useStore((s) => s.previewing);

  useEffect(() => {
    setFrames([]);
    setIndex(0);
    if (!packId || !ipc.isDesktop()) return;

    // Guards against a slow response for a pack the user has already left.
    let cancelled = false;
    void ipc
      .previewFrames(packId)
      .then((next) => {
        if (!cancelled) setFrames(next);
      })
      // A preview that will not load is not worth an error banner: the still
      // from the catalog is already on screen and stays there.
      .catch(() => undefined);

    return () => {
      cancelled = true;
    };
  }, [packId]);

  const timer = useRef<number | null>(null);

  useEffect(() => {
    if (timer.current !== null) {
      window.clearTimeout(timer.current);
      timer.current = null;
    }
    if (frames.length < 2 || previewing) return;

    const delay = frames[index]?.delayMs ?? 100;
    timer.current = window.setTimeout(() => {
      setIndex((at) => (at + 1) % frames.length);
    }, Math.max(20, delay));

    return () => {
      if (timer.current !== null) {
        window.clearTimeout(timer.current);
        timer.current = null;
      }
    };
  }, [frames, index, previewing]);

  return {
    current: frames[index]?.dataUri ?? frames[0]?.dataUri ?? null,
    animated: frames.length > 1,
  };
}
