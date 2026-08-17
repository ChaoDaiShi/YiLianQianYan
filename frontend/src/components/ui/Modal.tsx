import { useEffect, useId, useRef } from "react";
import { X } from "lucide-react";
import { cn } from "./cn";
import Button from "./Button";

interface ModalProps {
  open: boolean;
  onClose: () => void;
  title: string;
  children: React.ReactNode;
  className?: string;
  footer?: React.ReactNode;
}

export default function Modal({ open, onClose, title, children, className, footer }: ModalProps) {
  const panelRef = useRef<HTMLDivElement>(null);
  const previousFocusRef = useRef<HTMLElement | null>(null);
  const titleId = useId();

  useEffect(() => {
    if (!open) return;

    previousFocusRef.current =
      document.activeElement instanceof HTMLElement ? document.activeElement : null;

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      event.preventDefault();
      onClose();
    };

    window.addEventListener("keydown", handleKeyDown);
    panelRef.current?.focus();

    return () => {
      window.removeEventListener("keydown", handleKeyDown);
      previousFocusRef.current?.focus();
    };
  }, [onClose, open]);

  if (!open) return null;
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4" role="presentation">
      <div
        className="absolute inset-0 bg-[var(--backdrop)] backdrop-blur-sm animate-fade-in"
        aria-hidden="true"
        onClick={onClose}
      />
      <div
        ref={panelRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        tabIndex={-1}
        className={cn(
          "relative w-full max-w-lg rounded-[var(--radius-lg)] border border-[var(--border-soft)] bg-[var(--surface-solid)] text-[var(--text)] shadow-[var(--shadow-float)] outline-none animate-scale-in",
          className
        )}
        onClick={(event) => event.stopPropagation()}
      >
        <div className="flex items-center justify-between border-b border-[var(--border-soft)] px-5 py-4">
          <h3 id={titleId} className="font-semibold text-[var(--text)]">
            {title}
          </h3>
          <Button variant="ghost" size="sm" onClick={onClose} aria-label="关闭">
            <X className="w-4 h-4" />
          </Button>
        </div>
        <div className="px-5 py-4 max-h-[70vh] overflow-y-auto scrollbar-thin">{children}</div>
        {footer && (
          <div className="flex justify-end gap-2 border-t border-[var(--border-soft)] px-5 py-4">
            {footer}
          </div>
        )}
      </div>
    </div>
  );
}
