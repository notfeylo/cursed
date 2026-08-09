import { useState } from "react";
import {
  ArrowLeft,
  ArrowRight,
  Check,
  ChevronLeft,
  ChevronRight,
  Copy,
  Download,
  FileText,
  FolderOpen,
  FolderPlus,
  Info,
  Layers,
  Minus,
  Pencil,
  Plus,
  Scale,
  Search,
  Settings2,
  ShieldCheck,
  Sparkles,
  Star,
  Trash2,
  Upload,
  X,
} from "lucide-react";
import {
  Banner,
  Button,
  Card,
  Field,
  SectionTitle,
  Select,
  Slider,
  TextInput,
  Toggle,
} from "../components/ui";
import { Mark } from "../components/Mark";

/**
 * Every token, size, state and component on one scrollable page.
 *
 * Dev-only: reachable at `?specimen` under `npm run dev`, never in a build. The
 * point is that one screenshot answers every design question at once — whether a
 * font actually loaded, whether spacing is consistent, whether an icon is
 * missing, whether a long name overflows — instead of each being found one
 * screen at a time.
 *
 * It deliberately makes no IPC calls, so it renders in a plain browser tab at
 * any width.
 */

/* ── the scales ─────────────────────────────────────────────
   Nothing outside these values should appear anywhere in the app. Inconsistent
   spacing is the single biggest cause of an interface feeling cheap, and it is
   invisible until it is all laid out together like this.
   ──────────────────────────────────────────────────────── */

const TYPE_SCALE = [10, 11, 12, 14, 16, 20, 28, 40] as const;
const SPACE_SCALE = [4, 8, 12, 16, 24, 32, 48] as const;

const COLOURS: { token: string; value: string; note?: string }[] = [
  { token: "bg", value: "#050507", note: "window" },
  { token: "surface", value: "#0b0d12", note: "cards" },
  { token: "elevated", value: "#131722", note: "raised" },
  { token: "border", value: "#1e2532" },
  { token: "border-hi", value: "#2a3446", note: "hover" },
  { token: "accent", value: "#2e8bff" },
  { token: "accent-hi", value: "#5cb8ff" },
  { token: "accent-dim", value: "#0a2540" },
  { token: "text", value: "#edf1f7" },
  { token: "text-muted", value: "#8a94a6" },
  { token: "text-dim", value: "#58606e" },
  { token: "danger", value: "#ff4d5e" },
  { token: "success", value: "#33d6a6" },
];

const ICONS: { name: string; node: React.ReactNode }[] = [
  { name: "ArrowLeft", node: <ArrowLeft /> },
  { name: "ArrowRight", node: <ArrowRight /> },
  { name: "Check", node: <Check /> },
  { name: "ChevronLeft", node: <ChevronLeft /> },
  { name: "ChevronRight", node: <ChevronRight /> },
  { name: "Copy", node: <Copy /> },
  { name: "Download", node: <Download /> },
  { name: "FileText", node: <FileText /> },
  { name: "FolderOpen", node: <FolderOpen /> },
  { name: "FolderPlus", node: <FolderPlus /> },
  { name: "Info", node: <Info /> },
  { name: "Layers", node: <Layers /> },
  { name: "Minus", node: <Minus /> },
  { name: "Pencil", node: <Pencil /> },
  { name: "Plus", node: <Plus /> },
  { name: "Scale", node: <Scale /> },
  { name: "Search", node: <Search /> },
  { name: "Settings2", node: <Settings2 /> },
  { name: "ShieldCheck", node: <ShieldCheck /> },
  { name: "Sparkles", node: <Sparkles /> },
  { name: "Star", node: <Star /> },
  { name: "Trash2", node: <Trash2 /> },
  { name: "Upload", node: <Upload /> },
  { name: "X", node: <X /> },
];

/**
 * The shipping stack, and the runner-up kept for comparison.
 *
 * Only Geist is loaded from `/dev-fonts/`; the shipping faces come from the
 * app's own stylesheet, so this row also proves they are really loading.
 */
