import { useEffect } from "react";
import { useNavigate } from "react-router-dom";
import { NAV_ENTRIES } from "./navRegistry";

export function useUiNavigationShortcuts(): void {
  const navigate = useNavigate();

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      // Ignore if user is composing via IME
      if (event.isComposing) return;

      // Ignore if focus is in an input, textarea, select, contenteditable, or editor
      const target = event.target as HTMLElement | null;
      if (
        target &&
        (target.tagName === "INPUT" ||
          target.tagName === "TEXTAREA" ||
          target.tagName === "SELECT" ||
          target.isContentEditable ||
          target.closest("[contenteditable='true']") ||
          target.closest(".monaco-editor"))
      ) {
        return;
      }

      // Ignore if any modal or drawer is open
      if (document.querySelector("[role='dialog']") !== null) {
        return;
      }

      // Match shortcuts dynamically from NAV_ENTRIES
      for (const entry of NAV_ENTRIES) {
        if (!entry.shortcut) continue;
        const parts = entry.shortcut.toLowerCase().split("+");
        const key = parts[parts.length - 1];
        const reqCtrl = parts.includes("ctrl");
        const reqAlt = parts.includes("alt");
        const reqShift = parts.includes("shift");

        const hasCtrl = event.ctrlKey || event.metaKey;
        const hasAlt = event.altKey;
        const hasShift = event.shiftKey;

        if (
          hasCtrl === reqCtrl &&
          hasAlt === reqAlt &&
          hasShift === reqShift &&
          event.key.toLowerCase() === key
        ) {
          event.preventDefault();
          navigate(entry.to);
          return;
        }
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => {
      window.removeEventListener("keydown", handleKeyDown);
    };
  }, [navigate]);
}
