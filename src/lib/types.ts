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

/**
 * What the hand — the pointer Windows shows over a link — is made of.
 *
 * `Pack` is what every cursor did before this existed: the pack's own hand
 * artwork, which for a lot of the catalog is a second, unrelated drawing.
 */
export type HoverStyle = "Pack" | "Pointer" | "Mark";

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
  hoverStyle: HoverStyle;
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
  /** Whether the size control moves the link hand and the text cursor too. */
  scaleAllRoles: boolean;
  applyMode: ApplyMode;
  /** What the link hand is. */
  hoverStyle: HoverStyle;
  blendPack: string;
  /** Recolor catalog tiles to the tint. Off by default — see Settings. */
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
  /** Fraction of the image the background removal took, 0-1. */
  backgroundRemoved: number;
  /**
   * The image arrived with its background already gone. Not a refusal and not
   * a failure: there was nothing to remove. Every `.cur`, `.ani` and cut-out
   * PNG lands here.
   */
  alreadyTransparent: boolean;
  /**
   * Present when removal was declined, with the sentence to show. The preview
   * is then exactly what was imported -- nothing was changed.
   */
  refusal: string | null;
  /**
   * Whether an automatic attempt is worth offering. `false` means leading with
   * a retry just produces the same refusal for the same reason.
   */
  keyable: boolean;
}