const PAIRINGS = [
  {
    id: "SHIPPING (A)",
    stack: "Space Grotesk / Inter Tight / JetBrains Mono",
    display: "'Space Grotesk', sans-serif",
    body: "'Inter Tight', sans-serif",
    mono: "'JetBrains Mono', monospace",
  },
  {
    id: "B — fallback if A fails a pass/fail check",
    stack: "Geist / Geist / Geist Mono",
    display: "'Geist', sans-serif",
    body: "'Geist', sans-serif",
    mono: "'Geist Mono', monospace",
  },
] as const;

/** The strings that break layouts, all in one place. */
const LONG_PRESET = "My Extremely Long Preset Name For Testing Truncation";
const LONG_PATH =
  "C:\\Users\\huzai\\AppData\\Roaming\\Cursed\\imported\\kuromi-naruto-crossover-akatsuki-cloak-kunai-dagger\\arrow-role.cur";
const LONG_ERROR =
  "The downloaded installer does not match the checksum published with the release, so it was deleted. Check your connection and try again, or download it manually from the releases page.";

/**
 * The candidate faces, registered at runtime from `/dev-fonts/`.
 *
 * Deliberately not in `styles.css` and not under `src/assets`: either would put
 * them through the bundler, and a stylesheet is always emitted even when the
 * component importing it is tree-shaken — so eight fonts nothing uses would end
 * up inside the installer. Vite serves project-root files in dev and copies
 * only `public/`, so this is available at `?specimen` and absent from a build.
 */
const CANDIDATE_FACES = (
  [
    ["Geist", 400, "geist-400"],
    ["Geist", 500, "geist-500"],
    ["Geist", 700, "geist-700"],
    ["Geist Mono", 400, "geist-mono-400"],
  ] as const
)
  .map(
    ([family, weight, file]) =>
      `@font-face{font-family:"${family}";src:url("/dev-fonts/${file}.woff2") format("woff2");font-weight:${weight};font-display:block}`,
  )
  .join("");

