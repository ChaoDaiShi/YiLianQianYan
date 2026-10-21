import { useEffect, useRef, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";

export function ComposerPopover({ label, trigger, disabled, children, className = "" }: {
  label: string; trigger: ReactNode; disabled?: boolean; className?: string; children: (close: () => void) => ReactNode;
}) {
  const anchor = useRef<HTMLButtonElement>(null);
  const [position, setPosition] = useState<{ right: number; bottom: number } | null>(null);
  const close = () => { setPosition(null); anchor.current?.focus(); };
  useEffect(() => {
    if (!position) return;
    const onKey = (event: KeyboardEvent) => { if (event.key === "Escape") close(); };
    const onMove = () => setPosition(null);
    window.addEventListener("keydown", onKey);
    window.addEventListener("resize", onMove);
    return () => { window.removeEventListener("keydown", onKey); window.removeEventListener("resize", onMove); };
  }, [position]);
  return <>
    <button ref={anchor} type="button" aria-label={label} title={label} aria-haspopup="dialog" aria-expanded={!!position}
      disabled={disabled} className={`composer-control ${className}`} onClick={() => {
        if (position) { close(); return; }
        const rect = anchor.current!.getBoundingClientRect();
        const width = Math.min(296, window.innerWidth - 24);
        setPosition({ right: Math.min(Math.max(12, window.innerWidth - rect.right), window.innerWidth - width - 12), bottom: window.innerHeight - rect.top + 8 });
      }}>{trigger}</button>
    {position && createPortal(<><button className="composer-popover-backdrop" aria-label="关闭选项" onClick={close} /><div className="composer-popover" role="dialog" aria-label={label} style={position}>{children(close)}</div></>, document.body)}
  </>;
}
