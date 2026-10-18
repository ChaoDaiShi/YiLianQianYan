import { useEffect, useState } from "react";
import { createPortal } from "react-dom";
import { CheckCircle2, AlertCircle, Info, AlertTriangle, X } from "lucide-react";

export type ToastType = "success" | "error" | "info" | "warning";

export interface ToastMessage {
  id: string;
  message: string;
  type?: ToastType;
  duration?: number;
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

  show(message: string, type: ToastType = "info", duration = 3000) {
    const id = crypto.randomUUID();
    const item: ToastMessage = { id, message, type, duration };
    this.toasts = [...this.toasts, item];
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

export function showToast(message: string, type: ToastType = "info", duration = 3000) {
  return toastManager.show(message, type, duration);
}

const TYPE_ICONS: Record<ToastType, React.ReactNode> = {
  success: <CheckCircle2 className="h-4 w-4 text-[#52b798] shrink-0" />,
  error: <AlertCircle className="h-4 w-4 text-[#e05a80] shrink-0" />,
  warning: <AlertTriangle className="h-4 w-4 text-[#d9a441] shrink-0" />,
  info: <Info className="h-4 w-4 text-[#99cfea] shrink-0" />,
};

const TYPE_BORDERS: Record<ToastType, string> = {
  success: "border-[#52b798]/30",
  error: "border-[#e05a80]/30",
  warning: "border-[#d9a441]/30",
  info: "border-[#99cfea]/30",
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
      className="pointer-events-none fixed bottom-4 right-4 z-[var(--z-toast-tooltip,90)] flex flex-col gap-2 max-w-sm w-full"
      role="region"
      aria-label="系统轻通知"
    >
      {toasts.map((toast) => (
        <div
          key={toast.id}
          className={`pointer-events-auto flex items-center gap-2.5 rounded-xl border bg-[#29263a]/95 text-[#eeeaf8] p-3 shadow-2xl backdrop-blur-md animate-scale-in transition-all ${
            TYPE_BORDERS[toast.type || "info"]
          }`}
        >
          {TYPE_ICONS[toast.type || "info"]}
          <span className="flex-1 text-xs leading-5 break-words font-medium">
            {toast.message}
          </span>
          <button
            type="button"
            onClick={() => toastManager.dismiss(toast.id)}
            className="rounded p-1 text-[#eeeaf8]/60 hover:text-[#eeeaf8] transition-colors"
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
