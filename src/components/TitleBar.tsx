import { Minus, X } from "lucide-react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { isDesktop } from "../lib/ipc";
import { useStore } from "../store";
import { Mark } from "./Mark";

/** Re-exported so screens can pull the mark from either module. */
export { Mark as CursorMark } from "./Mark";

/**
 * Frameless chrome. The drag region is the whole bar minus the two buttons —
 * `data-tauri-drag-region` is handled natively, so dragging never round-trips
 * through JS.
 */
export function TitleBar() {
  const view = useStore((s) => s.view);

  const minimize = async () => {
    if (isDesktop()) await getCurrentWindow().minimize();
  };

  const close = async () => {
    // Honouring "close minimizes to tray" is the backend's decision, not ours:
    // it owns the setting and the tray. We just ask the window to close.
    if (isDesktop()) await getCurrentWindow().close();
  };

  return (
    <header
      data-tauri-drag-region
      className="flex h-9 shrink-0 items-center justify-between border-b border-border bg-bg/80 pl-3 select-none"
    >
      <div data-tauri-drag-region className="flex items-center gap-2">
        <Mark size={15} id="tb" />
        <span data-tauri-drag-region className="display text-[10px] text-text-muted">
          CURSORFORGE
          {view !== "home" && <span className="text-text-dim"> / {view}</span>}
        </span>
      </div>
      <div className="flex h-full">
        <button
          type="button"
          onClick={minimize}
          aria-label="Minimize"
          className="grid h-full w-11 place-items-center text-text-dim transition-colors duration-150 hover:bg-elevated hover:text-text"
        >
          <Minus size={13} strokeWidth={2} />
        </button>
        <button
          type="button"
          onClick={close}
          aria-label="Close"
          className="grid h-full w-11 place-items-center text-text-dim transition-colors duration-150 hover:bg-danger hover:text-white"
        >
          <X size={13} strokeWidth={2} />
        </button>
      </div>
    </header>
  );
}

