import { ChevronLeft } from "lucide-react";
import type { ReactNode } from "react";
import { useStore } from "../store";

export function ScreenHeader({
  title,
  children,
  back = "home",
}: {
  title: string;
  children?: ReactNode;
  back?: Parameters<ReturnType<typeof useStore.getState>["go"]>[0];
}) {
  const go = useStore((s) => s.go);
  return (
    <div className="sticky top-0 z-10 border-b border-border bg-bg/95 px-3 py-2 backdrop-blur">
      <div className="flex items-center gap-2">
        <button
          type="button"
          onClick={() => go(back)}
          aria-label="Back"
          className="grid h-7 w-7 shrink-0 place-items-center rounded-xs text-text-muted transition-colors duration-150 hover:bg-elevated hover:text-text"
        >
          <ChevronLeft size={16} />
        </button>
        <span className="display flex-1 truncate text-[11px] text-text">{title}</span>
        {children}
      </div>
    </div>
  );
}
