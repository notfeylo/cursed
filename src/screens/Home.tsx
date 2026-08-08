import { useEffect, useState } from "react";
import { ArrowRight, Download, FolderPlus, Layers, Settings2 } from "lucide-react";
import { useStore } from "../store";
import { Button } from "../components/ui";
import { Mark } from "../components/Mark";
import * as ipc from "../lib/ipc";

/**
 * The first thing anyone sees, so it earns its space.
 *
 * The grid backdrop is gone — a repeating pattern behind a dark UI reads as
 * texture-for-its-own-sake and flattens everything in front of it. What replaces
 * it is depth: two slow, offset colour fields drifting behind the mark, so the
 * background has somewhere to recede *to* rather than being a wall.
 */
export function Home() {
  const go = useStore((s) => s.go);
  const active = useStore((s) => s.active);
  const packs = useStore((s) => s.packs);

  const [update, setUpdate] = useState<ipc.UpdateState | null>(null);
  const [appVersion, setAppVersion] = useState("");

  // Visible without digging. "What version are you on?" has to be answerable in
  // two seconds by someone with no technical skill — it was the first question
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
  const version = update?.status?.latest;

  return (
    <div className="screen-in relative flex h-full flex-col overflow-hidden">
      <Aurora />

      <div className="relative flex flex-1 flex-col items-center justify-center px-6">
        <div className="relative mb-5 grid place-items-center">
          <div className="absolute h-32 w-32 rounded-full bg-[var(--accent-glow)] blur-3xl" />
          <div className="relative">
            <Mark size={92} animated id="hero" />
          </div>
        </div>

        <h1 className="display text-center text-[30px] leading-[1.05] text-text">
          CURSE
          <br />
          YOUR CURSOR
        </h1>

        <p className="mt-3 text-center text-[12px] text-text-muted">Your pointer. Possessed.</p>

        <div className="mt-4 flex items-center gap-2">
          <Pill icon={<Layers size={10} />} label={`${packs.length} CURSORS`} />
          <Pill
            label={active.isDefault ? "WINDOWS DEFAULT" : (active.packName ?? "CUSTOM")}
            tone={active.isDefault ? "dim" : "accent"}
          />
        </div>
      </div>

      <div className="relative px-5 pb-5">
        {ready && (
          <button
            type="button"
            onClick={() => go("settings")}
            className="mb-2 flex w-full items-center gap-2 rounded-sm border border-accent/50 bg-accent-dim/60 px-3 py-2 text-left transition-colors duration-150 hover:border-accent"
          >
            <Download size={13} className="shrink-0 text-accent-hi" />
            <span className="min-w-0 flex-1">
              <span className="display block text-[10px] text-accent-hi">
                VERSION {version} READY
              </span>
              <span className="block text-[10px] text-text-dim">
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
          <Tile icon={<FolderPlus size={13} />} label="IMPORT" onClick={() => go("settings")} />
          <Tile icon={<Layers size={13} />} label="SAVED" onClick={() => go("saved")} />
          <Tile icon={<Settings2 size={13} />} label="SETTINGS" onClick={() => go("settings")} />
        </div>

        {appVersion && (
          <button
            type="button"
            onClick={() => go("about")}
            title="About Cursed"
            className="mono mt-2.5 block w-full text-center text-[9px] text-text-dim transition-colors duration-150 hover:text-text-muted"
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
      className={`display inline-flex items-center gap-1.5 rounded-full border px-2.5 py-1 text-[9px] ${
        tone === "accent"
          ? "border-accent/40 bg-accent-dim/50 text-accent-hi"
          : "border-border text-text-dim"
      }`}
    >
      {icon}
      <span className="max-w-28 truncate">{label}</span>
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
      className="panel flex flex-col items-center justify-center gap-1.5 rounded-sm border border-border py-2.5 text-text-dim transition-all duration-150 ease-[cubic-bezier(0.16,1,0.3,1)] hover:-translate-y-px hover:border-accent hover:text-text"
    >
      {icon}
      <span className="display text-[9px]">{label}</span>
    </button>
  );
}

/**
 * Two large, soft colour fields drifting slowly past each other.
 *
 * Blurred gradients rather than a pattern, because the point is depth, not
 * decoration — and because at this size a blur is a handful of composited
 * pixels, not something the GPU notices. Frozen by the `no-motion` class while a
 * live cursor preview is on screen, and by `prefers-reduced-motion`.
 */
function Aurora() {
  return (
    <div className="pointer-events-none absolute inset-0 overflow-hidden">
      <div
        className="absolute -top-24 -left-16 h-72 w-72 rounded-full opacity-50 blur-3xl"
        style={{
          background: "radial-gradient(circle, rgba(46,139,255,0.55), transparent 70%)",
          animation: "drift-a 18s ease-in-out infinite",
        }}
      />
      <div
        className="absolute -right-20 bottom-4 h-64 w-64 rounded-full opacity-40 blur-3xl"
        style={{
          background: "radial-gradient(circle, rgba(162,75,255,0.45), transparent 70%)",
          animation: "drift-b 22s ease-in-out infinite",
        }}
      />
      {/* Grounds the lower half so the buttons sit on something solid. */}
      <div
        className="absolute inset-x-0 bottom-0 h-1/2"
        style={{ background: "linear-gradient(to top, var(--color-bg), transparent)" }}
      />
    </div>
  );
}
