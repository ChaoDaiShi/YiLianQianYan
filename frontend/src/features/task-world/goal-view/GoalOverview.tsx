import { useMemo } from "react";
import type { AcceptanceReport, CurrentDeliverableEvidence, EvidenceSnapshot, GoalContract } from "../../../api/proofloop";
import type { ArtifactRecord } from "../../../api/evidence";
import type { TaskGraphDetail, TaskNodeExecutionSummary } from "../../../api/taskWorld";
import { executionStatusLabel } from "../taskGraphProjection";
import DeliverableList from "./DeliverableList";
import GoalBlockers from "./GoalBlockers";
import GoalProgress from "./GoalProgress";
import { buildGoalViewState, type GoalViewPhase } from "./goalViewModel";

const PHASE_LABELS: Record<GoalViewPhase, string> = {
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

interface GoalOverviewProps {
  graphId: string;
  contract: GoalContract | null;
  detail: TaskGraphDetail;
  report: AcceptanceReport | null;
  snapshot: EvidenceSnapshot | null;
  currentEvidence: CurrentDeliverableEvidence[] | null;
  artifacts: ArtifactRecord[];
  artifactError: string | null;
  proofError: string | null;
  proofLoading: boolean;
  onDownloadArtifact: (artifact: ArtifactRecord) => Promise<void>;
  onViewExecution: (nodeId: string, executionId: string) => void;
}

function scrollTo(sectionId: string) {
  document.getElementById(sectionId)?.scrollIntoView({ behavior: "smooth", block: "start" });
}

function nodeExecutions(detail: TaskGraphDetail): Array<{ nodeId: string; nodeTitle: string; execution: TaskNodeExecutionSummary }> {
  return detail.nodes.flatMap((node) => {
    const entries = [...(node.execution_history ?? [])];
    if (node.latest_execution && !entries.some((item) => item.execution_id === node.latest_execution?.execution_id)) {
      entries.push(node.latest_execution);
    }
    return entries.map((execution) => ({ nodeId: node.id, nodeTitle: node.title, execution }));
  }).sort((left, right) => right.execution.updated_at - left.execution.updated_at);
}

export default function GoalOverview(props: GoalOverviewProps) {
  const state = buildGoalViewState(props.graphId, props.contract, props.detail, props.report, props.snapshot, null, props.currentEvidence);
  const executions = useMemo(() => nodeExecutions(props.detail), [props.detail]);

  return (
    <div className="goal-overview-scroll" data-testid="goal-overview">
      <div className="goal-overview-content">
        <header className="goal-view-card goal-view-hero">
          <div className="goal-view-section-heading">
            <div className="min-w-0">
              <span className="goal-view-eyebrow">任务目标</span>
              <h1>{props.proofLoading && !props.contract ? "正在读取目标…" : state.title}</h1>
              <p>{props.contract ? `当前方案 · ${props.detail.nodes.length} 个步骤 · 修订 r${props.detail.revision}` : props.proofLoading ? "正在读取任务目标和验收记录。" : "当前任务图 · 尚未关联可验证目标"}</p>
            </div>
            <span className={`goal-view-phase goal-view-phase-${state.phase}`}>{PHASE_LABELS[state.phase]}</span>
          </div>
          <div className="goal-view-actions" aria-label="目标快捷入口">
            <button type="button" onClick={() => scrollTo("goal-plan")}>查看方案</button>
            <button type="button" onClick={() => scrollTo("goal-deliverables")}>查看成果</button>
            <button type="button" onClick={() => scrollTo("goal-executions")}>查看执行记录</button>
          </div>
          {props.proofError && <p className="goal-view-error" role="alert">目标和验收状态加载失败：{props.proofError}</p>}
          {props.proofLoading && <p className="goal-view-artifact-meta" role="status">正在同步当前图、执行记录与验收结果…</p>}
        </header>

        <GoalProgress state={state} detail={props.detail} />
        <GoalBlockers state={state} loading={props.proofLoading && !props.contract} />

        <section className="goal-view-card" id="goal-plan" aria-label="当前任务方案">
          <div className="goal-view-section-heading">
            <div>
              <h2>当前方案</h2>
              <p>来自此任务图的当前修订，不另建目标或计划。</p>
            </div>
            <span className="goal-view-count">{props.detail.nodes.length} 步</span>
          </div>
          {props.detail.nodes.length ? (
            <ol className="goal-view-plan-list">
              {props.detail.nodes.map((node, index) => (
                <li key={node.id}>
                  <span className="goal-view-step-number">{index + 1}</span>
                  <div className="min-w-0 flex-1">
                    <h3>{node.title}</h3>
                    {node.instruction_summary && <p>{node.instruction_summary}</p>}
                    {node.executor_ref && <span className="goal-view-tool">{node.executor_ref}</span>}
                    {node.acceptance_criteria.length > 0 && <p>完成条件：{node.acceptance_criteria.join("；")}</p>}
                  </div>
                </li>
              ))}
            </ol>
          ) : <p className="goal-view-empty">当前任务图中没有已保存的方案步骤。</p>}
        </section>

        <DeliverableList
          deliverables={state.deliverables}
          artifacts={props.artifacts}
          snapshot={props.snapshot}
          detail={props.detail}
          report={props.report}
          artifactError={props.artifactError}
          onDownload={props.onDownloadArtifact}
          onViewExecution={props.onViewExecution}
        />

        <section className="goal-view-card" id="goal-executions" aria-label="任务执行记录">
          <div className="goal-view-section-heading">
            <div>
              <h2>执行记录</h2>
              <p>按执行时间倒序显示当前任务图中的真实记录。</p>
            </div>
            <span className="goal-view-count">{executions.length}</span>
          </div>
          {executions.length ? (
            <ol className="goal-view-execution-list">
              {executions.map(({ nodeId, nodeTitle, execution }) => (
                <li key={execution.execution_id}>
                  <div className="min-w-0 flex-1">
                    <strong>{nodeTitle}</strong>
                    <p>{executionStatusLabel(execution.status)} · 第 {execution.attempt} 次尝试{execution.updated_at ? ` · ${new Date(execution.updated_at).toLocaleString()}` : ""}</p>
                    {execution.result_summary && <p>{execution.result_summary}</p>}
                    {execution.error && <p className="goal-view-execution-error">{execution.error}</p>}
                  </div>
                  <button type="button" onClick={() => props.onViewExecution(nodeId, execution.execution_id)}>查看执行记录</button>
                </li>
              ))}
            </ol>
          ) : <p className="goal-view-empty">此任务图尚无执行记录。</p>}
        </section>
      </div>
    </div>
  );
}
