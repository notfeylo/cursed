import { useStore } from "../store";
import { Button } from "../components/ui";

/** The whole product in one screen: one verb, one button. */
export function Home() {
  const go = useStore((s) => s.go);
  const active = useStore((s) => s.active);

  return (
    <div className="screen-in flex h-full flex-col justify-center px-6 pb-6">
      <div className="flex flex-1 flex-col items-center justify-center gap-6">
        <HeroMark />
        <div className="text-center">
          <h1 className="display text-[26px] leading-[1.15] text-text">
            ENHANCE
            <br />
            YOUR CURSOR
          </h1>
          <p className="mt-2 text-[12px] text-text-muted">
            Give your dead cursor a new life.
          </p>
        </div>
      </div>

      <div className="flex flex-col gap-3">
        <Button full onClick={() => go("catalog")}>
          ENHANCE YOUR CURSOR
        </Button>

        <ActiveChip
          label={
            active.isDefault
              ? "ACTIVE — WINDOWS DEFAULT"
              : `ACTIVE — ${active.packName ?? "CUSTOM"}`
          }
          tint={active.tint}
          size={active.size}
          neutral={active.isDefault}
        />

        <div className="grid grid-cols-3 gap-2">
          <Button variant="ghost" onClick={() => go("custom")}>
            CUSTOM
          </Button>
          <Button variant="ghost" onClick={() => go("saved")}>
            SAVED
          </Button>
          <Button variant="ghost" onClick={() => go("settings")}>
            SETTINGS
          </Button>
        </div>
      </div>
    </div>
  );
}

function ActiveChip({
  label,
  tint,
  size,
  neutral,
}: {
  label: string;
  tint: string;
  size: number;
  neutral: boolean;
}) {
  return (
    <div className="flex items-center justify-center gap-2 rounded-xs border border-border bg-surface px-3 py-2">
      <span className="display truncate text-[10px] text-text-muted">{label}</span>
      {!neutral && (
        <>
          <span className="mono text-[10px] text-text-dim">·</span>
          <span className="mono text-[10px] text-text-dim">{size}px</span>
          <span
            className="h-3 w-3 shrink-0 rounded-full border border-border-hi"
            style={{ background: tint }}
            title={tint}
          />
        </>
      )}
    </div>
  );
}

/**
 * The mark: a pointer over a slowly breathing glow.
 *
 * Only the glow moves, and slowly. The mark sits directly above the button that
 * changes the real cursor, so anything livelier would compete with the thing the
 * user is here to look at. The global `prefers-reduced-motion` rule stops it.
 */
function HeroMark() {
  return (
    <div className="relative grid h-20 w-20 place-items-center">
      <div className="absolute inset-0 animate-[breathe_4s_ease-in-out_infinite] rounded-full bg-[var(--accent-glow)] blur-xl" />
      <svg width="44" height="44" viewBox="0 0 24 24" fill="none" className="relative">
        <path
          d="M5 3.2 19 12.4l-6.1 1.1 3.1 6.1-2.6 1.3-3.1-6.2L5 19.1Z"
          fill="var(--color-accent)"
          stroke="var(--color-accent-hi)"
          strokeWidth="0.9"
          strokeLinejoin="round"
        />
      </svg>
    </div>
  );
}
