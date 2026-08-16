/**
 * Shapes mirrored 1:1 from the Rust side. The frontend never invents a
 * registry key or a file path — it names a Role variant and nothing else.
 */

export const ROLES = [
  "Arrow",
  "Help",
  "AppStarting",
  "Wait",
  "Crosshair",
  "IBeam",
  "NWPen",
  "No",
  "SizeNS",
  "SizeWE",
  "SizeNWSE",
  "SizeNESW",
  "SizeAll",
  "UpArrow",
  "Hand",
  "Pin",
  "Person",
] as const;

export type Role = (typeof ROLES)[number];

export const ROLE_LABELS: Record<Role, string> = {
  Arrow: "Normal select",
  Help: "Help select",
  AppStarting: "Working in background",
  Wait: "Busy",
  Crosshair: "Precision select",
  IBeam: "Text select",
  NWPen: "Handwriting",
  No: "Unavailable",
  SizeNS: "Vertical resize",
  SizeWE: "Horizontal resize",
  SizeNWSE: "Diagonal resize 1",
  SizeNESW: "Diagonal resize 2",
  SizeAll: "Move",
  UpArrow: "Alternate select",
  Hand: "Link select",
  Pin: "Location select",
  Person: "Person select",
};

export type Category = "OPTIMAL CURSED" | "MINIMAL CURSED";

/**
 * MINIMAL CURSED is deliberately empty for now — a named, empty shelf is
 * clearer than guessing which cursors belong on it.
 */
export const CATEGORIES: Category[] = ["OPTIMAL CURSED", "MINIMAL CURSED"];

export interface PackSummary {
  id: string;
  name: string;
  category: Category;
  author: string;
  recolorable: boolean;
  animated: boolean;
  /** Inline SVG preview of the Arrow role, already tinted by the backend. */
  preview: string;
}

/** Which roles a cursor is applied to. */
export type ApplyMode = "ArrowOnly" | "Recommended" | "All" | "Blend";

export interface Preset {
  id: string;
  name: string;
  created: string;
  basePack: string;
  overrides: Partial<Record<Role, string>>;
  tint: string;
  size: number;
  outline: boolean;
  hotkey: string | null;
  isDefault: boolean;
}

export interface Settings {
  launchOnStartup: boolean;
  startMinimized: boolean;
  closeToTray: boolean;
  showTrayIcon: boolean;
  autoCheckUpdates: boolean;

  cursorSize: number | null;
  tint: string;
  outline: boolean;
  /** Whether the size control moves the link hand and the text I-beam too. */
  scaleAllRoles: boolean;
  applyMode: ApplyMode;
  blendPack: string;
  /** Recolour catalog tiles to the tint. Off by default — see Settings. */
  tintPreviews: boolean;
  animationSpeed: number;
  reapplyOnResume: boolean;

  watchdogEnabled: boolean;
  watchdogIntervalSecs: number;
  reapplyAfterThemeChange: boolean;

  hotkeyToggle: string;
  hotkeyOpen: string;
  hotkeyPresets: string[];

  debugLogging: boolean;
  firstRunDone: boolean;
  /** Set once the user dismisses the lost-original-scheme notice. */
  schemeLossAcknowledged: boolean;
}

export interface ActiveState {
  packId: string | null;
  packName: string | null;
  tint: string;
  size: number;
  isDefault: boolean;
}

export interface BuiltCursor {
  /** Absolute path under %APPDATA%\Cursed — opaque to the frontend. */
  id: string;
  name: string;
  animated: boolean;
  frames: number;
  hotspot: [number, number];
  /** data: URI previews at 1:1 for each generated size. */
  previews: { size: number; dataUri: string }[];
}

export interface ImportedImage {
  token: string;
  width: number;
  height: number;
  animated: boolean;
  frameCount: number;
  /** data: URI of the trimmed source, for the hotspot picker. */
  dataUri: string;
  suggestedHotspot: [number, number];
}
