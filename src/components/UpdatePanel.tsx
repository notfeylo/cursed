import { useEffect, useState } from "react";
import { Button, Card } from "./ui";
import { Markdown } from "./Markdown";
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

  const [progress, setProgress] = useState<{ got: number; total: number } | null>(null);
  const [installed, setInstalled] = useState<ipc.InstallOutcome | null>(null);
  /** What the version they are now running changed. Read from the binary. */
  const [whatsNew, setWhatsNew] = useState<string | null>(null);
  const [showWhatsNew, setShowWhatsNew] = useState(false);

  // How the *last* update went, read once whether or not this panel polls.
  //
  // Separate from the poll below because that only runs with `autoCheck`, and
  // the one thing worth saying after an update that did not take is worth
  // saying wherever the user happens to open the panel.
  useEffect(() => {
    if (!ipc.isDesktop()) return;
    let cancelled = false;
    void ipc
      .getUpdateState()
      .then(async (s) => {
        if (cancelled) return;
        setInstalled(s.installed);

        // Only after an update that took. Nobody wants a changelog for a
        // version they have been running for a fortnight.
        if (s.installed?.outcome !== "succeeded") return;
        const notes = await ipc.getReleaseNotes(s.installed.to).catch(() => null);
        if (!cancelled && notes) setWhatsNew(notes);
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, []);

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

  // The backend owns update state; this only mirrors it.
  //
  // The manual buttons used to keep their own phase while this poll ran every
  // three seconds off a state the buttons never wrote to. Pressing Download set
  // a local "ready", the next poll read the shared state — which had never heard
  // of that download — and put the Download button straight back. The action
  // worked perfectly and looked exactly like nothing happening, forever.
  //
  // Both paths now write the same state, so there is nothing left to disagree.
  // That claim was half true for a long time: downloading wrote the shared
  // state and *checking did not*, so a manual check was reverted by the next
  // tick of this poll — the update appeared for a second and a half and then
  // the app went back to insisting it was up to date. `check_for_updates` now
  // records its answer, which is what makes reading the phase from here safe.
  useEffect(() => {
    if (!autoCheck || !ipc.isDesktop()) return;

    let cancelled = false;
    // A check started because the shared state was empty, not because anyone
    // asked. Exactly one, ever: this runs every 1.5 seconds, and a check that
    // fires whenever there is nothing recorded is an unauthenticated request to
    // GitHub's API forty times a minute — which reaches the hourly rate limit in
    // under two minutes and then fails every check for the rest of the hour.
    let openingCheck = false;

    const read = async () => {
      try {
        const bg = await ipc.getUpdateState();
        if (cancelled) return;

        if (bg.status) setStatus(bg.status);
        setProgress(bg.downloading ? { got: bg.downloaded, total: bg.total } : null);

        // Only ever *adds* an error. This used to assign `bg.error ?? null`,
        // which cleared whatever the buttons had just put there: a failed
        // manual check wrote its reason, and the poll wiped it within a second
        // and a half, leaving a button that visibly did nothing and said
        // nothing about why.
        if (bg.error) setMessage(bg.error);

        if (bg.ready) setPhase("ready");
        else if (bg.downloading) setPhase("downloading");
        else if (bg.checking) setPhase("checking");
        else if (bg.status) setPhase(bg.status.newerAvailable ? "available" : "current");
        else if (!openingCheck) {
          openingCheck = true;
          void check();
        }
      } catch {
        /* falls back to the manual button */
      }
    };

    void read();
    const timer = window.setInterval(read, 1500);
    return () => {
      cancelled = true;
      window.clearInterval(timer);
    };
    // Intentionally runs once on mount.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [autoCheck]);

  const download = async () => {
    if (!status?.latest || !status.installer) {
      // Nothing to download is a state worth saying out loud. Silently doing
      // nothing on a click is indistinguishable from a broken button.
      setMessage("This release has no installer to download. Use the manual link below.");
      setPhase("failed");
      return;
    }
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
    if (!status?.latest || !status.installer) {
      setMessage("There is no downloaded installer to run. Try Download again.");
      setPhase("failed");
      return;
    }
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

  const mb = (bytes: number) => (bytes / 1_048_576).toFixed(1);
  const downloadingLabel =
    progress && progress.total > 0
      ? `DOWNLOADING ${mb(progress.got)} / ${mb(progress.total)} MB`
      : "DOWNLOADING";

  return (
    <Card>
      {/* What happened last time, before anything about what happens next.
          An update that silently did not take looks exactly like an update
          that was never started, and leaving the user to notice the version
          never changed is how an updater loses their trust for good. */}
      {installed?.outcome === "succeeded" && (
        <div className="mb-2">
          <p className="text-center text-[11px] text-success">Updated to v{installed.to}.</p>
          {/* Offered rather than unfolded. An update landing on someone who
              only opened Settings to change a hotkey should not push the thing
              they came for off the screen. */}
          {whatsNew && (
            <button
              type="button"
              onClick={() => setShowWhatsNew((open) => !open)}
              className="mx-auto mt-1 block text-[11px] text-text-dim hover:text-text hover:underline"
            >
              {showWhatsNew ? "Hide what changed" : "See what changed"}
            </button>
          )}
          {whatsNew && showWhatsNew && (
            <div className="mt-2 max-h-56 overflow-y-auto rounded-xs border border-border bg-bg p-2">
              <Markdown source={whatsNew} />
            </div>
          )}
        </div>
      )}
      {installed?.outcome === "didNotTake" && (
        <p className="mb-2 text-center text-[11px] break-words text-danger">
          The update to v{installed.to} didn't finish, so you're still on v{installed.from}.
          Your presets and cursors were not touched.
        </p>
      )}

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

          {/* A release that publishes nothing this PC can install. Rare, and
              worth saying plainly: the alternative is a Download button whose
              only outcome is an error message. */}
          {!status?.installer && phase === "available" ? (
            <>
              <p className="text-[11px] text-text-muted">
                That release doesn&apos;t include an installer for this PC&apos;s processor.
                The download page has every build.
              </p>
              <Button
                full
                onClick={() =>
                  void ipc.openExternal("https://cursorforge.vercel.app/").catch(() => undefined)
                }
              >
                OPEN THE DOWNLOAD PAGE
              </Button>
            </>
          ) : phase === "ready" ? (
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
                {phase === "downloading" ? downloadingLabel : "DOWNLOAD UPDATE"}
              </Button>
              {/* A real bar, not a shimmer. Eleven megabytes on a slow line
                  spent minutes showing one animated pixel, which is
                  indistinguishable from a frozen app — and being unable to tell
                  those apart is most of what "nothing happens" means. */}
              {phase === "downloading" &&
                (progress && progress.total > 0 ? (
                  <div className="h-px w-full bg-border">
                    <div
                      className="h-px bg-accent transition-[width] duration-200 ease-out"
                      style={{ width: `${Math.min(100, (progress.got / progress.total) * 100)}%` }}
                    />
                  </div>
                ) : (
                  <div className="h-px w-full shimmer" />
                ))}
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
          {/* Trying again is the right answer more often than not — most of
              what reaches this line is a network that misbehaved — and it must
              be offered in the app, because telling somebody their update
              failed and handing them only a browser link is how an updater
              stops being one. */}
          <Button
            full
            variant="ghost"
            onClick={() => {
              setMessage(null);
              if (status?.latest && status.installer && phase !== "ready") void download();
              else void check();
            }}
            disabled={phase === "downloading" || phase === "checking"}
          >
            TRY AGAIN
          </Button>
          {/* Never a dead end. Whatever went wrong — no network, a proxy, an
              antivirus holding the installer — the download page still works in
              a browser, so the user is one click from the same file. */}
          <Button
            full
            variant="ghost"
            onClick={() =>
              void ipc.openExternal("https://cursorforge.vercel.app/").catch(() => undefined)
            }
          >
            DOWNLOAD IT MANUALLY
          </Button>
          <p className="text-center text-[10px] text-text-dim">
            Opens the download page in your browser.
          </p>
        </div>
      )}
    </Card>
  );
}
