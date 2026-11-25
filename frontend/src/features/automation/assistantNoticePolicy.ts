const cooldown = 5 * 60_000;
export interface NoticeState { seen: string[]; completed: Record<string, number> }
export const emptyNoticeState = (): NoticeState => ({ seen: [], completed: {} });

/** UI-only policy: never changes task state or permission decisions. */
export function acceptNotice(state: NoticeState, automation: string, occurrence: string, status: string,
  conversation: string, currentPath: string, now: number): boolean {
  const key = `${occurrence}:${status}`;
  if (state.seen.includes(key)) return false;
  state.seen = [...state.seen.slice(-255), key];
  const currentChat = currentPath === `/chat/${encodeURIComponent(conversation)}`;
  if (status === "completed") {
    const previous = state.completed[automation];
    // Suppressed completions don't extend the cooldown indefinitely.
    if (previous !== undefined && now - previous < cooldown) return false;
    state.completed[automation] = now;
    const entries = Object.entries(state.completed).sort((a, b) => b[1] - a[1]).slice(0, 256);
    state.completed = Object.fromEntries(entries);
  }
  return !currentChat;
}
