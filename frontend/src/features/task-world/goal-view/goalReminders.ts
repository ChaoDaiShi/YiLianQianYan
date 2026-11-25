import type { ProductEvent } from "../../../api/events";
import type { ToastType } from "../../../components/ui/CyreneToast";
import type { GoalTrackingData } from "./goalTracking";
import { goalActionPolicy } from "./goalActionPolicy";
import type { GoalViewPhase } from "./goalViewModel";

export interface GoalNoticeSnapshot { graphId: string; title: string; phase: GoalViewPhase; signature: string; attention: ToastType | null; message: string; href: string; observedAt?: number }
export function goalNoticeSnapshot(data: GoalTrackingData): GoalNoticeSnapshot {
  const { state, next } = goalActionPolicy(data);
  const human = state.blockers.some(item => item.code === "criterion_human_pending");
  const attention: ToastType | null = state.phase === "completed" ? "success" : state.phase === "failed" ? "error" : human || ["needs_input", "waiting_approval", "needs_revalidation"].includes(state.phase) ? "warning" : null;
  const message = state.phase === "completed" ? "产物已通过当前验收" : state.phase === "failed" ? state.blockers[0]?.message || "执行或验收失败" : human ? "成果需要人工复核" : state.blockers[0]?.message || "任务需要你的决定";
  return {
    graphId: data.detail.graph_id, title: state.title, phase: state.phase, attention, message, observedAt: data.observedAt,
    href: next.href || `/task-world/${encodeURIComponent(data.detail.graph_id)}${human ? "?panel=proofloop" : ""}`,
    signature: JSON.stringify([state.phase, state.blockers, state.deliverables.map(item => [item.id, item.artifactId, item.state]), data.detail.nodes.map(node => node.latest_execution?.execution_id), data.binding?.conversations?.map(item => [item.conversation_id, item.run_status, item.run_started_at, item.run_finished_at])]),
  };
}

/** Last observed notification identity only; authoritative task state remains in the backend. */
export class GoalReminderLedger {
  private observed = new Map<string, { signature: string; at: number }>();
  isCurrentObservation(state: GoalNoticeSnapshot): boolean {
    return (state.observedAt ?? 0) >= (this.observed.get(state.graphId)?.at ?? 0);
  }
  observe(state: GoalNoticeSnapshot, muted = false, liveAttention = false): GoalNoticeSnapshot | null {
    const previous = this.observed.get(state.graphId);
    if (!this.isCurrentObservation(state)) return null;
    this.observed.set(state.graphId, { signature: state.signature, at: state.observedAt ?? 0 });
    if (muted || !state.attention || previous?.signature === state.signature || (previous === undefined && !liveAttention)) return null;
    return state;
  }
}
export function isLiveAttentionEvent(event: ProductEvent): boolean {
  return ["task.execution.failed", "task.execution.waiting_approval", "task.node.failed", "task.waiting_user"].includes(event.type)
    || (event.type === "task.goal.changed" && (event.payload.change === "acceptance_updated"
      || event.payload.change === "conversation_status" && ["waiting_approval", "waiting_input", "failed", "interrupted"].includes(String(event.payload.status))));
}
const mutedKey = (graphId: string) => `yilian.goal-reminders.muted:${graphId}`;
export function goalRemindersMuted(graphId: string): boolean { return typeof window !== "undefined" && localStorage.getItem(mutedKey(graphId)) === "true"; }
export function setGoalRemindersMuted(graphId: string, muted: boolean) { localStorage.setItem(mutedKey(graphId), String(muted)); }
export const goalReminderLedger = new GoalReminderLedger();

const observers = new Set<(data: GoalTrackingData) => void>();
export function observeGoalRead(data: GoalTrackingData) { for (const listener of observers) listener(data); }
export function subscribeGoalReads(listener: (data: GoalTrackingData) => void) {
  observers.add(listener); return () => { observers.delete(listener); };
}
