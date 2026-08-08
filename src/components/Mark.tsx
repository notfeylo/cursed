/**
 * The CursorForge mark.
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

      <path
        className="mark-hex"
        d="M32 5 L54 17.5 L54 46.5 L32 59 L10 46.5 L10 17.5 Z"
        fill={`url(#${core})`}
        stroke={`url(#${hex})`}
        strokeWidth="2.4"
        strokeLinejoin="round"
      />
      <path
        className="mark-seam"
        d="M14 40 L50 26"
        stroke="var(--color-accent-hi)"
        strokeWidth="1.6"
        strokeLinecap="round"
        opacity="0.5"
      />
      <path
        className="mark-blade"
        d="M23 15 L45 36 L33.5 37.2 L39.6 50 L34.2 52.4 L28.2 39.6 L23 45.4 Z"
        fill={`url(#${blade})`}
        stroke="#ffffff"
        strokeWidth="1.5"
        strokeLinejoin="round"
      />
      <circle className="mark-spark" cx="47" cy="19" r="2.6" fill="#ffffff" opacity="0.92" />
    </svg>
  );
}
