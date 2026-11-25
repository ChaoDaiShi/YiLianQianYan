import { useEffect } from "react";
import { useNavigate } from "react-router-dom";
import { subscribeToEvents } from "../../api/events";
import { showToast } from "../../components/ui/CyreneToast";
import { isTaskWorldEvent } from "./taskGraphProjection";
import { loadGoalTracking } from "./goal-view/goalTracking";
import { createGoalReadQueue } from "./goal-view/goalReadQueue";
import { goalNoticeSnapshot, goalReminderLedger, goalRemindersMuted, isLiveAttentionEvent, subscribeGoalReads, type GoalNoticeSnapshot } from "./goal-view/goalReminders";
import type { GoalTrackingData } from "./goal-view/goalTracking";

/** Application-level reminders consume existing goal events; they never dispatch actions. */
export default function GoalReminderHost() {
  const navigate = useNavigate();
  useEffect(() => {
    const readers = new Map<string, { queue: ReturnType<typeof createGoalReadQueue>; attention: boolean }>();
    const pending = new Map<string, GoalNoticeSnapshot>();
    const announce = (notice: GoalNoticeSnapshot) => {
      if (goalRemindersMuted(notice.graphId)) return;
      if (document.visibilityState !== "visible") { pending.set(notice.graphId, notice); return; }
      showToast(`${notice.title.slice(0, 60)}：${notice.message.slice(0, 160)}`, notice.attention || "info", undefined,
        { label: "查看任务", onClick: () => navigate(notice.href) });
    };
    const observed = (data: GoalTrackingData, liveAttention = false) => {
      if (!data.binding) return;
      const snapshot = goalNoticeSnapshot(data);
      if (!goalReminderLedger.isCurrentObservation(snapshot)) return;
      const notice = goalReminderLedger.observe(snapshot, goalRemindersMuted(snapshot.graphId), liveAttention);
      if (!snapshot.attention) pending.delete(snapshot.graphId);
      if (notice) announce(notice);
    };
    const unsubscribeReads = subscribeGoalReads(observed);
    const connection = subscribeToEvents(event => {
      const graphId = typeof event.payload.graph_id === "string" ? event.payload.graph_id : "";
      if (!graphId || !isTaskWorldEvent(event, graphId)) return;
      let reader = readers.get(graphId);
      if (!reader) {
        reader = { attention: false, queue: createGoalReadQueue(async (_verify, signal) => {
          const liveAttention = readers.get(graphId)?.attention ?? false;
          if (readers.has(graphId)) readers.get(graphId)!.attention = false;
          try {
            const data = await loadGoalTracking(graphId, { signal, observeOnly: true });
            if (signal.aborted || !data.binding) return;
            observed(data, liveAttention);
          } catch { /* The owning surface displays observation errors; events do not become error toasts. */ }
        }) };
        readers.set(graphId, reader);
      }
      reader.attention ||= isLiveAttentionEvent(event);
      void reader.queue.refresh(false, true);
    }, undefined, state => {
      if (state === "connected") for (const reader of readers.values()) void reader.queue.refresh(false, true);
    });
    const visible = () => {
      if (document.visibilityState !== "visible") return;
      for (const notice of pending.values()) announce(notice);
      pending.clear();
    };
    document.addEventListener("visibilitychange", visible);
    return () => { unsubscribeReads(); connection.abort(); for (const reader of readers.values()) reader.queue.close(); document.removeEventListener("visibilitychange", visible); };
  }, [navigate]);
  return null;
}
