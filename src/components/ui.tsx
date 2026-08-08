import type { ReactNode } from "react";

/* Shared primitives. Deliberately few — six screens do not need a component library. */

const cx = (...parts: (string | false | null | undefined)[]) =>
  parts.filter(Boolean).join(" ");

export function Button({
  children,
  onClick,
  variant = "primary",
  disabled,
  full,
  title,
}: {
  children: ReactNode;
  onClick?: () => void;
  variant?: "primary" | "ghost" | "danger" | "quiet";
  disabled?: boolean;
  full?: boolean;
  title?: string;
}) {
  const base =
    "display relative overflow-hidden text-[11px] px-4 h-9 rounded-sm transition-all duration-150 ease-[cubic-bezier(0.16,1,0.3,1)] disabled:opacity-40 disabled:pointer-events-none inline-flex items-center justify-center gap-2 active:scale-[0.985]";
  const kind =
    variant === "primary"
      ? // A gradient plus a top rim reads as a lit surface rather than a
        // rectangle of colour, which is most of what makes a button feel solid.
        "bg-gradient-to-b from-accent-hi to-accent text-white shadow-[inset_0_1px_0_rgba(255,255,255,0.25)] hover:-translate-y-px hover:glow"
      : variant === "danger"
        ? "border border-danger/40 text-danger hover:bg-danger/10 hover:border-danger"
        : variant === "quiet"
          ? "text-text-muted hover:text-text"
          : "panel border border-border text-text-muted hover:border-border-hi hover:text-text hover:-translate-y-px";
  return (
    <button
      type="button"
      title={title}
      onClick={onClick}
      disabled={disabled}
      className={cx(base, kind, full && "w-full")}
    >
      {children}
    </button>
  );
}

export function Toggle({
  checked,
  onChange,
  label,
  hint,
}: {
  checked: boolean;
  onChange: (next: boolean) => void;
  label: string;
  hint?: string;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      onClick={() => onChange(!checked)}
      className="flex w-full items-center gap-3 py-2 text-left"
    >
      <span className="min-w-0 flex-1">
        <span className="block text-[12px] text-text">{label}</span>
        {hint && <span className="block text-[11px] text-text-dim">{hint}</span>}
      </span>
      <span
        className={cx(
          "relative h-[18px] w-8 shrink-0 rounded-full transition-colors duration-150",
          checked ? "bg-accent" : "bg-border",
        )}
      >
        <span
          className={cx(
            "absolute top-[2px] h-[14px] w-[14px] rounded-full bg-white transition-[left] duration-150 ease-[cubic-bezier(0.16,1,0.3,1)]",
            checked ? "left-4" : "left-[2px]",
          )}
        />
      </span>
    </button>
  );
}

export function Slider({
  value,
  min,
  max,
  step = 1,
  onChange,
  label,
  suffix,
}: {
  value: number;
  min: number;
  max: number;
  step?: number;
  onChange: (next: number) => void;
  label?: string;
  suffix?: string;
}) {
  return (
    <div className="w-full">
      {label && (
        <div className="mb-1 flex items-baseline justify-between">
          <span className="text-[11px] text-text-muted">{label}</span>
          <span className="mono text-[11px] text-accent-hi">
            {value}
            {suffix}
          </span>
        </div>
      )}
      <input
        type="range"
        min={min}
        max={max}
        step={step}
        value={value}
        onChange={(e) => onChange(Number(e.currentTarget.value))}
        className="h-1 w-full cursor-default appearance-none rounded-full bg-border accent-[#2E8BFF] outline-none"
      />
    </div>
  );
}

export function Field({
  label,
  children,
  hint,
}: {
  label: string;
  children: ReactNode;
  hint?: string;
}) {
  return (
    <label className="block py-2">
      <span className="mb-1 block text-[11px] text-text-muted">{label}</span>
      {children}
      {hint && <span className="mt-1 block text-[11px] text-text-dim">{hint}</span>}
    </label>
  );
}

export function TextInput({
  value,
  onChange,
  placeholder,
  mono,
  maxLength,
}: {
  value: string;
  onChange: (next: string) => void;
  placeholder?: string;
  mono?: boolean;
  maxLength?: number;
}) {
  return (
    <input
      value={value}
      maxLength={maxLength}
      placeholder={placeholder}
      onChange={(e) => onChange(e.currentTarget.value)}
      className={cx(
        "h-8 w-full rounded-xs border border-border bg-bg px-2 text-[12px] text-text outline-none transition-colors duration-150 placeholder:text-text-dim focus:border-accent",
        mono && "mono",
      )}
    />
  );
}

export function Select<T extends string>({
  value,
  options,
  onChange,
}: {
  value: T;
  options: { value: T; label: string }[];
  onChange: (next: T) => void;
}) {
  return (
    <select
      value={value}
      onChange={(e) => onChange(e.currentTarget.value as T)}
      className="h-8 w-full rounded-xs border border-border bg-bg px-2 text-[12px] text-text outline-none focus:border-accent"
    >
      {options.map((o) => (
        <option key={o.value} value={o.value} className="bg-surface">
          {o.label}
        </option>
      ))}
    </select>
  );
}

export function SectionTitle({ children }: { children: ReactNode }) {
  return (
    <h2 className="display mt-5 mb-1 text-[10px] text-text-dim first:mt-0">{children}</h2>
  );
}

export function Card({
  children,
  className,
}: {
  children: ReactNode;
  className?: string;
}) {
  return (
    <div className={cx("panel rounded-sm border border-border p-3", className)}>{children}</div>
  );
}

export function Banner({
  tone,
  children,
  onDismiss,
}: {
  tone: "error" | "success";
  children: ReactNode;
  onDismiss?: () => void;
}) {
  return (
    <div
      className={cx(
        "flex items-start gap-2 rounded-xs border px-3 py-2 text-[11px]",
        tone === "error"
          ? "border-danger/40 bg-danger/10 text-danger"
          : "border-success/40 bg-success/10 text-success",
      )}
    >
      <span className="min-w-0 flex-1 break-words">{children}</span>
      {onDismiss && (
        <button type="button" onClick={onDismiss} className="shrink-0 opacity-70">
          ✕
        </button>
      )}
    </div>
  );
}
