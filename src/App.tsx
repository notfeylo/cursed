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

  return (
    <div
      className={`flex h-full flex-col overflow-hidden rounded-md border border-border bg-bg ${
        previewing ? "no-motion" : ""
      }`}
    >
      <TitleBar />

      {error && (
        <div className="px-3 pt-2">
          <Banner tone="error" onDismiss={() => setError(null)}>
            {error}
          </Banner>
        </div>
      )}

      <main className="min-h-0 flex-1 overflow-hidden">
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
