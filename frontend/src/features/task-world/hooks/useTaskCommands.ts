import { useCallback, useRef } from "react";
import type { ApiError } from "../../../api/taskWorld";

type CommandResult = { ok: true; data: unknown } | { ok: false; error: ApiError };

/**
 * The page's single write gateway.
 *
 * Every mutation goes through here so saving and failure presentation stay
 * uniform, and so a stale-revision rejection re-reads the authoritative state
 * instead of leaving the surface on a revision the backend has moved past.
 */
export function useTaskCommands(
  reload: () => Promise<void>,
  onSavingChange: (saving: boolean) => void,
  onSaveError: (error: ApiError | null) => void,
) {
  const pending = useRef(false);
  return useCallback(async (operation: () => Promise<CommandResult>) => {
    if (pending.current) return false;
    pending.current = true;
    onSavingChange(true);
    onSaveError(null);
    try {
      const result = await operation();
      if (!result.ok) {
        onSaveError(result.error);
        if (result.error.code === "stale_revision" || result.error.code === "stale_view_revision") await reload();
        return false;
      }
      await reload();
      return true;
    } finally {
      pending.current = false;
      onSavingChange(false);
    }
  }, [onSaveError, onSavingChange, reload]);
}
