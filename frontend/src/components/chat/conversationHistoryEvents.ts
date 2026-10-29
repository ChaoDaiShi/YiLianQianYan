/** Notify visible history projections to reload the server's conversation overview. */
export const CONVERSATION_HISTORY_CHANGED = "yilian:conversation-history-changed";
export function notifyConversationHistoryChanged() {
  window.dispatchEvent(new Event(CONVERSATION_HISTORY_CHANGED));
}
