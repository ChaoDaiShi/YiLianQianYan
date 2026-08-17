export const APP_CONTENT_VIEWPORT_CLASS_NAME =
  "relative z-0 flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden animate-page-in";

export const WORKBENCH_VIEWPORT_CLASS_NAME =
  "workbench-grid min-h-0 overflow-hidden grid-rows-[minmax(0,1fr)]";

export const CHAT_COLUMN_VIEWPORT_CLASS_NAME =
  "flex h-full min-h-0 min-w-0 flex-col overflow-hidden";

export const MESSAGE_LIST_VIEWPORT_CLASS_NAME =
  "scrollbar-thin min-h-0 flex-1 overflow-y-auto px-4 py-6";

export interface MessageScrollTarget {
  scrollHeight: number;
  scrollTo(options: ScrollToOptions): void;
}

export interface MessageScrollMetrics {
  scrollTop: number;
  clientHeight: number;
  scrollHeight: number;
}

export function isNearBottom(
  container: MessageScrollMetrics | null,
  threshold = 48
): boolean {
  if (!container) return true;
  return (
    container.scrollHeight - container.scrollTop - container.clientHeight <=
    threshold
  );
}

export function scrollMessageListToBottom(
  container: MessageScrollTarget | null,
  behavior: ScrollBehavior = "smooth"
): void {
  if (!container) return;
  container.scrollTo({ top: container.scrollHeight, behavior });
}

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
