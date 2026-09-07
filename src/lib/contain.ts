/**
 * Where an `object-contain` image really sits inside its box.
 *
 * ## Why this exists
 *
 * The crop rectangle and the hotspot are both sent to the backend as fractions
 * **of the picture** — `Transform::composed_crop` and `move_point_into_crop`
 * treat `0,0`–`1,1` as the artwork's own corners. The controls that produce them
 * are square frames holding an `object-contain` image, and they were reading
 * fractions **of the frame**.
 *
 * Those are the same number only when the picture is square. Import a 1920×1080
 * screenshot and `object-contain` letterboxes it: the artwork occupies the
 * middle 56% of the frame's height with empty bars above and below. Dragging a
 * box around the subject then produced a rectangle whose vertical coordinates
 * were stretched by nearly two, so the crop landed somewhere the user had not
 * pointed at — and the hotspot marker sat at a different place on the artwork
 * than where it was dropped.
 *
 * It is invisible on a square image, which is what a cursor usually is, and
 * wrong on every other one.
 */

/** The image's box, as fractions of the frame it is drawn in. */
export interface ContainedRect {
  left: number;
  top: number;
  width: number;
  height: number;
}

/** The whole frame — the correct answer while dimensions are still unknown. */
const WHOLE: ContainedRect = { left: 0, top: 0, width: 1, height: 1 };

const clamp01 = (n: number) => Math.min(1, Math.max(0, n));

/**
 * The rect an `object-contain` image occupies inside a box.
 *
 * Falls back to the whole frame for a missing or zero dimension, which is the
 * state between a `src` changing and its `load` firing. Treating an unmeasured
 * image as filling the frame keeps the old behaviour for that one moment rather
 * than collapsing the picture to a point.
 */
export function containedRect(
  box: { width: number; height: number },
  natural: { width: number; height: number } | null,
): ContainedRect {
  if (!natural || !box.width || !box.height || !natural.width || !natural.height) {
    return WHOLE;
  }
  const boxAspect = box.width / box.height;
  const imageAspect = natural.width / natural.height;

  // Wider than the box: full width, bars above and below.
  if (imageAspect > boxAspect) {
    const height = boxAspect / imageAspect;
    return { left: 0, top: (1 - height) / 2, width: 1, height };
  }
  // Taller than the box: full height, bars either side.
  const width = imageAspect / boxAspect;
  return { left: (1 - width) / 2, top: 0, width, height: 1 };
}

/**
 * A point in frame fractions, expressed in the picture's own fractions.
 *
 * Clamped, so dragging out onto a letterbox bar pins to the picture's edge
 * instead of returning a coordinate outside it.
 */
export function frameToImage(
  point: [number, number],
  rect: ContainedRect,
): [number, number] {
  return [
    clamp01((point[0] - rect.left) / (rect.width || 1)),
    clamp01((point[1] - rect.top) / (rect.height || 1)),
  ];
}

/** The inverse: a point on the picture, placed on the frame for drawing. */
export function imageToFrame(
  point: [number, number],
  rect: ContainedRect,
): [number, number] {
  return [rect.left + point[0] * rect.width, rect.top + point[1] * rect.height];
}
