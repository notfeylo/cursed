import { useEffect } from "react";
import { TitleBar } from "./components/TitleBar";
import { Banner } from "./components/ui";
import { Home } from "./screens/Home";
import { Catalog } from "./screens/Catalog";
import { CustomImport } from "./screens/CustomImport";
import { Saved } from "./screens/Saved";
import { SettingsScreen } from "./screens/Settings";
import { About } from "./screens/About";
import { useStore } from "./store";

export default function App() {
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
