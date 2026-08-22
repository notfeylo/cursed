import { useCallback, useEffect, useState } from "react";
import { Button, Card } from "./ui";
import * as ipc from "../lib/ipc";

const mb = (bytes: number) => (bytes / 1_048_576).toFixed(1);

/**
 * Photo mode: install it, see what it costs, remove it.
 *
 * The backend had all of this — download, checksum, signature, progress,
 * removal — and nothing in the app could reach any of it. A feature that exists
 * only as five `#[tauri::command]`s is a feature nobody has.
 *
 * The panel owns none of the state it shows. Everything is read from the
 * backend on a poll while a download is running, for the same reason the
 * updater does it: the download outlives any component, and a progress bar
 * driven from local state shows nothing at all if the user navigates away and
 * comes back.
 */
export function PhotoModePanel() {
  const [status, setStatus] = useState<ipc.PhotoStatus | null>(null);
  const [progress, setProgress] = useState<ipc.PhotoProgress | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const refresh = useCallback(async () => {
    try {
      setStatus(await ipc.getPhotoStatus());
    } catch {
      /* the panel simply does not appear */
    }
  }, []);

  useEffect(() => {
    if (!ipc.isDesktop()) return;
    void refresh();
  }, [refresh]);

  // Only while something is in flight. A poll that runs forever is a wake-up
  // every second for a feature most people install once and never think about.
  useEffect(() => {
    if (!ipc.isDesktop() || !progress?.running) return;
    let cancelled = false;
    const tick = async () => {
      try {
        const next = await ipc.getPhotoProgress();
        if (cancelled) return;
        setProgress(next);
        if (next.error) setMessage(next.error);
        if (!next.running) {
          await refresh();
          setBusy(false);
        }
      } catch {
        /* the next tick tries again */
      }
    };
    const timer = window.setInterval(tick, 700);
    return () => {
      cancelled = true;
      window.clearInterval(timer);
    };
  }, [progress?.running, refresh]);

  const install = async () => {
    setBusy(true);
    setMessage(null);
    try {
      await ipc.installPhotoMode();
      // Marked running locally so the poll above starts on this render rather
      // than after the first tick — the download begins immediately and a bar
      // that appears a second late reads as a button that did nothing.
      setProgress({ running: true, received: 0, total: 0, installed: false, error: null });
    } catch (e) {
      setBusy(false);
      setMessage(e instanceof Error ? e.message : "The download could not be started.");
    }
  };

  const remove = async () => {
    setBusy(true);
    try {
      const freed = await ipc.removePhotoMode();
      setMessage(`Removed. ${mb(freed)} MB reclaimed.`);
      await refresh();
    } catch (e) {
      setMessage(e instanceof Error ? e.message : "It could not be removed.");
    } finally {
      setBusy(false);
    }
  };

  if (!status) return null;

  // Nothing to offer and nothing to fix: an architecture with no build, or the
  // offline installer, which exists precisely so a machine with no network
  // works. Saying why beats a button that always fails.
  if (!status.available) {
    return (
      <Card>
        <p className="text-[11px] text-text-muted">{status.unavailableReason}</p>
      </Card>
    );
  }

  const downloading = progress?.running === true;

  return (
    <Card>
      <div className="mb-2 flex items-baseline justify-between gap-2">
        <span className="text-[12px] text-text">Photo mode</span>
        <span className="mono shrink-0 text-[10px] text-text-dim">
          {status.installed ? `${mb(status.installedBytes)} MB on disk` : `${mb(status.downloadBytes)} MB`}
        </span>
      </div>

      <p className="mb-2 text-[11px] leading-relaxed text-text-muted">
        The ordinary background remover works on flat backgrounds — logos, icons, screenshots.
        Photo mode adds a second one that can cut a person, a car or a pet out of a real
        photograph. It is a one-time download and it runs entirely on this machine.
      </p>

      {status.installed ? (
        <Button full variant="ghost" onClick={() => void remove()} disabled={busy}>
          {busy ? "REMOVING" : "REMOVE PHOTO MODE"}
        </Button>
      ) : (
        <>
          <Button full onClick={() => void install()} disabled={busy || downloading}>
            {downloading ? "DOWNLOADING" : `DOWNLOAD PHOTO MODE (${mb(status.downloadBytes)} MB)`}
          </Button>
          {downloading && (
            <>
              {/* A real bar wherever the server declared a length, for the same
                  reason the updater has one: twenty megabytes on a slow line is
                  minutes of a shimmer that cannot be told apart from a hang. */}
              {progress && progress.total > 0 ? (
                <div className="mt-2 h-px w-full bg-border">
                  <div
                    className="h-px bg-accent transition-[width] duration-200 ease-out"
                    style={{
                      width: `${Math.min(100, (progress.received / progress.total) * 100)}%`,
                    }}
                  />
                </div>
              ) : (
                <div className="mt-2 h-px w-full shimmer" />
              )}
              <p className="mt-1 text-center text-[10px] text-text-dim">
                {progress && progress.total > 0
                  ? `${mb(progress.received)} / ${mb(progress.total)} MB`
                  : "Starting…"}
              </p>
              <Button
                full
                variant="ghost"
                onClick={() => void ipc.cancelPhotoInstall().catch(() => undefined)}
              >
                CANCEL
              </Button>
            </>
          )}
        </>
      )}

      {message && <p className="mt-2 text-[11px] break-words text-text-muted">{message}</p>}

      <p className="mt-2 text-[11px] text-text-dim">
        Checked against its published checksum and signature before it is loaded. Nothing is
        uploaded: the image never leaves this machine.
      </p>
    </Card>
  );
}
