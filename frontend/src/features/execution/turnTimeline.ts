import type { ConversationTurn } from "../../api/contextManifest";
import type { ExecutionAuditEvent } from "../../api/executionAudit";
import type { PendingApproval } from "../../types/approval";
import { formatToolActionSummary } from "../../components/chat/toolDisplay";
import type { ExecutionRecord } from "./model";

export type TimelineTone = "neutral" | "waiting" | "success" | "error";
type TimelinePhase = "request" | "decision" | "start" | "dispatch" | "end" | "result" | "verification";

export interface TurnTimelineNode {
  id: string;
  actionId: string;
  phase: TimelinePhase;
  label: string;
  at?: number;
  tone: TimelineTone;
  detail: string;
  reason?: string;
}

export interface TimelineRange {
  node: TurnTimelineNode;
  start?: number;
  end?: number;
  live: boolean;
}

const approvalLabels: Record<string, string> = {
  approved: "已批准", rejected: "已拒绝", cancelled: "审批已取消", expired: "审批已过期",
};

export function projectTurnTimeline(turn: ConversationTurn, actions: ExecutionRecord[], audit: ExecutionAuditEvent[], approvals: PendingApproval[]) {
  const byCall = new Map<string, Map<string, ExecutionAuditEvent>>();
  for (const event of audit) {
    if (!event.tool_call_id || event.created_at < turn.started_at || (turn.finished_at != null && event.created_at > turn.finished_at)) continue;
    if (event.event_type === "policy_decided" && event.decision_status !== "deny") continue;
    const events = byCall.get(event.tool_call_id) ?? new Map<string, ExecutionAuditEvent>();
    if (!events.has(event.event_type)) events.set(event.event_type, event);
    byCall.set(event.tool_call_id, events);
  }
  const pendingTimes = new Map(approvals.map(approval => [approval.tool_call_id, Date.parse(approval.created_at)]));
  const timeline: (TurnTimelineNode & { orderAt: number })[] = [];
  for (const action of [...actions].sort((a, b) => a.sequence - b.sequence)) {
    const events = byCall.get(action.toolCallId);
    const requested = events?.get("approval_requested");
    const resolved = events?.get("approval_resolved");
    const started = events?.get("execution_started");
    const finished = events?.get("execution_finished");
    const verified = events?.get("verification_finished");
    const denied = events?.get("policy_decided");
    const decision = resolved?.decision_status ?? action.approvalStatus;
    const detail = formatToolActionSummary(action.name, action.args);
    const approvalTime = requested?.created_at ?? pendingTimes.get(action.toolCallId);
    const firstTimedStage = approvalTime ?? resolved?.created_at ?? started?.created_at ?? action.startedAt ?? denied?.created_at ?? finished?.created_at ?? action.finishedAt;
    let orderAt = firstTimedStage != null && Number.isFinite(firstTimedStage) ? firstTimedStage : turn.started_at;
    const add = (phase: TimelinePhase, label: string, tone: TimelineTone, timestamp?: number, reason?: string) => {
      const at = timestamp != null && Number.isFinite(timestamp) ? timestamp : undefined;
      orderAt = Math.max(orderAt, at ?? orderAt);
      timeline.push({ id: `${action.toolCallId}:${phase}`, actionId: action.toolCallId, phase, label, tone, detail, at, reason, orderAt });
    };

    if (requested || decision !== "not_required") {
      add("request", "等待审批", "waiting", approvalTime);
    }
    if (approvalLabels[decision]) {
      add("decision", approvalLabels[decision], decision === "approved" ? "success" : "error", resolved?.created_at,
        decision === "rejected" ? "用户拒绝了此次审批，工具未执行。" : undefined);
    }
    if (started) add("start", "开始执行", "neutral", started.created_at);
    else if (!denied && action.startedAt != null) add("dispatch", "开始处理", "neutral", action.startedAt);
    if (denied) {
      add("result", "执行被拦截", "error", denied.created_at, denied.details?.reason || action.result);
    } else if (finished || action.executionStatus === "succeeded" || action.executionStatus === "failed") {
      const failed = finished ? finished.decision_status === "failed" : action.executionStatus === "failed" && action.verificationStatus !== "failed";
      add(finished ? "end" : "result", failed ? "执行失败" : "执行结束", failed ? "error" : "neutral",
        finished?.created_at ?? action.finishedAt, failed ? action.result : undefined);
    } else if (action.executionStatus === "interrupted") {
      add("result", "执行中断", "error", action.finishedAt, action.result);
    }
    if (verified || action.verificationStatus === "passed" || action.verificationStatus === "failed") {
      const passed = verified ? verified.decision_status === "verified" : action.verificationStatus === "passed";
      add("verification", passed ? "已核验" : "核验失败", passed ? "success" : "error", verified?.created_at,
        passed ? undefined : action.verificationReason || verified?.details?.reason);
    }
  }
  return timeline.sort((a, b) => a.orderAt - b.orderAt);
}

export function projectTimelineTracks(nodes: TurnTimelineNode[], turn: ConversationTurn, now: number) {
  const grouped = new Map<string, Map<TimelinePhase, TurnTimelineNode>>();
  for (const node of nodes) {
    const phases = grouped.get(node.actionId) ?? new Map();
    phases.set(node.phase, node);
    grouped.set(node.actionId, phases);
  }

  return Array.from(grouped, ([id, phases]) => {
    const request = phases.get("request");
    const decision = phases.get("decision");
    const start = phases.get("start");
    const end = phases.get("end");
    const result = end ?? phases.get("result");
    const approvalNode = decision ?? request;
    const executionNode = result ?? start ?? phases.get("dispatch");
    const liveApproval = turn.finished_at == null && !decision && !executionNode;
    const liveExecution = turn.finished_at == null && !result && !phases.has("verification");
    const approval = approvalNode && range(approvalNode, request?.at, decision?.at, liveApproval, now);
    const execution = executionNode && range(executionNode, start?.at, end?.at, liveExecution, now);
    return { id, detail: (approvalNode ?? executionNode ?? phases.get("verification"))?.detail, approval, execution, verification: phases.get("verification") };
  });
}

function range(node: TurnTimelineNode, start: number | undefined, recordedEnd: number | undefined, live: boolean, now: number): TimelineRange {
  const end = recordedEnd ?? (live && start != null ? Math.max(start, now) : undefined);
  return { node, start, end, live: live && start != null };
}

export function timelineViewport(extent: number, zoom: number, center: number) {
  const width = extent / zoom;
  const start = Math.min(extent - width, Math.max(0, center - width / 2));
  return { start, end: start + width };
}

export function layoutTimelineLabels(positions: number[], gap: number) {
  const laneEnds: number[] = [];
  return positions.map(position => {
    const available = laneEnds.findIndex(end => position - end >= gap);
    const lane = available < 0 ? laneEnds.length : available;
    laneEnds[lane] = position;
    return lane;
  });
}

export function groupTimelinePoints<T extends { x: number; expanded?: boolean }>(points: T[], gap: number) {
  const groups: T[][] = [];
  for (const point of points) {
    const current = groups[groups.length - 1];
    const first = current?.[0];
    if (first && !first.expanded && !point.expanded && point.x - first.x <= gap) current.push(point);
    else groups.push([point]);
  }
  return groups;
}
