import { useEffect } from "react";
import { useNavigate } from "react-router-dom";

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

      // Ctrl+, -> Settings
      if ((event.ctrlKey || event.metaKey) && event.key === ",") {
        event.preventDefault();
        navigate("/settings");
        return;
      }

      // Alt+1..6 -> Top-level views
      if (event.altKey && !event.ctrlKey && !event.metaKey) {
        switch (event.key) {
          case "1":
            event.preventDefault();
            navigate("/tasks");
            break;
          case "2":
            event.preventDefault();
            navigate("/chat");
            break;
          case "3":
            event.preventDefault();
            navigate("/search");
            break;
          case "4":
            event.preventDefault();
            navigate("/workflows");
            break;
          case "5":
            event.preventDefault();
            navigate("/workspaces");
            break;
          case "6":
            event.preventDefault();
            navigate("/capabilities");
            break;
          default:
            break;
        }
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => {
      window.removeEventListener("keydown", handleKeyDown);
    };
  }, [navigate]);
}
