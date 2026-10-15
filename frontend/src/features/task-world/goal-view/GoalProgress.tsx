import type { TaskGraphDetail } from "../../../api/taskWorld";
import type { GoalViewState } from "./goalViewModel";

const PHASE_LABELS: Record<GoalViewState["phase"], string> = {
  planning: "规划中",
  needs_input: "需要补充信息",
  ready: "等待开始",
  running: "执行中",
  waiting_approval: "等待审批",
  verifying: "验收中",
  completed: "已通过验收",
  failed: "未通过",
  cancelled: "已取消",
  needs_revalidation: "需要重新验收",
};

function stageForNode(node: TaskGraphDetail["nodes"][number]): string {
  const execution = node.latest_execution;
  if (execution?.status === "waiting_approval") return "等待审批";
  if (execution?.status === "validating") return "校验结果";
  if (execution?.status === "running" || execution?.status === "dispatching" || execution?.status === "ready") return "执行中";
  if (node.status === "succeeded" || execution?.status === "succeeded") return "已完成";
  if (node.status === "failed" || execution?.status === "failed") return "失败";
  if (node.kind === "user_checkpoint") return "等待输入";
  if (node.status === "cancelled" || execution?.status === "cancelled") return "已取消";
  return "待处理";
}

export default function GoalProgress({ state, detail }: { state: GoalViewState; detail: TaskGraphDetail | null }) {
  const progress = state.totalNodes ? Math.round((state.completedNodes / state.totalNodes) * 100) : 0;
  return (
    <section className="goal-view-card" aria-label="目标进度">
      <div className="goal-view-section-heading">
        <div>
          <h2>当前进度</h2>
          <p>按任务图中的真实步骤和最近执行状态汇总。</p>
        </div>
        <strong className={`goal-view-phase goal-view-phase-${state.phase}`}>{PHASE_LABELS[state.phase]}</strong>
      </div>
      <div className="goal-view-progress-summary">
        <span>{state.completedNodes} / {state.totalNodes} 步完成</span>
        {state.totalNodes > 0 && <span>{progress}%</span>}
      </div>
      <div className="goal-view-progress-track" role="progressbar" aria-label="任务步骤完成度" aria-valuenow={progress} aria-valuemin={0} aria-valuemax={100}>
        <span style={{ width: `${progress}%` }} />
      </div>
      {detail?.nodes.length ? (
        <ol className="goal-view-step-list">
          {detail.nodes.map((node, index) => (
            <li key={node.id}>
              <span className="goal-view-step-number">{index + 1}</span>
              <span className="min-w-0 flex-1">
                <strong>{node.title}</strong>
                {node.result_summary && <span className="goal-view-step-summary">{node.result_summary}</span>}
              </span>
              <span className="goal-view-step-stage">{stageForNode(node)}</span>
            </li>
          ))}
        </ol>
      ) : (
        <p className="goal-view-empty">任务图中还没有步骤。</p>
      )}
    </section>
  );
}
