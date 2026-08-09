/**
 * The Cursed mark.
 *
 * The same geometry as `src-tauri/src/packs/brand.rs`, which renders the app
 * icon and the site favicon — a forge chamber with a pointer struck through it.
 *
 * Gradient ids are suffixed per instance. Two marks on one screen sharing an id
 * would make the second silently adopt the first's gradient, which is the kind
 * of bug that only shows up on the one screen that renders both.
 */
export function Mark({
  size = 24,
  animated = false,
  id = "m",
}: {
  size?: number;
  animated?: boolean;
  id?: string;
}) {
  const hex = `hex-${id}`;
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
        <linearGradient id={hex} x1="0" y1="0" x2="0.6" y2="1">
          <stop offset="0" stopColor="var(--color-accent-hi)" stopOpacity="0.95" />
          <stop offset="1" stopColor="var(--color-accent)" stopOpacity="0.25" />
        </linearGradient>
        <linearGradient id={blade} x1="0.1" y1="0" x2="0.9" y2="1">
          <stop offset="0" stopColor="#ffffff" />
          <stop offset="0.42" stopColor="var(--color-accent-hi)" />
          <stop offset="1" stopColor="var(--color-accent)" />
        </linearGradient>
        <radialGradient id={core} cx="0.5" cy="0.42" r="0.62">
          <stop offset="0" stopColor="var(--color-accent-hi)" stopOpacity="0.5" />
          <stop offset="1" stopColor="var(--color-accent)" stopOpacity="0" />
        </radialGradient>
      </defs>

      {/* Below 32 px the ring's stroke, its gaps and the air around the pointer
          are all fighting for the same two pixels, so the mark switches to a
          solid disc with the pointer knocked out of it. Same reading — a
          pointer, contained — with figure and ground swapped. This mirrors
          `brand::small_mark_svg`, which is what goes into the .ico. */}
      {size < 32 ? (
        <path
          fillRule="evenodd"
          fill={`url(#${blade})`}
          d="M32 2 A30 30 0 1 1 31.99 2 Z M24 15 L45 35.5 L34 36.5 L39.5 48.5 L33.5 51 L28 39 L24 44 Z"
        />
      ) : (
        <>
          <circle cx="32" cy="32" r="24" fill={`url(#${core})`} />
          <g
            className="mark-hex"
            fill="none"
            stroke={`url(#${hex})`}
            strokeWidth="7"
            strokeLinecap="butt"
          >
            <path d="M32 4.5 A27.5 27.5 0 0 1 59.5 32 A27.5 27.5 0 0 1 32 59.5" />
            <path className="mark-seam" d="M21.4 6.6 A27.5 27.5 0 0 0 6.6 21.4" />
            <path className="mark-seam" d="M4.5 32 A27.5 27.5 0 0 0 19.3 56.9" />
          </g>
          <path
            className="mark-blade"
            d="M23 17 L43.5 36.5 L33.5 37.5 L38.5 49 L33.5 51 L28.5 39.5 L23 44.5 Z"
            fill={`url(#${blade})`}
            stroke="#ffffff"
            strokeWidth="1.2"
            strokeLinejoin="round"
          />
        </>
      )}
    </svg>
  );
}
