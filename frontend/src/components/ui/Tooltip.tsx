import { useEffect, useRef, useState, type ReactNode } from "react";

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
  const triggerRef = useRef<HTMLSpanElement>(null);
  const [open, setOpen] = useState(false);
  const [position, setPosition] = useState({ top: 0, left: 0 });
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const calculatePosition = () => {
    const rect = triggerRef.current?.getBoundingClientRect();
    if (!rect) return;

    let top = 0;
    let left = 0;

    if (side === "left") {
      top = rect.top + rect.height / 2;
      left = rect.left - 8;
    } else if (side === "right") {
      top = rect.top + rect.height / 2;
      left = rect.right + 8;
    } else if (side === "top") {
      top = rect.top - 8;
      left = rect.left + rect.width / 2;
    } else {
      top = rect.bottom + 8;
      left = rect.left + rect.width / 2;
    }

    // Boundary clamp
    const viewportWidth = typeof window !== "undefined" ? window.innerWidth : 1024;
    const viewportHeight = typeof window !== "undefined" ? window.innerHeight : 768;

    left = Math.max(16, Math.min(viewportWidth - 276, left));
    top = Math.max(16, Math.min(viewportHeight - 60, top));

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

  return (
    <span
      ref={triggerRef}
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
      {open && (
        <span
          role="tooltip"
          aria-hidden="false"
          style={{ top: position.top, left: position.left }}
          className={`ui-tooltip pointer-events-none fixed z-50 max-w-[260px] -translate-y-1/2 rounded-[var(--radius-sm)] border border-[var(--border-soft)] bg-[var(--surface-elevated)] px-2.5 py-1.5 text-[13px] leading-snug text-[var(--text)] shadow-[var(--shadow-float)] whitespace-normal break-words transition-opacity duration-[var(--motion-fast)] opacity-100 ${
            side === "left" ? "-translate-x-full" : ""
          }`}
        >
          {content}
        </span>
      )}
    </span>
  );
}
