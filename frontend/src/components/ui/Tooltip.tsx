import { useId, useEffect, useRef, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";

export interface TooltipProps {
  content: string;
  children: ReactNode;
  side?: "left" | "right" | "top" | "bottom";
  delayMs?: number;
}

export default function Tooltip({
  content,
  children,
  side = "right",
  delayMs = 300,
}: TooltipProps) {
  const tooltipId = useId();
  const triggerRef = useRef<HTMLSpanElement>(null);
  const [open, setOpen] = useState(false);
  const [position, setPosition] = useState({ top: 0, left: 0 });
  const [actualSide, setActualSide] = useState(side);
  const [mounted, setMounted] = useState(false);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    setMounted(true);
  }, []);

  const calculatePosition = () => {
    const rect = triggerRef.current?.getBoundingClientRect();
    if (!rect) return;

    const viewportWidth = typeof window !== "undefined" ? window.innerWidth : 1024;
    const viewportHeight = typeof window !== "undefined" ? window.innerHeight : 768;

    let targetSide = side;
    // Auto flip horizontally
    if (side === "right" && rect.right + 260 > viewportWidth - 12) {
      targetSide = "left";
    } else if (side === "left" && rect.left - 260 < 12) {
      targetSide = "right";
    }

    // Auto flip vertically
    if (side === "top" && rect.top - 48 < 12) {
      targetSide = "bottom";
    } else if (side === "bottom" && rect.bottom + 48 > viewportHeight - 12) {
      targetSide = "top";
    }

    setActualSide(targetSide);

    let top = 0;
    let left = 0;

    if (targetSide === "left") {
      top = rect.top + rect.height / 2;
      left = rect.left - 8;
    } else if (targetSide === "right") {
      top = rect.top + rect.height / 2;
      left = rect.right + 8;
    } else if (targetSide === "top") {
      top = rect.top - 8;
      left = rect.left + rect.width / 2;
    } else {
      top = rect.bottom + 8;
      left = rect.left + rect.width / 2;
    }

    // Boundary clamp
    left = Math.max(12, Math.min(viewportWidth - 276, left));
    top = Math.max(12, Math.min(viewportHeight - 60, top));

    setPosition({ top, left });
  };

  const showImmediate = () => {
    if (timerRef.current) clearTimeout(timerRef.current);
    calculatePosition();
    setOpen(true);
  };

  const showDelayed = () => {
    if (timerRef.current) clearTimeout(timerRef.current);
    timerRef.current = setTimeout(() => {
      calculatePosition();
      setOpen(true);
    }, delayMs);
  };

  const hide = () => {
    if (timerRef.current) clearTimeout(timerRef.current);
    setOpen(false);
  };

  useEffect(() => {
    return () => {
      if (timerRef.current) clearTimeout(timerRef.current);
    };
  }, []);

  const sideTransform =
    actualSide === "left"
      ? "-translate-x-full -translate-y-1/2"
      : actualSide === "right"
        ? "-translate-y-1/2"
        : actualSide === "top"
          ? "-translate-x-1/2 -translate-y-full"
          : "-translate-x-1/2";

  const tooltipElement = open ? (
    <div
      id={tooltipId}
      role="tooltip"
      aria-hidden="false"
      style={{ top: position.top, left: position.left }}
      className={`ui-tooltip pointer-events-none fixed z-[var(--z-toast-tooltip,90)] max-w-[260px] rounded-[var(--radius-sm)] border border-[var(--border-soft)] bg-[var(--surface-elevated)] text-[var(--text)] px-2.5 py-1.5 text-[12px] leading-snug shadow-[var(--shadow-float)] backdrop-blur-md whitespace-normal break-words transition-opacity duration-[var(--motion-fast)] opacity-100 ${sideTransform}`}
    >
      {content}
    </div>
  ) : null;

  return (
    <span
      ref={triggerRef}
      aria-describedby={open ? tooltipId : undefined}
      className="inline-flex"
      onPointerEnter={showDelayed}
      onPointerLeave={hide}
      onFocus={showImmediate}
      onBlur={(event) => {
        if (!event.currentTarget.contains(event.relatedTarget as Node | null)) {
          hide();
        }
      }}
    >
      {children}
      {mounted && tooltipElement && typeof document !== "undefined"
        ? createPortal(tooltipElement, document.body)
        : tooltipElement}
    </span>
  );
}
