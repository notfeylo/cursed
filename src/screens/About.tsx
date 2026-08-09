import { useEffect, useRef, useState } from "react";
import { ArrowLeft, ChevronRight, FileText, Scale, ShieldCheck } from "lucide-react";
import { ScreenHeader } from "../components/ScreenHeader";
import { Button, Card, SectionTitle } from "../components/ui";
import { Markdown, sections, slug } from "../components/Markdown";
import { Mark } from "../components/Mark";
import * as ipc from "../lib/ipc";

type Doc = "terms" | "privacy" | "licenses";

const DOCS: { kind: Doc; title: string; blurb: string; icon: React.ReactNode }[] = [
  {
    kind: "terms",
    title: "TERMS & CONDITIONS",
    blurb: "What the software changes, and what it will not do",
    icon: <Scale size={14} />,
  },
  {
    kind: "privacy",
    title: "PRIVACY POLICY",
    blurb: "What leaves this computer — and what never does",
    icon: <ShieldCheck size={14} />,
  },
  {
    kind: "licenses",
    title: "LICENCES",
    blurb: "Cursed, the bundled fonts, and every dependency",
    icon: <FileText size={14} />,
  },
];

const titleOf = (kind: Doc) => DOCS.find((d) => d.kind === kind)?.title ?? "";

export function About() {
  const [info, setInfo] = useState<ipc.BuildInfo>({
    version: "—",
    commit: "local",
    target: "x86_64",
    built: "—",
    windows: "—",
  });
  const [doc, setDoc] = useState<Doc | null>(null);

  useEffect(() => {
    if (!ipc.isDesktop()) return;
    void ipc.getBuildInfo().then(setInfo).catch(() => undefined);
  }, []);

  if (doc) return <DocView kind={doc} onBack={() => setDoc(null)} />;

  return (
    <div className="screen-in flex h-full flex-col">
      <ScreenHeader title="ABOUT" back="settings" />

      <div className="min-h-0 flex-1 overflow-y-auto px-3 py-4">
        <div className="flex flex-col items-center gap-2 pb-4">
          <Mark size={54} animated id="about" />
          <span className="display text-[14px] text-text">CURSED</span>
          <span className="mono text-[11px] text-accent-hi">v{info.version}</span>
          <span className="text-[11px] text-text-muted">Give your dead cursor a new life.</span>
        </div>

        <SectionTitle>Build</SectionTitle>
        <Card>
          {/* Everything a support conversation asks for, in one place that can
              be read aloud over a chat window. */}
          <dl className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 text-[11px]">
            <Row label="Version" value={`v${info.version}`} />
            <Row label="Built" value={info.built} />
            <Row label="Commit" value={info.commit} />
            <Row label="Architecture" value={info.target} />
            <Row label="Windows" value={info.windows} />
          </dl>
        </Card>

        <SectionTitle>Updates</SectionTitle>
        <UpdateStatusLine />

        <SectionTitle>Legal</SectionTitle>
        <div className="grid gap-1.5">
          {DOCS.map((d) => (
            <button
              key={d.kind}
              type="button"
              onClick={() => setDoc(d.kind)}
              className="panel group flex items-center gap-3 rounded-sm border border-border px-3 py-2.5 text-left tile hover:tile-hover"
            >
              <span className="grid h-8 w-8 shrink-0 place-items-center rounded-xs border border-border bg-bg text-text-dim transition-colors duration-150 group-hover:border-accent/50 group-hover:text-accent-hi">
                {d.icon}
              </span>
              <span className="min-w-0 flex-1">
                <span className="display block text-[10px] text-text">{d.title}</span>
                <span className="block truncate text-[10px] text-text-dim">{d.blurb}</span>
              </span>
              <ChevronRight
                size={14}
                className="shrink-0 text-text-dim transition-transform duration-150 group-hover:translate-x-0.5 group-hover:text-accent-hi"
              />
            </button>
          ))}
        </div>
        <p className="mt-2 px-1 text-[11px] text-text-dim">
          All three render from inside the app — no network, no browser.
        </p>

        <SectionTitle>Project</SectionTitle>
        <Card>
          <div className="grid grid-cols-2 gap-2">
            <Button
              variant="ghost"
              onClick={() =>
                void ipc.openExternal("https://github.com/notfeylo/cursed").catch(() => undefined)
              }
            >
              GITHUB
            </Button>
            <Button
              variant="ghost"
              onClick={() =>
                void ipc
                  .openExternal("https://github.com/notfeylo/cursed/issues")
                  .catch(() => undefined)
              }
            >
              REPORT A BUG
            </Button>
          </div>
          <p className="mt-3 text-[11px] text-text-dim">
            MIT licensed. © 2026 feylo. Cursed changes only per-user pointer settings
            and is not affiliated with Microsoft.
          </p>
        </Card>
      </div>
    </div>
  );
}

function Row({ label, value }: { label: string; value: string }) {
  return (
    <>
      <dt className="text-text-dim">{label}</dt>
      <dd className="mono min-w-0 truncate text-right text-text" title={value}>
        {value}
      </dd>
    </>
  );
}

