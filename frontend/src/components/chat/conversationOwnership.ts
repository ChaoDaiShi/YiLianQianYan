/** Local UI ownership only; no transport or persisted revision semantics. */
export function createConversationOwnership(initialId: string | null) {
  let id = initialId, epoch = 0, revision = 0, generation = 0;
  let live = false, promoted = false;
  const completedIds = new Set<string>();
  const capture = () => ({ epoch, revision });
  const current = (ticket: { epoch: number }) => ticket.epoch === epoch;
  return {
    get id() { return id; },
    capture,
    current,
    activate(next: string | null) {
      if (id === next) return;
      id = next; epoch++; revision = 0; live = false; promoted = false; completedIds.clear();
    },
    promote(next: string) {
      if (id === next) return;
      id = next; epoch++; promoted = true;
    },
    claimCompletion(messageId: string) {
      if (completedIds.has(messageId)) return false;
      completedIds.add(messageId);
      return true;
    },
    invalidate() { epoch++; },
    mutate() { revision++; },
    start() { live = true; revision++; },
    finish() { live = false; revision++; },
    hydration() {
      const ticket = { ...capture(), generation: ++generation, protected: live || promoted };
      promoted = false;
      return ticket;
    },
    ownsRequest(ticket: { epoch: number; generation: number }) {
      return current(ticket) && ticket.generation === generation;
    },
    admits(ticket: { epoch: number; revision: number; generation: number; protected: boolean }) {
      return current(ticket) && ticket.generation === generation
        && ticket.revision === revision && !ticket.protected;
    },
  };
}
