import { useEffect, useRef, type RefObject } from "react";

const FOCUSABLE_SELECTOR = [
  "a[href]",
  "area[href]",
  "button:not([disabled])",
  "input:not([disabled])",
  "select:not([disabled])",
  "textarea:not([disabled])",
  "iframe",
  "[contenteditable=true]",
  "[tabindex]:not([tabindex='-1'])",
].join(",");

function getFocusableElements(panel: HTMLElement): HTMLElement[] {
  return Array.from(panel.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR)).filter(
    (element) => {
      if (element.hidden || element.closest("[inert]")) return false;
      const style = window.getComputedStyle(element);
      return style.display !== "none" && style.visibility !== "hidden";
    },
  );
}

function trapTabKey(event: KeyboardEvent, panel: HTMLElement) {
  const focusable = getFocusableElements(panel);
  if (focusable.length === 0) {
    event.preventDefault();
    panel.focus();
    return;
  }

  const first = focusable[0];
  const last = focusable[focusable.length - 1];
  const active = document.activeElement;

  if (active === panel || !panel.contains(active)) {
    event.preventDefault();
    (event.shiftKey ? last : first).focus();
  } else if (event.shiftKey && active === first) {
    event.preventDefault();
    last.focus();
  } else if (!event.shiftKey && active === last) {
    event.preventDefault();
    first.focus();
  }
}

interface DialogFocusLifecycleOptions {
  open: boolean;
  onClose: () => void;
  returnFocusRef?: RefObject<HTMLElement>;
}

export function useDialogFocusLifecycle({
  open,
  onClose,
  returnFocusRef,
}: DialogFocusLifecycleOptions) {
  const panelRef = useRef<HTMLDivElement>(null);
  const onCloseRef = useRef(onClose);
  const returnFocusRefRef = useRef(returnFocusRef);
  const openerRef = useRef<HTMLElement | null>(null);
  const openCycleRef = useRef(false);

  onCloseRef.current = onClose;
  returnFocusRefRef.current = returnFocusRef;

  // Capture before the open render commits, when a child autoFocus may take over.
  if (open && !openCycleRef.current) {
    openerRef.current =
      document.activeElement instanceof HTMLElement ? document.activeElement : null;
    openCycleRef.current = true;
  } else if (!open) {
    openCycleRef.current = false;
  }

  useEffect(() => {
    if (!open) return;

    const panel = panelRef.current;
    const opener = openerRef.current;

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        onCloseRef.current();
        return;
      }

      if (
        event.key === "Tab" &&
        panel?.getAttribute("aria-modal") === "true"
      ) {
        trapTabKey(event, panel);
      }
    };

    window.addEventListener("keydown", handleKeyDown);

    if (panel && !panel.contains(document.activeElement)) {
      panel.focus();
    }

    return () => {
      window.removeEventListener("keydown", handleKeyDown);
      const returnTarget = returnFocusRefRef.current?.current ?? opener;
      if (returnTarget?.isConnected) {
        returnTarget.focus();
      }
    };
  }, [open]);

  return panelRef;
}
