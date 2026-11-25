import { useCallback, useId, useLayoutEffect, useRef, useState, type KeyboardEvent as ReactKeyboardEvent, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { placeFloatingPanel } from "../ui/floatingPanelPosition";

export function ComposerPopover({ label, trigger, disabled, children, className = "" }: {
  label: string; trigger: ReactNode; disabled?: boolean; className?: string; children: (close: () => void) => ReactNode;
}) {
  const anchor = useRef<HTMLButtonElement>(null);
  const panel = useRef<HTMLDivElement>(null);
  const id = useId();
  const [open, setOpen] = useState(false);
  const close = useCallback(() => {
    setOpen(false);
    anchor.current?.focus({ preventScroll: true });
  }, []);

  useLayoutEffect(() => {
    if (!open || !anchor.current || !panel.current) return;
    const button = anchor.current;
    const dialog = panel.current;
    const place = () => {
      const { left, top, side } = placeFloatingPanel(button.getBoundingClientRect(), {
        width: dialog.offsetWidth, height: dialog.offsetHeight,
      }, { width: window.innerWidth, height: window.innerHeight });
      dialog.style.left = `${left}px`;
      dialog.style.top = `${top}px`;
      dialog.dataset.side = side;
    };
    place();
    dialog.focus({ preventScroll: true });
    const resize = new ResizeObserver(place);
    resize.observe(dialog);
    resize.observe(button);
    window.addEventListener("resize", place);
    const onScroll = (event: Event) => {
      if (!(event.target instanceof Node && dialog.contains(event.target))) place();
    };
    const onOutsidePointer = (event: PointerEvent) => {
      const target = event.target as Node;
      if (!button.contains(target) && !dialog.contains(target)) setOpen(false);
    };
    window.addEventListener("scroll", onScroll, true);
    document.addEventListener("pointerdown", onOutsidePointer);
    return () => {
      resize.disconnect();
      window.removeEventListener("resize", place);
      window.removeEventListener("scroll", onScroll, true);
      document.removeEventListener("pointerdown", onOutsidePointer);
    };
  }, [open, close]);

  const handleKey = (event: ReactKeyboardEvent<HTMLDivElement>) => {
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      close();
      return;
    }
    if (event.key !== "Tab") return;
    const controls = Array.from(event.currentTarget.querySelectorAll<HTMLElement>("button, input, select, textarea, a[href], [tabindex]")).filter(
      element => element.tabIndex >= 0 && !element.matches(":disabled") && element.getClientRects().length > 0,
    );
    const first = controls[0];
    const last = controls[controls.length - 1];
    if (!first || (event.shiftKey && [first, event.currentTarget].includes(document.activeElement as HTMLElement)) ||
        (!event.shiftKey && document.activeElement === last)) {
      event.preventDefault();
      (event.shiftKey ? last : first)?.focus();
    }
  };

  return <>
    <button ref={anchor} type="button" aria-label={label} title={label} aria-haspopup="dialog" aria-expanded={open} aria-controls={open ? id : undefined}
      disabled={disabled} className={`composer-control ${className}`} onClick={() => open ? close() : setOpen(true)}>{trigger}</button>
    {open && createPortal(
      <div id={id} ref={panel} className="composer-popover" role="dialog" aria-label={label} tabIndex={-1} onKeyDown={handleKey}>{children(close)}</div>,
      document.body,
    )}
  </>;
}
