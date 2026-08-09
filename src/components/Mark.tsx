/**
 * The Cursed mark.
 *
 * The same geometry as `src-tauri/src/packs/brand.rs`, which renders the app
 * icon, the tray icon and the site favicon — a pointer seen almost edge-on, a
 * broad wedge with a flat base and a curled tip. The path is traced from the
 * supplied artwork rather than redrawn by eye, so the angles are the artwork's
 * own.
 *
 * Depth comes from a fold and a cast shadow, never from a glow. The traced
 * outline has a real crease in it — the tip and the notch both sit at x≈44.5 —
 * so a vertical line from tip to base splits the wedge into a broad lit face and
 * a narrow wing in shadow. That is a tonal step *inside* the silhouette, so if
 * the two tones merge at a small size what remains is exactly the solid wedge.
 *
 * Gradient ids are suffixed per instance. Two marks on one screen sharing an id
 * would make the second silently adopt the first's gradient, which is the kind
 * of bug that only shows up on the one screen that renders both.
 */

/** Traced from the artwork; kept identical to `brand::MARK`. */
const MARK =
  "M45.15 10.76 L46.49 13.46 L44.47 34.36 L61.33 51.89 L2.00 51.89 L4.02 48.52 L44.47 11.44 Z";

/** The part right of the fold; kept identical to `brand::WING`. */
const WING =
  "M44.47 11.44 L45.15 10.76 L46.49 13.46 L44.47 34.36 L61.33 51.89 L44.47 51.89 Z";

export function Mark({
  size = 24,
  animated = false,
  id = "m",
}: {
  size?: number;
  animated?: boolean;
  id?: string;
}) {
  const face = `face-${id}`;
  const fold = `fold-${id}`;
  const cast = `cast-${id}`;

  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 64 64"
      fill="none"
      aria-hidden="true"
      className={animated ? "mark-live" : undefined}
    >
      <defs>
        <linearGradient id={face} x1="0.1" y1="0" x2="0.75" y2="1">
          <stop offset="0" stopColor="var(--color-accent-hi)" />
          <stop offset="1" stopColor="var(--color-accent)" />
        </linearGradient>
        <linearGradient id={fold} x1="0" y1="0" x2="0.4" y2="1">
          <stop offset="0" stopColor="var(--color-accent)" />
          <stop offset="1" stopColor="#123a72" />
        </linearGradient>
        <filter id={cast} x="-25%" y="-25%" width="160%" height="160%">
          <feGaussianBlur stdDeviation="1.4" />
        </filter>
      </defs>

      {/* Only worth casting when there are pixels for it to fall on. */}
      {size >= 32 && (
        <g filter={`url(#${cast})`} opacity="0.5">
          <path d={MARK} fill="#000000" transform="translate(2.4 3)" />
        </g>
      )}
      <path className="mark-blade" d={MARK} fill={`url(#${face})`} />
      <path className="mark-blade" d={WING} fill={`url(#${fold})`} />
    </svg>
  );
}