export function Specimen() {
  const [toggleA, setToggleA] = useState(true);
  const [toggleB, setToggleB] = useState(false);
  const [slider, setSlider] = useState(48);
  const [text, setText] = useState("");
  const [choice, setChoice] = useState<"a" | "b">("a");

  return (
    <div className="h-full overflow-y-auto bg-bg px-6 py-6 text-text">
      <style>{CANDIDATE_FACES}</style>
      <div className="mx-auto max-w-[900px] space-y-8">
        <header className="flex items-center gap-3 border-b border-border pb-4">
          <Mark size={44} animated id="specimen" />
          <div>
            <h1 className="display text-[20px] text-text">CURSED — SPECIMEN</h1>
            <p className="text-[12px] text-text-muted">
              Every token, size, state and component. Dev-only.
            </p>
          </div>
        </header>

        {/* ── colour ─────────────────────────────────────── */}
        <Block title="Colour tokens">
          <div className="grid grid-cols-2 gap-2 sm:grid-cols-3 md:grid-cols-4">
            {COLOURS.map((c) => (
              <div key={c.token} className="rounded-sm border border-border">
                <div className="h-12 rounded-t-sm" style={{ background: c.value }} />
                <div className="px-2 py-1.5">
                  <div className="mono text-[11px] text-text">{c.token}</div>
                  <div className="mono text-[10px] text-text-dim">{c.value}</div>
                  {c.note && <div className="text-[10px] text-text-dim">{c.note}</div>}
                </div>
              </div>
            ))}
          </div>
        </Block>

        {/* ── type ───────────────────────────────────────── */}
        <Block title="Type scale — display (Space Grotesk)">
          {TYPE_SCALE.map((size) => (
            <div key={size} className="flex items-baseline gap-4 border-b border-border/50 py-1.5">
              <span className="mono w-12 shrink-0 text-[10px] text-text-dim">{size}px</span>
              <span className="display truncate" style={{ fontSize: size }}>
                ENHANCE YOUR CURSOR
              </span>
            </div>
          ))}
        </Block>

        <Block title="Type scale — body (Inter Tight)">
          {TYPE_SCALE.map((size) => (
            <div key={size} className="flex items-baseline gap-4 border-b border-border/50 py-1.5">
              <span className="mono w-12 shrink-0 text-[10px] text-text-dim">{size}px</span>
              <span className="truncate" style={{ fontSize: size }}>
                Your pointer. Possessed. Sharp at every size.
              </span>
            </div>
          ))}
        </Block>

        <Block title="Type scale — mono (JetBrains Mono)">
          {TYPE_SCALE.map((size) => (
            <div key={size} className="flex items-baseline gap-4 border-b border-border/50 py-1.5">
              <span className="mono w-12 shrink-0 text-[10px] text-text-dim">{size}px</span>
              <span className="mono truncate" style={{ fontSize: size }}>
                v1.8.0 · 0123456789 · %APPDATA%\Cursed
              </span>
            </div>
          ))}
        </Block>

        <Block title="Font loading check">
          {/* If a self-hosted face failed to load, these fall back to a system
              font and the two lines below look identical. That is the classic
              silent bug this row exists to catch. */}
          <div className="space-y-1 text-[14px]">
            <p style={{ fontFamily: "'Space Grotesk', sans-serif" }}>
              Space Grotesk — the quick brown fox jumps over the lazy dog
            </p>
            <p style={{ fontFamily: "'Inter Tight', sans-serif" }}>
              Inter Tight — the quick brown fox jumps over the lazy dog
            </p>
            <p style={{ fontFamily: "'JetBrains Mono', monospace" }}>
              JetBrains Mono — the quick brown fox jumps over the lazy dog
            </p>
            <p style={{ fontFamily: "'ThisFontDoesNotExist', serif" }} className="text-text-dim">
              Deliberate fallback (serif) — if the three above match this, they did not load
            </p>
          </div>
        </Block>

        {/* ── font pairings ──────────────────────────────── */}
        <Block title="Font pairings — same words, three stacks">
          <p className="mb-3 text-[11px] text-text-dim">
            Identical content in each, at the sizes the home screen actually uses. Compare the
            display line first — it carries the app&apos;s character — then check the version
            string, which has to stay unambiguous at 10px.
          </p>
          <div className="space-y-3">
            {PAIRINGS.map((p) => (
              <div key={p.id} className="rounded-sm border border-border p-4">
                <div className="mb-3 flex items-baseline justify-between gap-3">
                  <span className="display text-[11px] text-accent-hi">{p.id}</span>
                  <span className="mono text-[10px] text-text-dim">{p.stack}</span>
                </div>

                <div
                  style={{
                    fontFamily: p.display,
                    fontWeight: 700,
                    textTransform: "uppercase",
                    letterSpacing: "0.08em",
                    fontSize: 28,
                    lineHeight: 1.05,
                  }}
                  className="text-text"
                >
                  Enhance your cursor
                </div>

                <p
                  style={{ fontFamily: p.body, fontSize: 14 }}
                  className="mt-2 text-text-muted"
                >
                  Your pointer. Possessed. Every one of the seventeen roles, sharp at any size,
                  with no added input latency.
                </p>

                <div className="mt-2 flex flex-wrap items-center gap-4">
                  <span style={{ fontFamily: p.mono, fontSize: 10 }} className="text-text-dim">
                    v1.8.0 · 0123456789 · %APPDATA%\Cursed
                  </span>
                  <span
                    style={{
                      fontFamily: p.display,
                      fontWeight: 700,
                      textTransform: "uppercase",
                      letterSpacing: "0.08em",
                      fontSize: 11,
                    }}
                    className="text-text"
                  >
                    Choose a cursor
                  </span>
                  <span style={{ fontFamily: p.body, fontSize: 12 }} className="text-text-muted">
                    Illegible pairs: Il1 · O0 · rn/m
                  </span>
                </div>
              </div>
            ))}
          </div>
        </Block>

        {/* ── spacing ────────────────────────────────────── */}
        <Block title="Spacing scale">
          {SPACE_SCALE.map((step) => (
            <div key={step} className="flex items-center gap-4 py-1">
              <span className="mono w-12 shrink-0 text-[10px] text-text-dim">{step}px</span>
              <span className="h-3 rounded-xs bg-accent" style={{ width: step * 4 }} />
              <span className="text-[11px] text-text-dim">
                {step === 4 ? "hairline gaps" : step === 48 ? "section breaks" : ""}
              </span>
            </div>
          ))}
        </Block>

        {/* ── buttons ────────────────────────────────────── */}
        <Block title="Buttons — every variant, every state">
          <p className="mb-2 text-[11px] text-text-dim">
            Hover and focus cannot be captured in a screenshot. Tab through this row to check the
            focus ring, and hover each to check the lift.
          </p>
          <div className="grid grid-cols-4 gap-3">
            {(["primary", "ghost", "danger", "quiet"] as const).map((variant) => (
              <div key={variant} className="space-y-2">
                <div className="mono text-[10px] text-text-dim">{variant}</div>
                <Button variant={variant} full>
                  DEFAULT
                </Button>
                <Button variant={variant} full disabled>
                  DISABLED
                </Button>
              </div>
            ))}
          </div>
        </Block>

        {/* ── controls ───────────────────────────────────── */}
        <Block title="Controls">
          <div className="grid gap-3 md:grid-cols-2">
            <Card>
              <Toggle
                checked={toggleA}
                onChange={setToggleA}
                label="Launch on Windows startup"
                hint="Starts minimised to the notification area"
              />
              <Toggle checked={toggleB} onChange={setToggleB} label="Off state, no hint" />
              <div className="pt-2">
                <Slider
                  value={slider}
                  min={32}
                  max={256}
                  step={8}
                  onChange={setSlider}
                  label="Cursor size"
                  suffix="px"
                />
              </div>
            </Card>
            <Card>
              <Field label="Preset name" hint="Shown in the tray quick-switch">
                <TextInput value={text} onChange={setText} placeholder="Untitled preset" />
              </Field>
              <Field label="Apply to">
                <Select
                  value={choice}
                  onChange={setChoice}
                  options={[
                    { value: "a", label: "Recommended (3 roles)" },
                    { value: "b", label: "All 17 roles" },
                  ]}
                />
              </Field>
            </Card>
          </div>
        </Block>

        {/* ── chips and badges ───────────────────────────── */}
        <Block title="Chips and badges">
          <div className="flex flex-wrap items-center gap-2">
            <span className="display rounded-full border border-border px-2.5 py-1 text-[10px] text-text-dim">
              205 CURSORS
            </span>
            <span className="display rounded-full border border-accent/40 bg-accent-dim/50 px-2.5 py-1 text-[10px] text-accent-hi">
              SKYRIM SET 2
            </span>
            <span className="display rounded-full border border-success/40 bg-success/10 px-2.5 py-1 text-[10px] text-success">
              UP TO DATE
            </span>
            <span className="display rounded-full border border-danger/40 bg-danger/10 px-2.5 py-1 text-[10px] text-danger">
              FILE MISSING
            </span>
            <span className="mono rounded-xs border border-border bg-bg px-1 py-px text-[11px] text-accent-hi">
              inline code
            </span>
          </div>
        </Block>

        {/* ── async states ───────────────────────────────── */}
        <Block title="Loading, empty and error — every async surface needs all three">
          <div className="grid gap-3 md:grid-cols-3">
            <Card>
              <div className="mono mb-2 text-[10px] text-text-dim">loading</div>
              <div className="space-y-2">
                <div className="h-2 w-1/3 rounded-full shimmer" />
                <div className="h-2 w-full rounded-full shimmer" />
                <div className="h-2 w-4/5 rounded-full shimmer" />
              </div>
            </Card>
            <Card>
              <div className="mono mb-2 text-[10px] text-text-dim">empty</div>
              <p className="text-[12px] text-text">No cursors match.</p>
              <p className="mt-1 text-[11px] text-text-dim">
                Clear the search, or import a folder of your own.
              </p>
            </Card>
            <Card>
              <div className="mono mb-2 text-[10px] text-text-dim">error</div>
              <Banner tone="error">Couldn&apos;t reach GitHub.</Banner>
            </Card>
          </div>
          <div className="mt-3">
            <Banner tone="success">Applied to all 17 roles.</Banner>
          </div>
        </Block>

        {/* ── icons ──────────────────────────────────────── */}
        <Block title={`Icons in use (${ICONS.length}) — 13px, the size the app uses`}>
          <div className="grid grid-cols-4 gap-2 sm:grid-cols-6 md:grid-cols-8">
            {ICONS.map((icon) => (
              <div
                key={icon.name}
                className="flex flex-col items-center gap-1.5 rounded-sm border border-border py-2.5"
              >
                <span className="text-text-muted [&>svg]:h-[13px] [&>svg]:w-[13px]">
                  {icon.node}
                </span>
                <span className="mono max-w-full truncate px-1 text-[10px] text-text-dim">
                  {icon.name}
                </span>
              </div>
            ))}
          </div>
          <p className="mt-2 text-[11px] text-text-dim">
            Any blank tile above is a missing or renamed icon.
          </p>
        </Block>

        <Block title="Icons at the three sizes used">
          <div className="flex items-end gap-6">
            {[13, 14, 15].map((size) => (
              <div key={size} className="flex items-center gap-2">
                <span className="mono text-[10px] text-text-dim">{size}px</span>
                <span
                  className="flex gap-2 text-text-muted"
                  style={{ ["--s" as string]: `${size}px` }}
                >
                  <Settings2 size={size} />
                  <Download size={size} />
                  <Search size={size} />
                  <Trash2 size={size} />
                </span>
              </div>
            ))}
          </div>
        </Block>

        {/* ── torture ────────────────────────────────────── */}
        <Block title="Long-text torture — nothing here may overflow its box">
          <div className="grid gap-3 md:grid-cols-2">
            <Card>
              <div className="mono mb-2 text-[10px] text-text-dim">
                60-char preset name, in a chip
              </div>
              <span
                className="display block max-w-full truncate rounded-full border border-accent/40 bg-accent-dim/50 px-2.5 py-1 text-[10px] text-accent-hi"
                title={LONG_PRESET}
              >
                {LONG_PRESET}
              </span>
              <div className="mono mt-3 mb-1 text-[10px] text-text-dim">…in a list row</div>
              <div className="flex items-center gap-2 rounded-sm border border-border px-2 py-1.5">
                <Star size={13} className="shrink-0 text-text-dim" />
                <span className="min-w-0 flex-1 truncate text-[12px]" title={LONG_PRESET}>
                  {LONG_PRESET}
                </span>
                <ChevronRight size={13} className="shrink-0 text-text-dim" />
              </div>
            </Card>
            <Card>
              <div className="mono mb-1 text-[10px] text-text-dim">deep file path</div>
              <p className="mono truncate text-[11px] text-text-dim" title={LONG_PATH}>
                {LONG_PATH}
              </p>
              <div className="mono mt-3 mb-1 text-[10px] text-text-dim">
                long error — wraps, never truncates
              </div>
              <p className="text-[11px] break-words text-danger">{LONG_ERROR}</p>
            </Card>
          </div>
        </Block>

        <Block title="Cards and surfaces">
          <div className="grid gap-3 md:grid-cols-3">
            <Card>
              <SectionTitle>Section title</SectionTitle>
              <p className="text-[12px] text-text-muted">
                Body copy inside a card, at 12px, the app&apos;s default.
              </p>
            </Card>
            <div className="rounded-sm border border-border bg-elevated p-3">
              <p className="text-[12px] text-text-muted">Elevated surface, no sheen.</p>
            </div>
            <div className="panel rounded-sm border border-accent p-3">
              <p className="text-[12px] text-text-muted">Panel with accent border (hover state).</p>
            </div>
          </div>
        </Block>

        <footer className="border-t border-border py-6 text-center">
          <p className="mono text-[10px] text-text-dim">
            End of specimen · screenshot this whole page at 100% and 200% scaling
          </p>
        </footer>
      </div>
    </div>
  );
}

function Block({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section>
      <h2 className="display mb-3 border-b border-border pb-1.5 text-[11px] text-accent-hi">
        {title}
      </h2>
      {children}
    </section>
  );
}
