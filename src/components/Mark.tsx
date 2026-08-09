/**
 * The Cursed mark.
 *
 * The same geometry as `src-tauri/src/packs/brand.rs`, which renders the app
 * icon, the tray icon and the site favicon — a pointer seen almost edge-on, a
 * broad wedge with a flat base and a curled tip.
 *
 * The path is traced from the supplied artwork rather than redrawn by eye, so
 * the angles are the artwork's own. One solid silhouette with no counters, which
 * is why there is no separate small-size drawing: it holds together at 16 px on
 * its own.
 *
 * Gradient ids are suffixed per instance. Two marks on one screen sharing an id
 * would make the second silently adopt the first's gradient, which is the kind
 * of bug that only shows up on the one screen that renders both.
 */

/** Traced from the artwork; kept identical to `brand::MARK`. */
const MARK =
  "M45.15 10.76 L46.49 13.46 L44.47 34.36 L61.33 51.89 L2.00 51.89 L4.02 48.52 L44.47 11.44 Z";

export function Mark({
  size = 24,
  animated = false,
  id = "m",
}: {
  size?: number;
  animated?: boolean;
  id?: string;
}) {
  const blade = `blade-${id}`;
  const core = `core-${id}`;

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
        <linearGradient id={blade} x1="0.15" y1="0" x2="0.85" y2="1">
          <stop offset="0" stopColor="#ffffff" />
          <stop offset="0.45" stopColor="var(--color-accent-hi)" />
          <stop offset="1" stopColor="var(--color-accent)" />
        </linearGradient>
        <radialGradient id={core} cx="0.5" cy="0.6" r="0.6">
          <stop offset="0" stopColor="var(--color-accent-hi)" stopOpacity="0.45" />
          <stop offset="1" stopColor="var(--color-accent)" stopOpacity="0" />
        </radialGradient>
      </defs>

      {/* The bloom is only worth drawing when there is room for it to read. */}
      {size >= 40 && <ellipse className="mark-seam" cx="32" cy="40" rx="30" ry="22" fill={`url(#${core})`} />}
      <path className="mark-blade" d={MARK} fill={`url(#${blade})`} />
    </svg>
  );
}
