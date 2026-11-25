/** Serializes observation only. An invalidation during I/O schedules one follow-up read. */
export function createGoalReadQueue(read: (verify: boolean, signal: AbortSignal) => Promise<void>) {
  const controller = new AbortController();
  let active: Promise<void> | null = null;
  let pending = false;
  let verifyPending = false;
  const refresh = (verify = false, invalidated = false): Promise<void> => {
    if (controller.signal.aborted) return Promise.resolve();
    if (active) {
      pending ||= verify || invalidated;
      verifyPending ||= verify;
      return active;
    }
    pending = true; verifyPending ||= verify;
    active = (async () => {
      try {
        while (pending && !controller.signal.aborted) {
          const check = verifyPending;
          pending = false; verifyPending = false;
          await read(check, controller.signal);
        }
      } finally { active = null; }
    })();
    return active;
  };
  return { refresh, close: () => { pending = false; controller.abort(); } };
}
