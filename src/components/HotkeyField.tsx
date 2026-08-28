import { useEffect, useRef, useState } from "react";
import * as ipc from "../lib/ipc";
import { cx } from "./ui";

/**
 * Set a shortcut by pressing it.
 *
 * **What it replaces.** These were free-text boxes. To change the toggle you had
 * to know that the app wanted the string `Ctrl+Alt+0` — not `CTRL+ALT+0`, not
 * `Control-Alt-0`, not `^⌥0` — and type it exactly. Get it wrong and nothing
 * said so: `hotkeys::register` skips an accelerator it cannot parse and carries
 * on, so a typo produced a shortcut that was displayed, saved, and dead. The
 * only way to find out was to press the keys and notice nothing happened.
 *
 * Now the keyboard is the input. Press the combination and it is captured
 * exactly as pressed.
 *
 * ## Why `event.code` and not `event.key`
 *
 * `key` is the character the layout produces, so on a French keyboard the key
 * in the position of QWERTY's `A` reports `q`, and on any layout `Shift+1`
 * reports `!`. Binding either would record a shortcut that stops matching the
 * moment somebody switches layout, or that names a character rather than a key.
 * `code` is the physical key and is what the backend's parser expects.
 *
 * ## Why a modifier is required
 *
 * These are *global* shortcuts — they are registered with Windows and fire
 * whatever has focus. A binding of `A` would swallow that letter in every other
 * application on the machine, and the app that did it would be the last place
 * anyone thought to look.
 */

const MODIFIERS = new Set([
  "ControlLeft",
  "ControlRight",
  "AltLeft",
  "AltRight",
  "ShiftLeft",
  "ShiftRight",
  "MetaLeft",
  "MetaRight",
]);

/** `KeyC` → `C`, `Digit4` → `4`, `F5` → `F5`, `ArrowUp` → `ArrowUp`. */
function keyName(code: string): string | null {
  if (MODIFIERS.has(code)) return null;
  if (/^Key[A-Z]$/.test(code)) return code.slice(3);
  if (/^Digit[0-9]$/.test(code)) return code.slice(5);
  if (/^Numpad[0-9]$/.test(code)) return code;
  if (/^F([1-9]|1[0-9]|2[0-4])$/.test(code)) return code;
  const named: Record<string, string> = {
    Space: "Space",
    Enter: "Enter",
    Tab: "Tab",
    Backquote: "`",
    Minus: "-",
    Equal: "=",
    BracketLeft: "[",
    BracketRight: "]",
    Backslash: "\\",
    Semicolon: ";",
    Quote: "'",
    Comma: ",",
    Period: ".",
    Slash: "/",
    ArrowUp: "Up",
    ArrowDown: "Down",
    ArrowLeft: "Left",
    ArrowRight: "Right",
    Home: "Home",
    End: "End",
    PageUp: "PageUp",
    PageDown: "PageDown",
    Insert: "Insert",
    Delete: "Delete",
  };
  return named[code] ?? null;
}

/** Builds the accelerator string the backend parses, in its own spelling. */
function accelerator(e: React.KeyboardEvent): string | null {
  const name = keyName(e.code);
  if (!name) return null;
  const parts: string[] = [];
  if (e.ctrlKey) parts.push("Ctrl");
  if (e.altKey) parts.push("Alt");
  if (e.shiftKey) parts.push("Shift");
  if (e.metaKey) parts.push("Super");
  if (parts.length === 0) return null;
  parts.push(name);
  return parts.join("+");
}

export function HotkeyField({
  value,
  onChange,
  label,
}: {
  value: string;
  onChange: (next: string) => void;
  /** Used for the accessible name, since the button's own text is the binding. */
  label: string;
}) {
  const [listening, setListening] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  const button = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    if (!listening) return;
    // Nothing else on the screen should act on keys while one is being captured
    // — pressing Ctrl+Alt+S must not also trigger whatever else is bound to it.
    const swallow = (e: KeyboardEvent) => {
      if (e.key !== "Tab") e.preventDefault();
    };
    window.addEventListener("keydown", swallow, true);
    return () => window.removeEventListener("keydown", swallow, true);
  }, [listening]);

  const capture = async (e: React.KeyboardEvent) => {
    if (e.key === "Escape") {
      setListening(false);
      setProblem(null);
      return;
    }
    // A shortcut you cannot remove is a shortcut you are stuck with.
    if (e.key === "Backspace" || e.key === "Delete") {
      e.preventDefault();
      setListening(false);
      setProblem(null);
      onChange("");
      return;
    }

    const next = accelerator(e);
    if (!next) {
      // Held modifiers on their own are the normal in-between state of pressing
      // a combination, so they are not an error — just keep listening.
      if (!MODIFIERS.has(e.code)) {
        setProblem("Hold Ctrl, Alt or Shift together with another key.");
      }
      return;
    }

    e.preventDefault();
    setListening(false);

    // Asked of the backend rather than assumed. This field builds the string,
    // but the parser that has to accept it lives on the other side.
    try {
      if (!(await ipc.hotkeyIsRegisterable(next))) {
        setProblem(`${next} is not a shortcut this build can register.`);
        return;
      }
    } catch {
      // The check is a courtesy; failing it must not block setting a shortcut.
    }
    setProblem(null);
    onChange(next);
  };

  return (
    <div>
      <button
        ref={button}
        type="button"
        aria-label={label}
        aria-keyshortcuts={value || undefined}
        onClick={() => {
          setListening(true);
          setProblem(null);
        }}
        onBlur={() => setListening(false)}
        onKeyDown={(e) => {
          if (!listening) return;
          void capture(e);
        }}
        className={cx(
          "mono h-8 w-full rounded-xs border px-2 text-left text-[12px] outline-none transition-colors duration-150",
          listening
            ? "border-accent bg-bg text-accent-hi"
            : "border-border bg-bg text-text hover:border-border-hi",
        )}
      >
        {listening ? "PRESS KEYS…" : value || "NOT SET"}
      </button>
      {listening && (
        <p className="mt-1 text-[10px] text-text-dim">
          Esc to cancel, Backspace to clear.
        </p>
      )}
      {problem && <p className="mt-1 text-[10px] text-danger">{problem}</p>}
    </div>
  );
}
