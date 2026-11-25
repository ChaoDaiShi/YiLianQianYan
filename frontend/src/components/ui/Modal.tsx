import { useId, type ReactNode, type RefObject } from "react";
import { createPortal } from "react-dom";
import { X } from "lucide-react";
import { cn } from "./cn";
import Button from "./Button";
import { useDialogFocusLifecycle } from "./useDialogFocusLifecycle";

export type ModalSize = "sm" | "md" | "lg" | "xl";
export type ModalVariant = "default" | "danger";

export interface ModalProps {
  open: boolean;
  onClose: () => void;
  title: string;
  size?: ModalSize;
  variant?: ModalVariant;
  children: ReactNode;
  className?: string;
  footer?: ReactNode;
  closeDisabled?: boolean;
  returnFocusRef?: RefObject<HTMLElement>;
}

const modalSizes: Record<ModalSize, string> = {
  sm: "max-w-[420px]",
  md: "max-w-[560px]",
  lg: "max-w-[740px]",
  xl: "max-w-[920px]",
};

export default function Modal({
  open,
  onClose,
  title,
  size = "sm",
  variant = "default",
  children,
  className,
  footer,
  closeDisabled = false,
  returnFocusRef,
}: ModalProps) {
  const requestClose = () => { if (!closeDisabled) onClose(); };
  const panelRef = useDialogFocusLifecycle({ open, onClose: requestClose, returnFocusRef });
  const titleId = useId();

  if (!open) return null;

  const modalContent = (
    <div
      className="fixed inset-0 z-[var(--z-modal-approval,70)] flex items-center justify-center p-4"
      role="presentation"
    >
      <div
        className="ui-dialog-scrim absolute inset-0 animate-fade-in"
        aria-hidden="true"
        onClick={requestClose}
      />
      <div
        ref={panelRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        tabIndex={-1}
        className={cn(
          "shared-dialog shadow-float-token relative flex flex-col w-full max-h-[calc(100dvh-32px)] overflow-hidden rounded-[var(--radius-lg)] border border-[var(--border-soft)] bg-[var(--surface-solid)] text-[var(--text)] text-[length:var(--font-size-body)] outline-none animate-scale-in",
          modalSizes[size],
          className
        )}
        onClick={(event) => event.stopPropagation()}
      >
        {/* Top luminous accent band */}
        <div
          className={cn(
            "h-1 w-full shrink-0",
            variant === "danger" && "bg-[var(--danger)]"
          )}
          style={variant !== "danger" ? { backgroundImage: "var(--cyrene-gradient-primary)" } : undefined}
          aria-hidden="true"
        />

        <div className="flex shrink-0 items-center justify-between gap-3 border-b border-[var(--border-soft)] px-5 py-4">
          <h3 id={titleId} className="min-w-0 [overflow-wrap:anywhere] font-semibold text-[var(--text)] text-[length:var(--font-size-title-sm)]">
            {title}
          </h3>
          <Button
            variant="ghost"
            size="sm"
            className="shrink-0"
            disabled={closeDisabled}
            onClick={requestClose}
            aria-label={`关闭${title}`}
          >
            <X className="w-4 h-4" />
          </Button>
        </div>

        <div className="min-h-0 flex-1 px-5 py-4 overflow-y-auto scrollbar-thin">
          {children}
        </div>

        {footer && (
          <div className="shared-dialog-footer flex shrink-0 flex-wrap justify-end gap-2 border-t border-[var(--border-soft)] px-5 py-3.5">
            {footer}
          </div>
        )}
      </div>
    </div>
  );

  if (typeof document !== "undefined") {
    return createPortal(modalContent, document.body);
  }

  return modalContent;
}
