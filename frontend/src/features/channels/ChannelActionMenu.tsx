import { useEffect, useId, useRef, useState, type KeyboardEvent } from "react";
import { Ellipsis } from "lucide-react";
import Button from "../../components/ui/Button";
import { placeFloatingPanel } from "../../components/ui/floatingPanelPosition";

export default function ChannelActionMenu({ summary, busy, actions }: {
  summary: string;
  busy: boolean;
  actions: { label: string; run: () => void }[];
}) {
  const id = useId();
  const triggerRef = useRef<HTMLButtonElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);
  const [open, setOpen] = useState(false);

  useEffect(() => {
    const trigger = triggerRef.current!;
    const menu = menuRef.current!;
    trigger.popoverTargetElement = menu;
    const position = () => {
      const { left, top } = placeFloatingPanel(trigger.getBoundingClientRect(), {
        width: menu.offsetWidth, height: menu.offsetHeight,
      }, { width: window.innerWidth, height: window.innerHeight });
      menu.style.left = `${left}px`;
      menu.style.top = `${top}px`;
    };
    const sync = () => {
      const visible = menu.matches(":popover-open");
      setOpen(visible);
      if (visible) {
        position();
        menu.querySelector<HTMLElement>('[role="menuitem"]')?.focus({ preventScroll: true });
      }
    };
    const dismissOnMove = (event: Event) => {
      if (event.target instanceof Node && menu.contains(event.target)) return;
      if (menu.matches(":popover-open")) position();
    };
    menu.addEventListener("toggle", sync);
    window.addEventListener("resize", dismissOnMove);
    window.addEventListener("scroll", dismissOnMove, true);
    return () => {
      menu.removeEventListener("toggle", sync);
      window.removeEventListener("resize", dismissOnMove);
      window.removeEventListener("scroll", dismissOnMove, true);
    };
  }, []);

  function handleKeys(event: KeyboardEvent<HTMLDivElement>) {
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      event.currentTarget.hidePopover();
      triggerRef.current?.focus();
      return;
    }
    if (!["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) return;
    event.preventDefault();
    const items = [...event.currentTarget.querySelectorAll<HTMLButtonElement>('[role="menuitem"]')];
    const current = items.indexOf(document.activeElement as HTMLButtonElement);
    const index = event.key === "Home" ? 0 : event.key === "End" ? items.length - 1
      : (current + (event.key === "ArrowDown" ? 1 : -1) + items.length) % items.length;
    items[index]?.focus();
  }

  return <>
    <Button ref={triggerRef} variant="ghost" size="sm" iconOnly disabled={busy} loading={busy}
      aria-haspopup="menu" aria-expanded={open} aria-controls={id} aria-label="渠道操作" title="渠道操作"><Ellipsis size={18} /></Button>
    <div {...{ popover: "auto" }} ref={menuRef} id={id} role="menu" aria-label="渠道操作" className="channel-action-menu" onKeyDown={handleKeys}>
      <p role="presentation">{summary}</p>
      {actions.map(({ label, run }) => <button key={label} type="button" role="menuitem" onClick={() => { menuRef.current?.hidePopover(); run(); }}>{label}</button>)}
    </div>
  </>;
}
