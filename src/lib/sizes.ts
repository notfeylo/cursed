/**
 * The rungs a `.cur` file is built at, mirroring `TARGET_SIZES` in
 * `src-tauri/src/build/cur_writer.rs`.
 *
 * The size control offers these and nothing between them, and that is the whole
 * point. A `.cur` carries a fixed set of resolutions; ask Windows for a size
 * that is not one of them and the shell scales the nearest entry to fit —
 * bilinear, unpremultiplied, no gamma correction — and the pointer arrives soft
 * with its edge stepped.
 *
 * The slider used to move in twos from 10 to 128: sixty positions against these
 * rungs, fifty-two of which were a stretch. The eight that were not were exactly
 * the preset buttons underneath it, which is why clicking a preset looked sharp
 * and dragging the slider did not.
 *
 * The backend snaps too, in `Settings::sanitised` and `effective_size`, so a
 * hand-edited settings file or a size inherited from Windows' own accessibility
 * slider lands on a rung as well. This list only keeps the control honest about
 * what it is choosing between.
 */
export const CURSOR_SIZES = [10, 16, 24, 32, 48, 64, 96, 128] as const;

/** The nearest rung to an arbitrary size. */
export function snapCursorSize(size: number): number {
  return CURSOR_SIZES.reduce((best, rung) =>
    Math.abs(rung - size) < Math.abs(best - size) ? rung : best,
  );
}
