import { useEffect, useState } from "react";
import { ArrowRight, Download, FolderPlus, Layers, Settings2 } from "lucide-react";
import { useStore } from "../store";
import { Button } from "../components/ui";
import { Mark } from "../components/Mark";
import * as ipc from "../lib/ipc";

/**
 * The first thing anyone sees, so it earns its space.
 *
 * The mark is the anchor of the composition rather than a corner decoration,
 * and everything below it reads top to bottom in one column: mark, display
 * line, tagline, the one action worth taking, what is currently applied, then
 * the quieter routes.
 *
 * Every size and gap here comes from the two scales fixed in the specimen —
 * type at 11/12/14/16/20/28/40, spacing at 4/8/12/16/24/32/48. Inconsistent
 * spacing is the single biggest reason an interface feels cheap, and it is
 * invisible until it is all in one place.
 */
export function Home() {
  const go = useStore((s) => s.go);
  const active = useStore((s) => s.active);
  const packs = useStore((s) => s.packs);

  const [update, setUpdate] = useState<ipc.UpdateState | null>(null);
  const [appVersion, setAppVersion] = useState("");

  // Visible without digging. "What version are you on?" was the first question
  // every support conversation needed and nobody could answer it.
  useEffect(() => {
    if (!ipc.isDesktop()) return;
    void ipc
      .getBuildInfo()
      .then((info) => setAppVersion(info.version))
      .catch(() => undefined);
  }, []);

  // The updater works in the background, so the home screen is where its result
  // should surface — nobody opens Settings to find out an update exists.
  useEffect(() => {
    if (!ipc.isDesktop()) return;
    const read = () => void ipc.getUpdateState().then(setUpdate).catch(() => undefined);
    read();
    const timer = window.setInterval(read, 4000);
    return () => window.clearInterval(timer);
  }, []);

  const ready = update?.ready === true;
  const latest = update?.status?.latest;

  return (
    <div className="screen-in relative flex h-full flex-col overflow-hidden">
      <Backdrop />

      <div className="relative flex flex-1 flex-col items-center justify-center px-6">
        {/* A soft bloom behind the mark — the one piece of character in the
            background, chosen over a vignette or a watermark because it does
            the compositional work of anchoring the eye rather than just adding
            texture. */}
        <div className="relative mb-6 grid place-items-center">
          <div
            className="pointer-events-none absolute h-40 w-40 rounded-full blur-3xl"
            style={{ background: "radial-gradient(circle, rgba(46,139,255,0.34), transparent 70%)" }}
          />
          <div className="relative">
            <Mark size={96} animated id="hero" />
          </div>
        </div>

        <h1 className="display text-center text-[28px] leading-[1.05] text-text">
          ENHANCE
          <br />
          YOUR CURSOR
        </h1>

        <p className="mt-3 text-center text-[12px] text-text-muted">Your pointer. Possessed.</p>

        <div className="mt-4 flex max-w-full items-center gap-2">
          <Pill icon={<Layers size={11} />} label={`${packs.length} CURSORS`} />
          <Pill
            label={active.isDefault ? "WINDOWS DEFAULT" : (active.packName ?? "CUSTOM")}
            tone={active.isDefault ? "dim" : "accent"}
          />
        </div>
      </div>

      <div className="relative px-4 pb-4">
        {ready && (
          <button
            type="button"
            onClick={() => go("settings")}
            className="mb-2 flex w-full items-center gap-2 rounded-sm border border-accent/50 bg-accent-dim/60 px-3 py-2 text-left transition-colors duration-150 ease-out hover:border-accent focus-visible:outline focus-visible:outline-1 focus-visible:outline-accent"
          >
            <Download size={13} className="shrink-0 text-accent-hi" />
            <span className="min-w-0 flex-1">
              <span className="display block text-[11px] text-accent-hi">
                VERSION {latest} READY
              </span>
              <span className="block truncate text-[11px] text-text-dim">
                Downloaded and verified — install it in Settings
              </span>
            </span>
            <ArrowRight size={13} className="shrink-0 text-text-dim" />
          </button>
        )}

        <Button full onClick={() => go("catalog")}>
          CHOOSE A CURSOR
        </Button>

        <div className="mt-2 grid grid-cols-3 gap-2">
          <Tile icon={<FolderPlus size={13} />} label="CUSTOM" onClick={() => go("custom")} />
          <Tile icon={<Layers size={13} />} label="SAVED" onClick={() => go("saved")} />
          <Tile icon={<Settings2 size={13} />} label="SETTINGS" onClick={() => go("settings")} />
        </div>

        {appVersion && (
          <button
            type="button"
            onClick={() => go("about")}
            title="About Cursed"
            className="mono mt-3 block w-full rounded-xs text-center text-[11px] text-text-dim transition-colors duration-150 ease-out hover:text-text-muted focus-visible:outline focus-visible:outline-1 focus-visible:outline-accent"
          >
            v{appVersion}
          </button>
        )}
      </div>
    </div>
  );
}

function Pill({
  icon,
  label,
  tone = "dim",
}: {
  icon?: React.ReactNode;
  label: string;
  tone?: "dim" | "accent";
}) {
  return (
    <span
      title={label}
      className={`display inline-flex min-w-0 items-center gap-1.5 rounded-full border px-3 py-1 text-[11px] ${
        tone === "accent"
          ? "border-accent/40 bg-accent-dim/50 text-accent-hi"
          : "border-border text-text-dim"
      }`}
    >
      {icon}
      <span className="truncate">{label}</span>
    </span>
  );
}

function Tile({
  icon,
  label,
  onClick,
}: {
  icon: React.ReactNode;
  label: string;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="panel flex flex-col items-center justify-center gap-1.5 rounded-sm border border-border py-3 text-text-dim transition-all duration-150 ease-out hover:-translate-y-px hover:border-accent hover:text-text focus-visible:outline focus-visible:outline-1 focus-visible:outline-accent"
    >
      {icon}
      <span className="display text-[11px]">{label}</span>
    </button>
  );
}

/**
 * A deep gradient with a fine grain over it. Completely static.
 *
 * The previous version drifted two blurred gradients on a CSS animation. That
 * looks pleasant for the ten seconds anyone watches it and then composites
 * forever, on a window that sits in the tray all day — which is exactly the kind
 * of cost that does not show up in a screenshot and does show up in a battery.
 *
 * The grain is a pre-baked `feTurbulence` tile as a data URI: rasterised once by
 * the browser, then repeated. No canvas, no animation frame, nothing to schedule.
 * It is what stops a large flat gradient from banding on a cheap panel.
 */
function Backdrop() {
  return (
    <div className="pointer-events-none absolute inset-0 overflow-hidden">
      <div
        className="absolute inset-0"
        style={{
          background:
            "radial-gradient(120% 90% at 50% 0%, #10182b 0%, #0a0e18 45%, var(--color-bg) 100%)",
        }}
      />
      <div
        className="absolute inset-0 opacity-[0.05] mix-blend-overlay"
        style={{ backgroundImage: `url("${GRAIN}")`, backgroundRepeat: "repeat" }}
      />
    </div>
  );
}

/** One 120px tile of fractal noise, inlined so it costs no request. */
const GRAIN =
  "data:image/svg+xml;utf8," +
  encodeURIComponent(
    `<svg xmlns='http://www.w3.org/2000/svg' width='120' height='120'>` +
      `<filter id='g'><feTurbulence type='fractalNoise' baseFrequency='0.85' numOctaves='3' stitchTiles='stitch'/></filter>` +
      `<rect width='120' height='120' filter='url(#g)'/></svg>`,
  );
