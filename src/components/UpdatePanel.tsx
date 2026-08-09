import { useEffect, useState } from "react";
import { Button, Card } from "./ui";
import * as ipc from "../lib/ipc";

type Phase = "idle" | "checking" | "current" | "available" | "downloading" | "ready" | "failed";

/**
 * The whole update flow, in one place.
 *
 * Lives in Settings rather than buried under About: an update nobody can find
 * is an update nobody installs. `autoCheck` runs a check as soon as the panel
 * mounts, so the common case needs no clicks at all.
 */
export function UpdatePanel({ autoCheck = false }: { autoCheck?: boolean }) {
  const [phase, setPhase] = useState<Phase>("idle");
  const [status, setStatus] = useState<ipc.UpdateStatus | null>(null);
  const [message, setMessage] = useState<string | null>(null);

  const check = async () => {
    setPhase("checking");
    setMessage(null);
    try {
      const next = await ipc.checkForUpdates();
      setStatus(next);
      setPhase(next.newerAvailable ? "available" : "current");
    } catch (e) {
      setPhase("failed");
      setMessage(e instanceof Error ? e.message : "Couldn't reach GitHub.");
    }
  };

  // The background updater usually got there first, so adopt whatever it found
  // rather than starting a second check and making the user wait again.
  useEffect(() => {
    if (!autoCheck || !ipc.isDesktop()) return;

    let cancelled = false;
    const read = async () => {
      try {
        const bg = await ipc.getUpdateState();
        if (cancelled) return;

        if (bg.status) setStatus(bg.status);
        if (bg.error) setMessage(bg.error);

        if (bg.ready) setPhase("ready");
        else if (bg.downloading) setPhase("downloading");
        else if (bg.checking) setPhase("checking");
        else if (bg.status) setPhase(bg.status.newerAvailable ? "available" : "current");
        else void check();
      } catch {
        /* falls back to the manual button */
      }
    };

    void read();
    const timer = window.setInterval(read, 3000);
    return () => {
      cancelled = true;
      window.clearInterval(timer);
    };
    // Intentionally runs once on mount.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [autoCheck]);

  const download = async () => {
    if (!status?.latest || !status.installer) return;
    setPhase("downloading");
    setMessage(null);
    try {
      await ipc.downloadUpdate(status.latest, status.installer);
      setPhase("ready");
    } catch (e) {
      setPhase("failed");
      setMessage(e instanceof Error ? e.message : "The download failed.");
    }
  };

  const install = async () => {
    if (!status?.latest || !status.installer) return;
    setMessage(null);
    try {
      // Verifies the download against the checksum published with the release
      // before running it, then closes the app so the installer can replace it.
      await ipc.installUpdate(status.latest, status.installer);
    } catch (e) {
      setPhase("failed");
      setMessage(e instanceof Error ? e.message : "The installer could not be verified.");
    }
  };

  const offering = phase === "available" || phase === "downloading" || phase === "ready";

  return (
    <Card>
      {offering ? (
        <div className="space-y-2">
          <div className="flex items-baseline justify-between gap-2">
            <span className="display text-[11px] text-accent-hi">
              VERSION {status?.latest} AVAILABLE
            </span>
            {status?.size ? (
              <span className="mono shrink-0 text-[10px] text-text-dim">
                {(status.size / 1_048_576).toFixed(1)} MB
              </span>
            ) : null}
          </div>

          {status?.notes && (
            <p className="max-h-28 overflow-y-auto text-[11px] whitespace-pre-line text-text-muted">
              {status.notes}
            </p>
          )}

          {phase === "ready" ? (
            <>
              <Button full onClick={() => void install()}>
                INSTALL & RESTART
              </Button>
              <p className="text-[11px] text-text-dim">
                The download is checked against the checksum published with the release before
                it runs. Cursed closes so the installer can replace it.
              </p>
            </>
          ) : (
            <>
              <Button full onClick={() => void download()} disabled={phase === "downloading"}>
                {phase === "downloading" ? "DOWNLOADING" : "DOWNLOAD UPDATE"}
              </Button>
              {phase === "downloading" && <div className="h-px w-full shimmer" />}
            </>
          )}
        </div>
      ) : (
        <>
          <div className="mb-2 flex items-baseline justify-between">
            <span className="text-[12px] text-text">
              {phase === "current" ? "Up to date" : "Updates"}
            </span>
            {status?.current && (
              <span className="mono text-[10px] text-text-dim">v{status.current}</span>
            )}
          </div>
          <Button
            full
            variant={phase === "current" ? "ghost" : "primary"}
            onClick={() => void check()}
            disabled={phase === "checking"}
          >
            {phase === "checking" ? "CHECKING" : "CHECK FOR UPDATES"}
          </Button>
          {phase === "checking" && <div className="mt-2 h-px w-full shimmer" />}
          {phase === "current" && (
            <p className="mt-2 text-center text-[11px] text-success">
              You're on the latest version.
            </p>
          )}
        </>
      )}

      {message && (
        <div className="mt-2 space-y-1.5">
          <p className="text-center text-[11px] break-words text-danger">{message}</p>
          {/* Never a dead end. Whatever went wrong — no network, a proxy, an
              antivirus holding the installer — the releases page still works in
              a browser, so the user is one click from the same file. */}
          <Button
            full
            variant="ghost"
            onClick={() =>
              void ipc
                .openExternal("https://github.com/notfeylo/cursed/releases/latest")
                .catch(() => undefined)
            }
          >
            DOWNLOAD IT MANUALLY
          </Button>
          <p className="text-center text-[10px] text-text-dim">
            Opens the releases page in your browser.
          </p>
        </div>
      )}
    </Card>
  );
}
