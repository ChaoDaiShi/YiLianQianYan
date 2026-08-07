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
  if (!open) return null;
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4">
      <div className="absolute inset-0 bg-black/60 backdrop-blur-sm animate-fade-in" onClick={onClose} />
      <div
        className={cn(
          "relative w-full max-w-lg rounded-xl border border-[var(--border)] bg-[var(--panel)] shadow-2xl animate-scale-in",
          className
        )}
      >
        <div className="flex items-center justify-between px-5 py-4 border-b border-[var(--border)]">
          <h3 className="font-semibold text-[var(--text)]">{title}</h3>
          <Button variant="ghost" size="sm" onClick={onClose} aria-label="关闭">
            <X className="w-4 h-4" />
          </Button>
        </div>
        <div className="px-5 py-4 max-h-[70vh] overflow-y-auto scrollbar-thin">{children}</div>
        {footer && (
          <div className="px-5 py-4 border-t border-[var(--border)] flex justify-end gap-2">
            {footer}
          </div>
        )}
      </div>
    </div>
  );
}
