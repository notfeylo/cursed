import { useEffect, useState } from "react";
import { ScreenHeader } from "../components/ScreenHeader";
import { Button, Card, SectionTitle } from "../components/ui";
import { Mark } from "../components/Mark";
import * as ipc from "../lib/ipc";

type Doc = "terms" | "privacy" | "licenses";

const DOC_TITLES: Record<Doc, string> = {
  terms: "TERMS & CONDITIONS",
  privacy: "PRIVACY POLICY",
  licenses: "LICENCES",
};

type Phase = "idle" | "checking" | "current" | "available" | "downloading" | "ready" | "failed";

export function About() {
  const [info, setInfo] = useState({ version: "1.0.0", commit: "local", target: "x86_64" });
  const [doc, setDoc] = useState<Doc | null>(null);
  const [text, setText] = useState("");

  const [phase, setPhase] = useState<Phase>("idle");
  const [status, setStatus] = useState<ipc.UpdateStatus | null>(null);
  const [message, setMessage] = useState<string | null>(null);

  useEffect(() => {
    if (!ipc.isDesktop()) return;
    void ipc.getBuildInfo().then(setInfo).catch(() => undefined);
  }, []);

  useEffect(() => {
    if (!doc || !ipc.isDesktop()) return;
    setText("");
    void ipc
      .getLegalDoc(doc)
      .then(setText)
      .catch(() => setText("This document could not be loaded."));
  }, [doc]);

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

  if (doc) {
    return (
      <div className="screen-in flex h-full flex-col">
        <div className="sticky top-0 z-10 border-b border-border bg-bg/95 px-3 py-2 backdrop-blur">
          <div className="flex items-center gap-2">
            <button
              type="button"
              onClick={() => setDoc(null)}
              className="display text-[10px] text-text-dim hover:text-text"
            >
              ← BACK
            </button>
            <span className="display flex-1 truncate text-[11px] text-text">
              {DOC_TITLES[doc]}
            </span>
          </div>
        </div>
        <div className="min-h-0 flex-1 overflow-y-auto px-4 py-3">
          <Markdown source={text} />
        </div>
      </div>
    );
  }

  return (
    <div className="screen-in flex h-full flex-col">
      <ScreenHeader title="ABOUT" back="settings" />

      <div className="min-h-0 flex-1 overflow-y-auto px-3 py-4">
        <div className="flex flex-col items-center gap-2 pb-4">
          <Mark size={54} animated id="about" />
          <span className="display text-[13px] text-text">CURSORFORGE</span>
          <span className="mono text-[10px] text-text-dim">
            v{info.version} · {info.commit} · {info.target}
          </span>
          <span className="text-[11px] text-text-muted">Give your dead cursor a new life.</span>
        </div>

        <Card>
          {phase === "available" || phase === "downloading" || phase === "ready" ? (
            <div className="space-y-2">
              <div className="flex items-baseline justify-between">
                <span className="display text-[11px] text-accent-hi">
                  VERSION {status?.latest} AVAILABLE
                </span>
                {status?.size ? (
                  <span className="mono text-[10px] text-text-dim">
                    {(status.size / 1_048_576).toFixed(1)} MB
                  </span>
                ) : null}
              </div>

              {status?.notes && (
                <p className="max-h-24 overflow-y-auto text-[11px] whitespace-pre-line text-text-muted">
                  {status.notes}
                </p>
              )}

              {phase === "ready" ? (
                <>
                  <Button full onClick={() => void install()}>
                    INSTALL & RESTART
                  </Button>
                  <p className="text-[11px] text-text-dim">
                    The download is checked against the checksum published with the release
                    before it runs. CursorForge will close so the installer can replace it.
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
              <Button full variant="ghost" onClick={() => void check()} disabled={phase === "checking"}>
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
            <p className="mt-2 text-center text-[11px] text-danger">{message}</p>
          )}
        </Card>

        <SectionTitle>Legal</SectionTitle>
        <Card>
          <div className="grid gap-1">
            {(Object.keys(DOC_TITLES) as Doc[]).map((kind) => (
              <button
                key={kind}
                type="button"
                onClick={() => setDoc(kind)}
                className="display rounded-xs px-2 py-2 text-left text-[10px] text-text-muted transition-colors duration-150 hover:bg-elevated hover:text-text"
              >
                {DOC_TITLES[kind]}
              </button>
            ))}
          </div>
          <p className="mt-2 text-[11px] text-text-dim">
            All three render from inside the app — no network, no browser.
          </p>
        </Card>

        <SectionTitle>Project</SectionTitle>
        <Card>
          <div className="grid grid-cols-2 gap-2">
            <Button
              variant="ghost"
              onClick={() =>
                void ipc.openExternal("https://github.com/notfeylo/cursorforge").catch(() => undefined)
              }
            >
              GITHUB
            </Button>
            <Button
              variant="ghost"
              onClick={() =>
                void ipc
                  .openExternal("https://github.com/notfeylo/cursorforge/issues")
                  .catch(() => undefined)
              }
            >
              REPORT A BUG
            </Button>
          </div>
          <p className="mt-3 text-[11px] text-text-dim">
            MIT licensed. © 2026 feylo. CursorForge changes only per-user pointer settings
            and is not affiliated with Microsoft.
          </p>
        </Card>
      </div>
    </div>
  );
}

/**
 * A deliberately small Markdown renderer.
 *
 * These three documents are ours and ship inside the binary, so the job is
 * headings, lists, tables and emphasis — not arbitrary HTML. Rendering to
 * elements rather than `innerHTML` means there is no path from a document to
 * script execution, whatever a future edit adds to them.
 */
function Markdown({ source }: { source: string }) {
  if (!source) return <div className="h-px w-24 shimmer" />;

  const blocks = source.split("\n");
  return (
    <div className="space-y-2 text-[12px] leading-relaxed text-text-muted">
      {blocks.map((line, index) => {
        const key = `${index}`;
        if (line.startsWith("# ")) {
          return (
            <h1 key={key} className="display mt-2 text-[13px] text-text">
              {strip(line.slice(2))}
            </h1>
          );
        }
        if (line.startsWith("## ")) {
          return (
            <h2 key={key} className="display mt-4 text-[11px] text-text">
              {strip(line.slice(3))}
            </h2>
          );
        }
        if (line.startsWith("|")) {
          return (
            <p key={key} className="mono text-[10px] text-text-dim">
              {strip(line.replace(/\|/g, " "))}
            </p>
          );
        }
        if (line.startsWith("- ") || line.startsWith("> ")) {
          return (
            <p key={key} className="pl-3 text-text-muted">
              {line.startsWith("- ") ? "· " : ""}
              {strip(line.slice(2))}
            </p>
          );
        }
        if (line.trim() === "") return <div key={key} className="h-1" />;
        return <p key={key}>{strip(line)}</p>;
      })}
    </div>
  );
}

const strip = (text: string) => text.replace(/[*_`]/g, "");
