import { invoke } from "@tauri-apps/api/core";
import { ROLES } from "./types";
import type {
  ActiveState,
  ApplyMode,
  BuiltCursor,
  ImportedImage,
  PackSummary,
  Preset,
  Role,
  Settings,
} from "./types";

/**
 * The whole IPC surface, in one place. Every call is typed and every error
 * arrives as a plain string the UI can show — the Rust side returns
 * Result<T, AppError> and never panics.
 */

export class IpcError extends Error {}

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(cmd, args);
  } catch (e) {
    throw new IpcError(typeof e === "string" ? e : JSON.stringify(e));
  }
}

/** True when running inside the Tauri shell (false in a plain browser dev tab). */
export const isDesktop = (): boolean =>
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

/* ── settings & state ──────────────────────────────────────── */

/**
 * Tells the backend the UI has painted, so it can show the window.
 *
 * The window starts hidden so nobody sees an unpainted rectangle. If this never
 * arrives the backend shows the window anyway after a couple of seconds — a
 * broken frontend must not leave a running process with no window.
 */
export const frontendReady = () => call<void>("frontend_ready");

export const getSettings = () => call<Settings>("get_settings");
export const saveSettings = (settings: Settings) =>
  call<Settings>("save_settings", { settings });
export const getActiveState = () => call<ActiveState>("get_active_state");

/* ── catalog ───────────────────────────────────────────────── */

export const listPacks = () => call<PackSummary[]>("list_packs");

export interface ApplyArgs {
  packId: string;
  tint: string;
  size: number;
  outline: boolean;
  applyMode: ApplyMode;
}

/** Live layer only: SetSystemCursor, no registry write. */
export const previewPack = (args: ApplyArgs) => call<void>("preview_pack", { args });
/** Drops the live layer and reloads whatever the registry says. */
export const clearPreview = () => call<void>("clear_preview");
/** Commits: registry scheme + broadcast + live layer. */
export const applyPack = (args: ApplyArgs) => call<void>("apply_pack", { args });

export const restoreWindowsDefault = () => call<void>("restore_windows_default");
export const getCursorBaseSize = () => call<number>("get_cursor_base_size");

/* ── presets ───────────────────────────────────────────────── */

export const listPresets = () => call<Preset[]>("list_presets");
export const savePreset = (preset: Preset) => call<Preset>("save_preset", { preset });
export const deletePreset = (id: string) => call<void>("delete_preset", { id });
export const applyPreset = (id: string) => call<void>("apply_preset", { id });
export const setDefaultPreset = (id: string) => call<void>("set_default_preset", { id });
export const duplicatePreset = (id: string) => call<Preset>("duplicate_preset", { id });
export const exportPreset = (id: string, dest: string) =>
  call<string>("export_preset", { id, dest });
export const importCfpack = (src: string) => call<Preset>("import_cfpack", { src });

/* ── custom import ─────────────────────────────────────────── */

export const importImage = (path: string) => call<ImportedImage>("import_image", { path });
export const importImageBytes = (bytes: number[]) =>
  call<ImportedImage>("import_image_bytes", { bytes });

export interface BuildArgs {
  token: string;
  name: string;
  hotspot: [number, number];
  outline: boolean;
  animationSpeed: number;
}

export const previewCustom = (token: string, outline: boolean) =>
  call<{ size: number; dataUri: string }[]>("preview_custom", { token, outline });

export const buildCustomCursor = (args: BuildArgs) =>
  call<BuiltCursor>("build_custom_cursor", { args });

export const deleteCustomCursor = (id: string) =>
  call<void>("delete_custom_cursor", { id });

export interface ApplyCustomArgs {
  cursorId: string;
  applyMode: ApplyMode;
  /** Pack used to fill the remaining roles when applyMode is "Blend". */
  blendPackId: string | null;
  tint: string;
  size: number;
  outline: boolean;
}

export const applyCustomCursor = (args: ApplyCustomArgs) =>
  call<void>("apply_custom_cursor", { args });

/* ── advanced / about ──────────────────────────────────────── */

export const getStorageDir = () => call<string>("get_storage_dir");
export const openStorageDir = () => call<void>("open_storage_dir");
export const getCacheSize = () => call<number>("get_cache_size");
export const clearCache = () => call<number>("clear_cache");
export const getLegalDoc = (kind: "terms" | "privacy" | "licenses") =>
  call<string>("get_legal_doc", { kind });
export const getBuildInfo = () =>
  call<{ version: string; commit: string; target: string }>("get_build_info");
export interface UpdateStatus {
  current: string;
  latest: string | null;
  newerAvailable: boolean;
  /** Present only when the release ships an installer we can verify. */
  installer: string | null;
  size: number | null;
  notes: string | null;
  url: string;
}

export const checkForUpdates = () => call<UpdateStatus>("check_for_updates");

export const downloadUpdate = (version: string, installer: string) =>
  call<number>("download_update", { version, installer });

/**
 * Verifies the downloaded installer against the checksum published with the
 * release, then launches it and exits. Throws rather than running anything
 * whose hash does not match.
 */
export const installUpdate = (version: string, installer: string) =>
  call<void>("install_update", { version, installer });

export const clearUpdateDownloads = () => call<void>("clear_update_downloads");

/* ── importing your own cursors ────────────────────────────── */

export interface ImportReport {
  imported: number;
  skipped: number;
  problems: string[];
  names: string[];
}

export interface ImportedPack {
  id: string;
  name: string;
  category: string;
  animated: boolean;
  source: string;
  imported: string;
}

/** Scans a folder for .cur/.ani/.png/.zip cursors and installs what it finds. */
export const importCursorFolder = (folder: string) =>
  call<ImportReport>("import_cursor_folder", { folder });

export const listImported = () => call<ImportedPack[]>("list_imported");
export const deleteImported = (id: string) => call<void>("delete_imported", { id });
export const deleteAllImported = () => call<void>("delete_all_imported");
export const openExternal = (url: string) => call<void>("open_external", { url });
export const quitApp = () => call<void>("quit_app");
export const hideToTray = () => call<void>("hide_to_tray");

/* ── role helpers ──────────────────────────────────────────── */

/** Which roles an apply mode touches, for the "what will change" copy. */
export const rolesForMode = (mode: ApplyMode): readonly Role[] =>
  mode === "Recommended"
    ? (["Arrow", "Hand", "Crosshair"] as const)
    : mode === "All"
      ? ROLES
      : (["Arrow"] as const);
