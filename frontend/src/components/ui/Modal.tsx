import { useId } from "react";
import { X } from "lucide-react";
import { cn } from "./cn";
import Button from "./Button";
import { useDialogFocusLifecycle } from "./useDialogFocusLifecycle";

export type ModalSize = "sm" | "md" | "lg";

export interface ModalProps {
  open: boolean;
  onClose: () => void;
  title: string;
  size?: ModalSize;
  children: React.ReactNode;
  className?: string;
  footer?: React.ReactNode;
}

const modalSizes: Record<ModalSize, string> = {
  sm: "max-w-[480px]",
  md: "max-w-[640px]",
  lg: "max-w-[860px]",
};

export default function Modal({
  open,
  onClose,
  title,
  size = "sm",
  children,
  className,
  footer,
}: ModalProps) {
  const panelRef = useDialogFocusLifecycle({ open, onClose });
  const titleId = useId();

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
          "shadow-float-token relative flex flex-col w-full max-h-[calc(100dvh-32px)] rounded-[var(--radius-lg)] border border-[var(--border-soft)] bg-[var(--surface-solid)] text-[var(--text)] outline-none animate-scale-in",
          modalSizes[size],
          className
        )}
        onClick={(event) => event.stopPropagation()}
      >
        <div className="flex shrink-0 items-center justify-between border-b border-[var(--border-soft)] px-5 py-4">
          <h3 id={titleId} className="font-semibold text-[var(--text)]">
            {title}
          </h3>
          <Button variant="ghost" size="sm" onClick={onClose} aria-label="关闭">
            <X className="w-4 h-4" />
          </Button>
        </div>
        <div className="min-h-0 flex-1 px-5 py-4 overflow-y-auto scrollbar-thin">
          {children}
        </div>
        {footer && (
          <div className="flex shrink-0 justify-end gap-2 border-t border-[var(--border-soft)] px-5 py-4">
            {footer}
          </div>
        )}
      </div>
    </div>
  );
}
