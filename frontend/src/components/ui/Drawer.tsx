import { type ReactNode, type RefObject } from "react";
import { X } from "lucide-react";
import { cn } from "./cn";
import { useDialogFocusLifecycle } from "./useDialogFocusLifecycle";

interface DrawerProps {
  open: boolean;
  side: "left" | "right";
  title: string;
  onClose: () => void;
  returnFocusRef?: RefObject<HTMLElement>;
  showHeader?: boolean;
  children: ReactNode;
}

export default function Drawer({
  open,
  side,
  title,
  onClose,
  returnFocusRef,
  showHeader = true,
  children,
}: DrawerProps) {
  const panelRef = useDialogFocusLifecycle({ open, onClose, returnFocusRef });

  if (!open) return null;

  return (
    <div className="fixed inset-0 z-50 flex" role="presentation">
      <div
        className="absolute inset-0 bg-[var(--backdrop)]"
        aria-hidden="true"
        onClick={onClose}
      />
      <div
        ref={panelRef}
        role="dialog"
        aria-modal="true"
        aria-label={title}
        tabIndex={-1}
        className={cn(
          "shadow-float-token relative z-10 flex h-full w-[min(88vw,360px)] flex-col border-[var(--border-soft)] bg-[var(--surface-solid)] text-[var(--text)] outline-none transition-[background-color,border-color,box-shadow] duration-[var(--motion-fast)]",
          side === "left" ? "mr-auto border-r" : "ml-auto border-l"
        )}
        onClick={(event) => event.stopPropagation()}
      >
        {showHeader && (
          <div className="flex h-14 shrink-0 items-center justify-between border-b border-[var(--border-soft)] px-4">
            <h2 className="text-sm font-semibold">{title}</h2>
            <button
              type="button"
              onClick={onClose}
              className="rounded-[var(--radius-sm)] p-2 text-[var(--text-secondary)] transition-colors duration-[var(--motion-fast)] hover:bg-[var(--surface-hover)] hover:text-[var(--text)]"
              aria-label={`关闭${title}`}
            >
              <X className="h-4 w-4" />
            </button>
          </div>
        )}
        <div className="min-h-0 flex-1 overflow-hidden">{children}</div>
      </div>
    </div>
  );
}
