import { create } from "zustand";
import * as ipc from "./lib/ipc";
import type { ActiveState, PackSummary, Preset, Settings } from "./lib/types";

export type View = "home" | "catalog" | "custom" | "saved" | "settings" | "about";

export const DEFAULT_SETTINGS: Settings = {
  launchOnStartup: true,
  startMinimized: true,
  closeToTray: true,
  showTrayIcon: true,
  autoCheckUpdates: true,

  cursorSize: null,
  tint: "#2E8BFF",
  outline: true,
  applyMode: "Blend",
  blendPack: "precision-gap-cross",
  tintPreviews: false,
  animationSpeed: 1,
  reapplyOnResume: true,

  watchdogEnabled: true,
  watchdogIntervalSecs: 5,
  reapplyAfterThemeChange: true,

  hotkeyToggle: "Ctrl+Alt+0",
  hotkeyOpen: "Ctrl+Alt+C",
  hotkeyPresets: ["Ctrl+Alt+1", "Ctrl+Alt+2", "Ctrl+Alt+3", "Ctrl+Alt+4", "Ctrl+Alt+5"],

  debugLogging: false,
  firstRunDone: false,
};

const DEFAULT_ACTIVE: ActiveState = {
  packId: null,
  packName: null,
  tint: "#2E8BFF",
  size: 32,
  isDefault: true,
};

interface Store {
  view: View;
  ready: boolean;
  /** Set while a live SetSystemCursor preview is on screen — freezes motion. */
  previewing: boolean;
  error: string | null;

  settings: Settings;
  active: ActiveState;
  packs: PackSummary[];
  presets: Preset[];

  go: (view: View) => void;
  setError: (error: string | null) => void;
  setPreviewing: (previewing: boolean) => void;

  bootstrap: () => Promise<void>;
  patchSettings: (patch: Partial<Settings>) => Promise<void>;
  refreshActive: () => Promise<void>;
  refreshPresets: () => Promise<void>;
}

export const useStore = create<Store>((set, get) => ({
  view: "home",
  ready: false,
  previewing: false,
  error: null,

  settings: DEFAULT_SETTINGS,
  active: DEFAULT_ACTIVE,
  packs: [],
  presets: [],

  go: (view) => set({ view }),
  setError: (error) => set({ error }),
  setPreviewing: (previewing) => set({ previewing }),

  bootstrap: async () => {
    if (!ipc.isDesktop()) {
      set({ ready: true });
      return;
    }
    try {
      const [settings, active, packs, presets] = await Promise.all([
        ipc.getSettings(),
        ipc.getActiveState(),
        ipc.listPacks(),
        ipc.listPresets(),
      ]);
      set({ settings, active, packs, presets, ready: true });
    } catch (e) {
      // A failed load still gets a window: the user needs to see the error and
      // reach Settings, not stare at a process with no UI.
      set({ ready: true, error: e instanceof Error ? e.message : String(e) });
    } finally {
      void ipc.frontendReady().catch(() => undefined);
    }
  },

  patchSettings: async (patch) => {
    const next = { ...get().settings, ...patch };
    set({ settings: next });
    if (!ipc.isDesktop()) return;
    try {
      set({ settings: await ipc.saveSettings(next) });
    } catch (e) {
      set({ error: e instanceof Error ? e.message : String(e) });
    }
  },

  refreshActive: async () => {
    if (!ipc.isDesktop()) return;
    try {
      set({ active: await ipc.getActiveState() });
    } catch {
      /* a stale active chip is not worth an error banner */
    }
  },

  refreshPresets: async () => {
    if (!ipc.isDesktop()) return;
    try {
      set({ presets: await ipc.listPresets() });
    } catch (e) {
      set({ error: e instanceof Error ? e.message : String(e) });
    }
  },
}));
