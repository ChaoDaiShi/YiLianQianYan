import { Link } from "react-router-dom";
import type { GoalTrackingData } from "./goalTracking";
import { buildGoalViewState, GOAL_PHASE_LABELS, type GoalViewState } from "./goalViewModel";
import "./goalTracking.css";
import { DELIVERABLE_STATUS } from "./DeliverableList";

export function goalNodeDestination(graphId: string, nodeId?: string | null): string {
  return `/task-world/${encodeURIComponent(graphId)}${nodeId ? `?node=${encodeURIComponent(nodeId)}` : ""}`;
}

export function goalBlockerDestination(graphId: string, blocker: GoalViewState["blockers"][number]): string | null {
  if (blocker.conversationId) return `/chat/${encodeURIComponent(blocker.conversationId)}`;
  return blocker.nodeId ? goalNodeDestination(graphId, blocker.nodeId) : null;
}

export default function GoalTrackingSummary({ data, onDetails, inCanvas = false, children }: {
  data: GoalTrackingData; onDetails?: () => void; inCanvas?: boolean; children?: React.ReactNode;
}) {
  const { detail, binding, report, snapshot, currentEvidence } = data;
  const state = buildGoalViewState(detail.graph_id, binding?.contract ?? null, detail, report, snapshot, null, currentEvidence, binding?.conversations);
  const current = detail.nodes.filter(node => ["dispatching", "running", "waiting_approval", "validating"].includes(node.latest_execution?.status ?? "") || node.status === "running");
  const decisions = state.blockers.filter(item => ["task_waiting_approval", "task_needs_input", "criterion_human_pending", "goal_chat_waiting_approval", "goal_chat_waiting_input"].includes(item.code));
  const chatActivity = binding?.conversations?.filter(item => ["running", "waiting_approval", "waiting_input"].includes(item.run_status)).map(item => `对话 · ${item.title}`).join("、");
  const currentLabel = state.phase === "cancelled" ? "已停止，不再启动后续步骤" : state.phase === "paused" ? "已暂停，等待恢复" : current.map(node => node.title).join("、") || (chatActivity ? "正在处理目标对话" : state.phase === "completed" ? "产物已通过当前验收" : state.phase === "failed" ? "执行或验收失败，可查看失败记录" : state.phase === "verifying" ? "步骤完成，正在核验产物" : "等待执行或处理待办");
  const renderBlocker = (blocker: GoalViewState["blockers"][number], className?: string, key?: number) => {
    const target = goalBlockerDestination(detail.graph_id, blocker) ?? (inCanvas ? null : goalNodeDestination(detail.graph_id));
    return target ? <Link key={key} className={className} to={target}>{blocker.message}</Link> : <span key={key} className={className}>{blocker.message}</span>;
  };
  return <section className="goal-tracking-summary" aria-label="共享任务状态" data-graph-id={detail.graph_id} data-phase={state.phase}>
    <header><strong title={state.title}>{state.title}</strong><span className={`goal-view-phase goal-view-phase-${state.phase}`}>{GOAL_PHASE_LABELS[state.phase]}</span></header>
    <dl className="scrollbar-thin" tabIndex={0} aria-label="任务状态正文，可滚动查看">
      <div><dt>正在做</dt><dd><span className="goal-tracking-preview">{currentLabel}{chatActivity && state.phase !== "cancelled" && state.phase !== "paused" ? ` · ${chatActivity}` : ""}</span><span className="goal-tracking-count">{state.completedNodes}/{state.totalNodes} 步</span></dd></div>
      <div><dt>卡在哪里</dt><dd>{state.blockers.length ? <>
        {renderBlocker(state.blockers[0], "goal-tracking-preview")}
        {(state.blockers.length > 1 || state.blockers[0].message.length > 110) && <details className="goal-tracking-more"><summary>查看完整阻塞信息 · {state.blockers.length} 项</summary><ul>{state.blockers.map((blocker, index) => <li key={`${blocker.code}:${index}`}>{renderBlocker(blocker)}</li>)}</ul></details>}
      </> : "暂无阻塞"}</dd></div>
      <div><dt>需要决定</dt><dd>{decisions.length ? decisions.map((item, index) => renderBlocker(item, "goal-tracking-preview", index)) : "暂无待决定事项"}</dd></div>
      <div><dt>最终产物</dt><dd><span className="goal-tracking-preview">{state.deliverables.length ? state.deliverables.map(item => `${item.title}（${DELIVERABLE_STATUS[item.state]}）`).join("、") : "尚未登记产物"}</span></dd></div>
    </dl>
    <nav aria-label="共享任务操作">
      {onDetails && <button type="button" onClick={onDetails}>任务详情与成果</button>}
      {!inCanvas && <Link to={goalNodeDestination(detail.graph_id)}>打开任务画布</Link>}
      {binding && <Link to={`/workspaces/${encodeURIComponent(binding.contract.workspace_id)}?tab=tasks&graph=${encodeURIComponent(detail.graph_id)}`}>工作空间</Link>}
      {children}
    </nav>
  </section>;
}
