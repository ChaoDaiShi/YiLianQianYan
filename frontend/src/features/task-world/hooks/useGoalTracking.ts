import { useCallback, useEffect, useRef, useState } from "react";
import { subscribeToEvents, type EventConnectionState } from "../../../api/events";
import { isTaskWorldEvent } from "../taskGraphProjection";
import { loadGoalTracking, type GoalTrackingData } from "../goal-view/goalTracking";
import { createGoalReadQueue } from "../goal-view/goalReadQueue";
import { observeGoalRead } from "../goal-view/goalReminders";

export function useGoalTracking(graphId: string) {
  const [data, setData] = useState<GoalTrackingData | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [connection, setConnection] = useState<EventConnectionState>("connecting");
  const [eventError, setEventError] = useState<string | null>(null);
  const reader = useRef<ReturnType<typeof createGoalReadQueue> | null>(null);
  const refresh = useCallback((verify = false, invalidated = false) => reader.current?.refresh(verify, invalidated) ?? Promise.resolve(), []);

  useEffect(() => {
    setData(null); setError(null); setEventError(null);
    const queue = createGoalReadQueue(async (verify, signal) => {
      setLoading(true);
      try {
        const next = await loadGoalTracking(graphId, { signal, force: verify });
        if (!signal.aborted) { setData(next); setError(null); observeGoalRead(next); }
      } catch (cause) {
        if (!signal.aborted) setError(cause instanceof Error ? cause.message : String(cause));
      } finally { if (!signal.aborted) setLoading(false); }
    });
    reader.current = queue;
    void queue.refresh();
    const invalidate = () => { void queue.refresh(false, true); };
    const controller = subscribeToEvents(event => {
      if (isTaskWorldEvent(event, graphId)) void queue.refresh(false, true);
    }, cause => setEventError(cause.message), state => {
      setConnection(state);
      if (state === "connected") { setEventError(null); invalidate(); }
    });
    const timer = window.setInterval(() => { void queue.refresh(); }, 3000);
    window.addEventListener("focus", invalidate);
    const visible = () => { if (document.visibilityState === "visible") invalidate(); };
    document.addEventListener("visibilitychange", visible);
    return () => {
      queue.close(); reader.current = null;
      controller.abort(); window.clearInterval(timer); window.removeEventListener("focus", invalidate);
      document.removeEventListener("visibilitychange", visible);
    };
  }, [graphId]);
  return { data, error, loading, refresh, connection, eventError };
}
