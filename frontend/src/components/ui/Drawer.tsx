import { useCallback, useEffect, useRef, useState, type ReactNode, type RefObject } from "react";
import { X } from "lucide-react";
import { cn } from "./cn";
import { useDialogFocusLifecycle } from "./useDialogFocusLifecycle";

export interface DrawerProps {
  open: boolean;
  side: "left" | "right";
  title: string;
  onClose: () => void;
  returnFocusRef?: RefObject<HTMLElement>;
  showHeader?: boolean;
  className?: string;
  width?: number;
  resizable?: boolean;
  onWidthChange?: (width: number) => void;
  children: ReactNode;
}

const MIN_WIDTH = 360;
const MAX_WIDTH = 640;
const STEP = 16;

export default function Drawer({
  open,
  side,
  title,
  onClose,
  returnFocusRef,
  showHeader = true,
  className,
  width,
  resizable = false,
  onWidthChange,
  children,
}: DrawerProps) {
  const panelRef = useDialogFocusLifecycle({ open, onClose, returnFocusRef });
  const isDraggingRef = useRef(false);
  const startXRef = useRef(0);
  const startWidthRef = useRef(width ?? MIN_WIDTH);
  const [isNarrow, setIsNarrow] = useState(() => typeof window !== "undefined" && window.innerWidth < 960);

  useEffect(() => {
    const handleResize = () => {
      setIsNarrow(window.innerWidth < 960);
    };
    window.addEventListener("resize", handleResize);
    return () => window.removeEventListener("resize", handleResize);
  }, []);

  const clampWidth = useCallback((val: number) => {
    const maxAllowed = Math.min(MAX_WIDTH, (typeof window !== "undefined" ? window.innerWidth : 1024) - 16);
    return Math.max(MIN_WIDTH, Math.min(maxAllowed, val));
  }, []);

  const handlePointerDown = (e: React.PointerEvent) => {
    if (!resizable || !onWidthChange) return;
    isDraggingRef.current = true;
    startXRef.current = e.clientX;
    startWidthRef.current = width ?? MIN_WIDTH;
    (e.target as HTMLElement).setPointerCapture(e.pointerId);
    document.body.style.userSelect = "none";
  };

  const handlePointerMove = (e: React.PointerEvent) => {
    if (!isDraggingRef.current || !onWidthChange) return;
    const deltaX = e.clientX - startXRef.current;
    const newWidth = side === "right"
      ? startWidthRef.current - deltaX
      : startWidthRef.current + deltaX;
    onWidthChange(clampWidth(newWidth));
  };

  const handlePointerUp = (e: React.PointerEvent) => {
    if (!isDraggingRef.current) return;
    isDraggingRef.current = false;
    document.body.style.userSelect = "";
    try {
      (e.target as HTMLElement).releasePointerCapture(e.pointerId);
    } catch {
      // Ignored if capture already released
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (!resizable || !onWidthChange) return;
    const current = width ?? MIN_WIDTH;
    if (e.key === "ArrowLeft") {
      e.preventDefault();
      const change = side === "right" ? STEP : -STEP;
      onWidthChange(clampWidth(current + change));
    } else if (e.key === "ArrowRight") {
      e.preventDefault();
      const change = side === "right" ? -STEP : STEP;
      onWidthChange(clampWidth(current + change));
    }
  };

  if (!open) return null;

  const showHandle = resizable && !!onWidthChange && !isNarrow;

  const resolvedWidthStyle = width
    ? `min(${width}px, calc(100vw - 16px))`
    : "min(88vw, 360px)";

  return (
    <div className="fixed inset-0 z-[var(--z-rail-drawer,50)] flex" role="presentation">
      <div
        className="absolute inset-0 bg-[var(--backdrop)] backdrop-blur-sm"
        aria-hidden="true"
        onClick={onClose}
      />
      <div
        ref={panelRef}
        role="dialog"
        aria-modal="true"
        aria-label={title}
        tabIndex={-1}
        style={{ width: resolvedWidthStyle }}
        className={cn(
          "shadow-float-token relative z-10 flex h-full flex-col border-[var(--border-soft)] bg-[var(--surface-solid)] text-[var(--text)] outline-none transition-[background-color,border-color,box-shadow] duration-[var(--motion-fast)]",
          side === "left" ? "mr-auto border-r" : "ml-auto border-l",
          className
        )}
        onClick={(event) => event.stopPropagation()}
      >
        {/* Resizer Handle */}
        {showHandle && (
          <div
            role="separator"
            aria-orientation="vertical"
            aria-label="调整详情面板宽度"
            aria-valuenow={width ?? MIN_WIDTH}
            aria-valuemin={MIN_WIDTH}
            aria-valuemax={MAX_WIDTH}
            aria-valuetext={`${width ?? MIN_WIDTH} 像素`}
            tabIndex={0}
            onPointerDown={handlePointerDown}
            onPointerMove={handlePointerMove}
            onPointerUp={handlePointerUp}
            onPointerCancel={handlePointerUp}
            onKeyDown={handleKeyDown}
            className={cn(
              "absolute top-0 bottom-0 z-20 w-2 cursor-col-resize hover:bg-[var(--accent-primary)]/40 focus-visible:bg-[var(--accent-primary)] focus-visible:outline-none transition-colors",
              side === "right" ? "-left-1" : "-right-1"
            )}
          />
        )}

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
