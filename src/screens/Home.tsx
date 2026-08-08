import { useStore } from "../store";
import { Button } from "../components/ui";
import { Mark } from "../components/Mark";

/** The whole product in one screen: one verb, one button. */
export function Home() {
  const go = useStore((s) => s.go);
  const active = useStore((s) => s.active);

  return (
    <div className="screen-in relative flex h-full flex-col justify-center px-6 pb-6">
      {/* The grid sits behind everything and never scrolls — the surface the
          app rests on, not part of the content. */}
      <div className="circuit pointer-events-none absolute inset-0 opacity-60" />
      <div
        className="pointer-events-none absolute inset-x-0 top-0 h-56"
        style={{
          background:
            "radial-gradient(ellipse 70% 100% at 50% 0%, var(--accent-glow), transparent 70%)",
        }}
      />

      <div className="relative flex flex-1 flex-col items-center justify-center gap-6">
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

      <div className="relative flex flex-col gap-3">
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
 * The forge mark, breathing slowly over its own bloom.
 *
 * The motion is deliberately small: this sits directly above the button that
 * changes the real cursor, so anything livelier competes with the thing the user
 * came here to look at. The global `prefers-reduced-motion` rule stops it, and
 * the `no-motion` class freezes it while a live cursor preview is on screen.
 */
function HeroMark() {
  return (
    <div className="relative grid h-24 w-24 place-items-center">
      <div className="absolute inset-2 animate-[breathe_5s_ease-in-out_infinite] rounded-full bg-[var(--accent-glow)] blur-2xl" />
      <div className="relative">
        <Mark size={78} animated id="hero" />
      </div>
    </div>
  );
}
