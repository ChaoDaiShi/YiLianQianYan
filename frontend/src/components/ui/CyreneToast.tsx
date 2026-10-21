import { useEffect, useState } from "react";
import { createPortal } from "react-dom";
import { CheckCircle2, AlertCircle, Info, AlertTriangle, X } from "lucide-react";

export type ToastType = "success" | "error" | "info" | "warning";

export interface ToastAction {
  label: string;
  onClick: () => void;
}

export interface ToastMessage {
  id: string;
  message: string;
  type?: ToastType;
  duration?: number;
  action?: ToastAction;
}

type ToastListener = (toasts: ToastMessage[]) => void;

class ToastManager {
  private listeners: Set<ToastListener> = new Set();
  private toasts: ToastMessage[] = [];

  subscribe(listener: ToastListener) {
    this.listeners.add(listener);
    listener([...this.toasts]);
    return () => {
      this.listeners.delete(listener);
    };
  }

  show(
    message: string,
    type: ToastType = "info",
    duration = 3000,
    action?: ToastAction,
  ) {
    const id = crypto.randomUUID();
    const item: ToastMessage = { id, message, type, duration, action };
    // Keep at most 3 active toasts
    this.toasts = [...this.toasts.slice(-2), item];
    this.notify();

    if (duration > 0) {
      window.setTimeout(() => {
        this.dismiss(id);
      }, duration);
    }
    return id;
  }

  dismiss(id: string) {
    this.toasts = this.toasts.filter((t) => t.id !== id);
    this.notify();
  }

  private notify() {
    this.listeners.forEach((listener) => listener([...this.toasts]));
  }
}

export const toastManager = new ToastManager();

export function showToast(
  message: string,
  type: ToastType = "info",
  duration = 3000,
  action?: ToastAction,
) {
  return toastManager.show(message, type, duration, action);
}

const TYPE_ICONS: Record<ToastType, React.ReactNode> = {
  success: <CheckCircle2 className="h-4 w-4 text-[var(--success)] shrink-0" />,
  error: <AlertCircle className="h-4 w-4 text-[var(--danger)] shrink-0" />,
  warning: <AlertTriangle className="h-4 w-4 text-[var(--warning)] shrink-0" />,
  info: <Info className="h-4 w-4 text-[var(--info)] shrink-0" />,
};

const TYPE_BORDERS: Record<ToastType, string> = {
  success: "border-l-4 border-l-[var(--success)] border-[var(--border-soft)]",
  error: "border-l-4 border-l-[var(--danger)] border-[var(--border-soft)]",
  warning: "border-l-4 border-l-[var(--warning)] border-[var(--border-soft)]",
  info: "border-l-4 border-l-[var(--info)] border-[var(--border-soft)]",
};

export default function CyreneToast() {
  const [toasts, setToasts] = useState<ToastMessage[]>([]);
  const [mounted, setMounted] = useState(false);

  useEffect(() => {
    setMounted(true);
    return toastManager.subscribe(setToasts);
  }, []);

  if (!mounted || typeof document === "undefined" || toasts.length === 0) {
    return null;
  }

  const content = (
    <div
      className="pointer-events-none fixed top-4 right-4 z-[var(--z-toast-tooltip,90)] flex flex-col gap-2 max-w-sm w-full"
      role="region"
      aria-live="polite"
      aria-atomic="true"
      aria-label="系统轻通知"
    >
      {toasts.map((toast) => (
        <div
          key={toast.id}
          className={`pointer-events-auto flex items-center gap-2.5 rounded-xl bg-[var(--surface-solid)] text-[var(--text-primary)] p-3 shadow-[var(--shadow-float)] backdrop-blur-md animate-scale-in transition-all ${
            TYPE_BORDERS[toast.type || "info"]
          }`}
        >
          {TYPE_ICONS[toast.type || "info"]}
          <span className="flex-1 text-xs leading-5 break-words font-medium">
            {toast.message}
          </span>
          {toast.action && (
            <button
              type="button"
              onClick={() => {
                toast.action?.onClick();
                toastManager.dismiss(toast.id);
              }}
              className="text-xs font-semibold text-[var(--accent-primary)] hover:underline shrink-0"
            >
              {toast.action.label}
            </button>
          )}
          <button
            type="button"
            onClick={() => toastManager.dismiss(toast.id)}
            className="rounded p-1 text-[var(--text-secondary)] hover:text-[var(--text-primary)] hover:bg-[var(--surface-hover)] transition-colors"
            aria-label="关闭提示"
          >
            <X className="h-3.5 w-3.5" />
          </button>
        </div>
      ))}
    </div>
  );

  return createPortal(content, document.body);
}
