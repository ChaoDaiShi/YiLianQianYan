import { useRef, useState, type ReactNode } from "react";

interface TooltipProps {
  content: string;
  children: ReactNode;
  side?: "left" | "right";
}

export default function Tooltip({ content, children, side = "right" }: TooltipProps) {
  const triggerRef = useRef<HTMLSpanElement>(null);
  const [open, setOpen] = useState(false);
  const [position, setPosition] = useState({ top: 0, left: 0 });

  const show = () => {
    const rect = triggerRef.current?.getBoundingClientRect();
    if (!rect) return;
    setPosition({
      top: rect.top + rect.height / 2,
      left: side === "left" ? rect.left - 8 : rect.right + 8,
    });
    setOpen(true);
  };

  const hide = () => setOpen(false);

  return (
    <span
      ref={triggerRef}
      className="inline-flex"
      onPointerEnter={show}
      onPointerLeave={hide}
      onFocus={show}
      onBlur={(event) => {
        if (!event.currentTarget.contains(event.relatedTarget as Node | null)) {
          hide();
        }
      }}
    >
      {children}
      <span
        role="tooltip"
        aria-hidden={!open}
        style={{ top: position.top, left: position.left }}
        className={`ui-tooltip pointer-events-none fixed z-50 -translate-y-1/2 whitespace-nowrap rounded-md border border-[var(--border)] bg-[var(--surface-elevated)] px-2 py-1 text-[var(--text)] shadow-[var(--shadow-float)] transition-opacity duration-[var(--motion-fast)] ${side === "left" ? "-translate-x-full" : ""} ${
          open ? "opacity-100" : "opacity-0"
        }`}
      >
        {content}
      </span>
    </span>
  );
}