/**
 * Update state in plain language, and never a dead end.
 *
 * The full download-and-install flow lives in Settings; this is the answer to
 * "am I up to date?", which is the question people actually open About to ask.
 * It reports an outcome in every case, including the failure — a check that
 * silently does nothing is worse than no check.
 */
function UpdateStatusLine() {
  const [state, setState] = useState<ipc.UpdateState | null>(null);
  const [checking, setChecking] = useState(false);

  const read = () => void ipc.getUpdateState().then(setState).catch(() => undefined);

  useEffect(() => {
    if (!ipc.isDesktop()) return;
    read();
    const timer = window.setInterval(read, 4000);
    return () => window.clearInterval(timer);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const check = async () => {
    setChecking(true);
    try {
      await ipc.startUpdateCheck();
    } catch {
      /* the state below reports the failure */
    } finally {
      window.setTimeout(() => {
        setChecking(false);
        read();
      }, 1200);
    }
  };

  const busy = checking || state?.checking === true;
  const latest = state?.status?.latest;
  const newer = state?.status?.newerAvailable === true;

  const line = busy
    ? "Checking…"
    : state?.error
      ? state.error
      : state?.ready
        ? `${latest} downloaded and verified — install it in Settings`
        : newer
          ? `${latest} is available — download it in Settings`
          : state?.status
            ? "You're on the latest version."
            : "Not checked yet.";

  const tone = state?.error
    ? "text-danger"
    : newer || state?.ready
      ? "text-accent-hi"
      : state?.status
        ? "text-success"
        : "text-text-dim";

  return (
    <Card>
      <p className={`text-[11px] break-words ${tone}`}>{line}</p>
      <div className="mt-2 grid grid-cols-2 gap-2">
        <Button variant="ghost" onClick={() => void check()} disabled={busy}>
          {busy ? "CHECKING" : "CHECK NOW"}
        </Button>
        <Button
          variant="ghost"
          onClick={() =>
            void ipc
              .openExternal("https://github.com/notfeylo/cursed/releases/latest")
              .catch(() => undefined)
          }
        >
          DOWNLOAD MANUALLY
        </Button>
      </div>
    </Card>
  );
}

/**
 * One legal document, read inside the app.
 *
 * Two things make a wall of legal text usable in a 420px window: knowing how
 * much is left, and being able to skip to the clause you came for. Hence the
 * progress rule under the header and the numbered section index — everything
 * else is the renderer's job.
 */
function DocView({ kind, onBack }: { kind: Doc; onBack: () => void }) {
  const [text, setText] = useState("");
  const [progress, setProgress] = useState(0);
  const scroller = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    if (!ipc.isDesktop()) return;
    setText("");
    void ipc
      .getLegalDoc(kind)
      .then(setText)
      .catch(() => setText("This document could not be loaded."));
  }, [kind]);

  const onScroll = () => {
    const el = scroller.current;
    if (!el) return;
    const span = el.scrollHeight - el.clientHeight;
    setProgress(span > 8 ? Math.min(1, el.scrollTop / span) : 1);
  };

  const jump = (id: string) => {
    const target = scroller.current?.querySelector(`#${CSS.escape(id)}`);
    target?.scrollIntoView({ behavior: "smooth", block: "start" });
  };

  const index = text ? sections(text) : [];

  return (
    <div className="screen-in flex h-full flex-col">
      <div className="sticky top-0 z-10 border-b border-border bg-bg/95 backdrop-blur">
        <div className="flex items-center gap-2 px-3 py-2">
          <button
            type="button"
            onClick={onBack}
            title="Back to About"
            className="grid h-6 w-6 shrink-0 place-items-center rounded-xs text-text-dim transition-colors duration-150 hover:bg-elevated hover:text-text"
          >
            <ArrowLeft size={13} />
          </button>
          <span className="display min-w-0 flex-1 truncate text-[11px] text-text">
            {titleOf(kind)}
          </span>
        </div>
        {/* How much is left. A legal document with no end in sight goes unread. */}
        <div className="h-px w-full bg-border">
          <div
            className="h-px bg-accent transition-[width] duration-100 ease-linear"
            style={{ width: `${Math.round(progress * 100)}%` }}
          />
        </div>
      </div>

      {index.length >= 3 && (
        <div className="border-b border-border px-3 py-2">
          <div className="flex gap-1.5 overflow-x-auto pb-0.5">
            {index.map((s) => (
              <button
                key={s.id}
                type="button"
                title={s.text}
                onClick={() => jump(slug(s.text))}
                className="mono max-w-28 shrink-0 truncate rounded-full border border-border px-2 py-0.5 text-[10px] text-text-dim transition-colors duration-150 hover:border-accent hover:text-accent-hi"
              >
                {/^\d+\./.test(s.text) ? s.text.split(".")[0] : s.text}
              </button>
            ))}
          </div>
        </div>
      )}

      <div
        ref={scroller}
        onScroll={onScroll}
        className="min-h-0 flex-1 overflow-y-auto px-4 py-3"
      >
        <Markdown source={text} />
        <div className="h-6" />
      </div>
    </div>
  );
}
