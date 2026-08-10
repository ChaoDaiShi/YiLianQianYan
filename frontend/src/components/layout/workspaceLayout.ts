export type WorkspaceMode = "narrow" | "compact" | "full";

export function getWorkspaceMode(width: number): WorkspaceMode {
  if (width >= 1180) return "full";
  if (width >= 960) return "compact";
  return "narrow";
}

export type WorkspaceDrawer = "conversations" | "execution";

export interface WorkspaceDrawerState {
  conversationOpen: boolean;
  executionOpen: boolean;
}

export function getDrawerState(
  mode: WorkspaceMode,
  target: WorkspaceDrawer
): WorkspaceDrawerState {
  if (mode === "full") {
    return { conversationOpen: false, executionOpen: false };
  }
  if (mode === "compact") {
    return {
      conversationOpen: false,
      executionOpen: target === "execution",
    };
  }
  return {
    conversationOpen: target === "conversations",
    executionOpen: target === "execution",
  };
}
