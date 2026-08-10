import { useEffect } from "react";
import { TitleBar } from "./components/TitleBar";
import { Banner } from "./components/ui";
import { Home } from "./screens/Home";
import { Catalog } from "./screens/Catalog";
import { Customise } from "./screens/Customise";
import { CustomImport } from "./screens/CustomImport";
import { Saved } from "./screens/Saved";
import { SettingsScreen } from "./screens/Settings";
import { About } from "./screens/About";
import { Specimen } from "./screens/Specimen";
import { useStore } from "./store";
import { useGlideScroll } from "./lib/useGlideScroll";
import backdrop from "./assets/backdrop.png";

/**
 * Dev-only, and absent from any build.
 *
 * `import.meta.env.DEV` is a compile-time constant, so the bundler drops this
 * branch — and the `Specimen` import with it — from a production build. Reached
 * at `http://localhost:1420/?specimen` under `npm run dev`.
 *
 * Kept out of the app's own navigation on purpose: it is a reference sheet for
 * whoever is working on the design, not a screen anyone ships.
 */
const SHOW_SPECIMEN =
  import.meta.env.DEV &&
  typeof window !== "undefined" &&
  window.location.search.includes("specimen");

export default function App() {
  if (SHOW_SPECIMEN) return <Specimen />;
  return <MainApp />;
}

function MainApp() {
  const view = useStore((s) => s.view);
  const ready = useStore((s) => s.ready);
  const error = useStore((s) => s.error);
  const previewing = useStore((s) => s.previewing);
  const bootstrap = useStore((s) => s.bootstrap);
  const setError = useStore((s) => s.setError);

  useEffect(() => {
    void bootstrap();
  }, [bootstrap]);

  // Eased wheel scrolling for every scrollable panel, installed once.
  useGlideScroll();

  return (
    <div
      className={`relative flex h-full flex-col overflow-hidden rounded-md border border-border bg-bg ${
        previewing ? "no-motion" : ""
      }`}
    >
      {/* The backdrop belongs to the window, not to the home screen.
          Everything above it is translucent, so the whole app shares one
          surface instead of the title bar being a black strip laid over a
          themed page. */}
      <Backdrop />

      <TitleBar />

      {error && (
        <div className="px-3 pt-2">
          <Banner tone="error" onDismiss={() => setError(null)}>
            {error}
          </Banner>
        </div>
      )}

      <main className="relative min-h-0 flex-1 overflow-hidden">
        {!ready ? (
          <Booting />
        ) : view === "home" ? (
          <Home />
        ) : view === "catalog" ? (
          <Catalog />
        ) : view === "customise" ? (
          <Customise />
        ) : view === "custom" ? (
          <CustomImport />
        ) : view === "saved" ? (
          <Saved />
        ) : view === "settings" ? (
          <SettingsScreen />
        ) : (
          <About />
        )}
      </main>
    </div>
  );
}

function Booting() {
  return (
    <div className="grid h-full place-items-center">
      <div className="h-px w-24 shimmer" />
    </div>
  );
}

/**
 * The supplied artwork, held still, behind the entire app.
 *
 * Tinted toward the brand blue rather than left neutral grey, and darkened by a
 * vignette so type and controls keep their contrast wherever a fold happens to
 * be bright. Nothing animates: this window sits in the tray all day, and a
 * moving background composites forever for the ten seconds anyone looks at it.
 *
 * The source arrived as a 659 KB screenshot. Blurred greyscale survives being
 * halved and flattened to one channel with nothing visible lost — 126 KB, and it
 * is scaled to fill the window regardless.
 */
function Backdrop() {
  return (
    <div className="pointer-events-none absolute inset-0 overflow-hidden">
      <img
        src={backdrop}
        alt=""
        aria-hidden="true"
        className="absolute inset-0 h-full w-full object-cover opacity-[0.5]"
      />
      <div
        className="absolute inset-0 mix-blend-color"
        style={{ background: "linear-gradient(160deg, #2e8bff 0%, #123a72 100%)" }}
      />
      <div
        className="absolute inset-0"
        style={{
          background:
            "radial-gradient(120% 85% at 50% 35%, rgba(5,5,7,0.42) 0%, rgba(5,5,7,0.86) 60%, rgba(5,5,7,0.95) 100%)",
        }}
      />
    </div>
  );
}
