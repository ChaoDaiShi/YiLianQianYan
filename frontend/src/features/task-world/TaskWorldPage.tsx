import type { CapabilityDescriptor } from "../../api/capabilities";
import CapabilityPicker from "./capability-picker/CapabilityPicker";
import { useCanvasCapabilities } from "./capability-picker/useCanvasCapabilities";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Eye, FileCheck, ListTree, RefreshCw, Settings2, SlidersHorizontal, X } from "lucide-react";
import { useNavigate, useParams } from "react-router-dom";
import {
  addTaskEdge,
  addTaskNode,
  cancelTaskExecution,
  createTaskCheckpoint,
  deleteTaskEdge,
  deleteTaskNode,
  restoreTaskCheckpoint,
  rerunTaskFromNode,
  startTaskExecution,
  startTaskNode,
  updateTaskNode,
  type ApiError,
  type TaskExecutionStatus,
  type TaskGraphNodeDefinition,
} from "../../api/taskWorld";
import { getProviderReadiness } from "../../api/providerConnection";
import ContextRail, { type ContextRailStatusTone } from "../../components/layout/ContextRail";
import { Button, EmptyState, ErrorState, Skeleton } from "../../components/ui";
import { useGlobalVoiceContext } from "../voice/GlobalVoiceHost";
import StudioHeader from "./StudioHeader";
import "./canvas/studio.css";
import TaskWorldCanvas from "./canvas/TaskWorldCanvas";
import TaskInspector from "./inspector/TaskInspector";
import { useCanvasView } from "./hooks/useCanvasView";
import { useTaskCommands } from "./hooks/useTaskCommands";
import { useTaskWorldEvents } from "./hooks/useTaskEvents";
import { useTaskGraph } from "./hooks/useTaskGraph";
import { useTaskGraphReview } from "./hooks/useTaskReview";
import { getExecutorAvailability, isActiveExecution, projectTaskGraph } from "./taskGraphProjection";
import AcceptancePanel from "./proofloop/AcceptancePanel";
import RepairReviewPanel from "./proofloop/RepairReviewPanel";
import { formatAcceptanceReportDisplayStatus, normalizeAcceptanceReport } from "./proofloop/proofloopModel";
import {
  evaluateGoal,
  listGoalEvaluations,
  proposeRepair,
  getGraphGoal,
  getEvaluationDetail,
  type GraphGoalBinding,
  type EvidenceSnapshot,
  type AcceptanceReport,
  type CurrentDeliverableEvidence,
  type RepairProposal,
} from "../../api/proofloop";
import { downloadArtifact, listArtifactResults, type ArtifactRecord } from "../../api/evidence";
import { formatIdentifier } from "../../utils/identifiers";
import GoalOverview from "./goal-view/GoalOverview";
import "../../styles/proofloopGoalView.css";

type StudioPanel = "trail" | "inspector" | "tools" | "proofloop" | null;

/**
 * Task World page.
 *
 * Owns routing, the shared UI state the sections agree on (focus, saving,
 * save failure), and the composition of canvas, trail and inspector. All the
 * behaviour lives in `hooks/` and all the presentation in `inspector/` and
 * `TaskWorldCanvas`.
 */
export default function TaskWorldPage() {
  const { graphId = "" } = useParams();
  return <TaskWorldSession key={graphId} graphId={graphId} />;
}

function TaskWorldSession({ graphId }: { graphId: string }) {
  const navigate = useNavigate();
  const capabilities = useCanvasCapabilities();
  const [pickerOpen, setPickerOpen] = useState(false);
  const [activePanel, setActivePanel] = useState<StudioPanel>(null);
  const [viewMode, setViewMode] = useState<"goal" | "expert">(() =>
    typeof window !== "undefined" && window.localStorage.getItem(`task-world-view:${graphId}`) === "goal" ? "goal" : "expert",
  );
  const panelTrigger = useRef<HTMLElement | null>(null);
  const changePanel = useCallback((panel: StudioPanel) => {
    if (panel) panelTrigger.current = document.activeElement as HTMLElement;
    setActivePanel(panel);
    if (!panel) panelTrigger.current?.focus();
  }, []);
  useEffect(() => {
    if (!activePanel) return;
    const closePanel = (event: KeyboardEvent) => {
      // Disabling a focused save button can return focus to body. Preserve
      // Escape there, while leaving portals (voice/dialogs) to their own host.
      const target = event.target;
      if (event.key === "Escape" && !event.defaultPrevented
        && target instanceof Element
        && (target === document.body || target.closest(".canvas-studio"))) {
        changePanel(null);
      }
    };
    window.addEventListener("keydown", closePanel);
    return () => window.removeEventListener("keydown", closePanel);
  }, [activePanel, changePanel]);
  const { updateContext } = useGlobalVoiceContext();
  const [focusedNodeId, setFocusedNodeId] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState<ApiError | null>(null);
  const [modelUnavailable, setModelUnavailable] = useState(false);
  const [proofReport, setProofReport] = useState<AcceptanceReport | null>(null);
  const [proofCurrentEvidence, setProofCurrentEvidence] = useState<CurrentDeliverableEvidence[] | null>(null);
  const [proofGoal, setProofGoal] = useState<GraphGoalBinding | null>(null);
  const [proofSnapshot, setProofSnapshot] = useState<EvidenceSnapshot | null>(null);
  const [goalArtifacts, setGoalArtifacts] = useState<ArtifactRecord[]>([]);
  const [goalArtifactError, setGoalArtifactError] = useState<string | null>(null);
  const proofRequest = useRef<AbortController | null>(null);
  const [proofLoading, setProofLoading] = useState(false);
  const [proofError, setProofError] = useState<string | null>(null);
  const [activeRepairProposal, setActiveRepairProposal] = useState<RepairProposal | null>(null);
  const proofReportMeta = proofReport ? formatAcceptanceReportDisplayStatus(proofReport, proofGoal?.contract) : null;
  const currentEvidenceStale = Boolean(proofReport && proofGoal?.contract.deliverables.some((deliverable) => {
    const artifactId = proofSnapshot?.deliverables.find((mapping) => mapping.deliverable_id === deliverable.id)?.artifact_id;
    return !artifactId || !proofCurrentEvidence?.some((item) =>
      item.deliverable_id === deliverable.id && item.artifact_id === artifactId && item.status === "current",
    );
  }));
  const proofReportTone: ContextRailStatusTone | undefined = proofReportMeta
    ? (proofReportMeta.needsReevaluation || currentEvidenceStale ? "warning" : proofReportMeta.tone)
    : undefined;

  useEffect(() => {
    window.localStorage.setItem(`task-world-view:${graphId}`, viewMode);
  }, [graphId, viewMode]);

  useEffect(() => {
    updateContext({
      conversational_anchor: null,
      active_task: graphId || null,
    });
  }, [graphId, updateContext]);

  const handleNodesLoaded = useCallback((nodeIds: string[]) => {
    setFocusedNodeId((current) => current && nodeIds.includes(current) ? current : nodeIds[0] || null);
  }, []);

  const { detail, error, setError, refreshDetail } = useTaskGraph(graphId, handleNodesLoaded);
  const clearFocus = useCallback(() => setFocusedNodeId(null), []);
  const {
    view,
    canvasSaveError,
    canvasSavePending,
    retrySave,
    refreshView,
    updateLayouts,
    updateViewport,
    updateSelection,
    createVisualGroup,
    toggleVisualGroup,
    removeVisualGroup,
  } = useCanvasView(graphId, setError, setSaveError, focusedNodeId, clearFocus);

  const reload = useCallback(async () => {
    await Promise.all([refreshDetail(), refreshView()]);
  }, [refreshDetail, refreshView]);

  useEffect(() => { void reload(); }, [reload]);

  const mutate = useTaskCommands(reload, setSaving, setSaveError);

  useEffect(() => {
    let active = true;
    void getProviderReadiness().then((readiness) => {
      if (active) setModelUnavailable(!readiness?.model.available);
    });
    return () => { active = false; };
  }, []);

  const eventWarning = useTaskWorldEvents(graphId, refreshDetail, refreshView);

  const projection = useMemo(() => detail ? projectTaskGraph(detail, capabilities.items) : null, [detail, capabilities.items]);
  const selectedNode = projection?.nodes.find((node) => node.id === focusedNodeId) || null;
  const graphLocked = detail?.nodes.some((node) => isActiveExecution(node.latest_execution?.status)) || false;
  const executedNodeCount = projection?.nodes.filter((node) => node.latest_execution).length ?? 0;
  const selectedExecutionTone = executionStatusTone(selectedNode?.latest_execution?.status);

  const {
    review,
    setReview,
    reviewBusy,
    reviewError,
    runGraphReview,
    acceptReviewSuggestion,
  } = useTaskGraphReview(graphId, projection, reload, mutate, setModelUnavailable);

  // A small refresh repairs missed events while an execution is in flight.
  useEffect(() => {
    const timer = window.setInterval(() => {
      if (graphLocked) void refreshDetail();
    }, 2000);
    return () => window.clearInterval(timer);
  }, [graphLocked, refreshDetail]);

  const loadProofEvaluation = useCallback(async (force = false) => {
    if (!graphId) return;
    setProofCurrentEvidence(null);
    proofRequest.current?.abort();
    const controller = new AbortController();
    proofRequest.current = controller;
    setProofLoading(true);
    setProofError(null);
    try {
      const binding = await getGraphGoal(graphId, controller.signal);
      if (controller.signal.aborted) return;
      if (!binding.ok) { setProofGoal(null); setProofReport(null); setProofSnapshot(null); setProofCurrentEvidence(null); throw new Error(binding.error.message); }
      setProofGoal(binding.data);
      const res = await listGoalEvaluations(binding.data.goal_id, controller.signal);
      if (!res.ok) throw new Error(res.error.message);
      const execIds = detail?.nodes.map((node) => node.latest_execution?.execution_id).filter((id): id is string => Boolean(id)) || [];
      let report = res.data.reports.find((item) => item.graph_revision === detail?.revision && item.goal_version === binding.data.goal_version);
      if (report && !force) {
        const evidence = await getEvaluationDetail(report.report_id, controller.signal);
        if (!evidence.ok) throw new Error(evidence.error.message);
        if (execIds.some((id) => !evidence.data.snapshot.executions.some((execution) => execution.execution_id === id))) report = undefined;
        else if (!controller.signal.aborted) {
          setProofSnapshot(evidence.data.snapshot);
          setProofCurrentEvidence(evidence.data.current_evidence ?? []);
        }
      }
      if (!report || force) {
        setProofReport(null);
        setProofSnapshot(null);
        setProofCurrentEvidence(null);
        if (detail?.nodes.some((node) => isActiveExecution(node.latest_execution?.status))) return;
        if (execIds.length === 0 || !detail) return;
        const result = await evaluateGoal(binding.data.goal_id, { goal_version: binding.data.goal_version, graph_revision: detail.revision, execution_ids: execIds }, controller.signal);
        if (!result.ok) throw new Error(result.error.message);
        report = result.data;
        const evidence = await getEvaluationDetail(report.report_id, controller.signal);
        if (!evidence.ok) throw new Error(evidence.error.message);
        if (!controller.signal.aborted) {
          setProofSnapshot(evidence.data.snapshot);
          setProofCurrentEvidence(evidence.data.current_evidence ?? []);
        }
      }
      if (!controller.signal.aborted) setProofReport(report ? normalizeAcceptanceReport(report) : null);
    } catch (e: unknown) {
      if (!controller.signal.aborted) setProofError(e instanceof Error ? e.message : String(e));
    } finally {
      if (!controller.signal.aborted) setProofLoading(false);
    }
  }, [graphId, detail]);

  useEffect(() => {
    if (activePanel === "proofloop" || viewMode === "goal") {
      void loadProofEvaluation();
    }
    return () => proofRequest.current?.abort();
  }, [activePanel, viewMode, loadProofEvaluation]);

  useEffect(() => {
    let active = true;
    if (!proofSnapshot) {
      setGoalArtifacts([]);
      setGoalArtifactError(null);
      return () => { active = false; };
    }
    const executions = [...new Map(proofSnapshot.executions.map((execution) => [execution.execution_id, execution])).values()];
    if (!executions.length) {
      setGoalArtifacts([]);
      setGoalArtifactError(null);
      return () => { active = false; };
    }
    void Promise.all(executions.map((execution) =>
      listArtifactResults({ kind: "node", graph_id: execution.graph_id, node_id: execution.node_id, execution_id: execution.execution_id }),
    )).then((records) => {
      if (!active) return;
      setGoalArtifacts(records.flat());
      setGoalArtifactError(null);
    }).catch((error: unknown) => {
      if (!active) return;
      setGoalArtifacts([]);
      setGoalArtifactError(error instanceof Error ? error.message : "读取成果记录失败。");
    });
    return () => { active = false; };
  }, [proofSnapshot]);

  if (!graphId) return <ErrorState title="任务图地址无效" description="缺少 graph id。" />;
  if (error && !detail) return <ErrorState title="任务图暂时无法加载" description={error.message} action={<Button onClick={() => void reload()}>重试</Button>} />;
  if (!projection) return <TaskWorldLoading />;

  const addNode = () => { void mutate(() => addTaskNode(graphId, projection.revision, { id: crypto.randomUUID(), kind: "work", title: "新任务", input: { instruction: "", acceptance_criteria: [] }, retry_policy: { max_attempts: 1 } })); };
  const addCapability = async (tool: CapabilityDescriptor) => {
    const id = crypto.randomUUID();
    if (await mutate(() => addTaskNode(graphId, projection.revision, {id,kind:"work",title:tool.name,input:{executor_ref:`capability://${tool.id}`,capability_input:{}},retry_policy:{max_attempts:1}}))) {
      setPickerOpen(false); setFocusedNodeId(id); changePanel("inspector");
    }
  };
  const canRun = !!selectedNode && !saving && !graphLocked && getExecutorAvailability(selectedNode.executor_ref, selectedNode.capability).kind === "configured";
  const runSelected = () => { if (canRun && selectedNode) void mutate(() => startTaskExecution(graphId, selectedNode.id, projection.revision)); };

  const downloadGoalArtifact = async (artifact: ArtifactRecord) => {
    const blob = await downloadArtifact(artifact.id);
    const url = URL.createObjectURL(blob);
    const link = document.createElement("a");
    link.href = url;
    link.download = artifact.name;
    link.click();
    window.setTimeout(() => URL.revokeObjectURL(url), 1000);
  };

  const handleRequestRepair = async (report: AcceptanceReport) => {
    const failedExecIds = detail?.nodes
      .filter((n) => n.latest_execution?.status === "failed")
      .map((n) => n.latest_execution?.execution_id)
      .filter((id): id is string => Boolean(id)) || [];
    if (!proofGoal) return;
    const res = await proposeRepair(proofGoal.goal_id, {
      report_id: report.report_id,
      failure_execution_ids: failedExecIds,
    });
    if (res.ok) {
      setActiveRepairProposal(res.data);
    } else {
      setProofError(res.error.message || "生成修复建议失败。");
    }
  };

  return (
    <div className="canvas-studio page-canvas">
      <CapabilityPicker open={pickerOpen} onClose={()=>setPickerOpen(false)} items={capabilities.items} onSelect={tool=>void addCapability(tool)} onRefresh={()=>void capabilities.refresh(true)} loading={capabilities.loading} error={capabilities.error}/>
      <StudioHeader graphId={projection.graphId} revision={view?.view_revision ?? null} pending={canvasSavePending} error={canvasSaveError?.message ?? null} onRetry={retrySave} onBack={()=>navigate("/tasks")}/>
      <div className="studio-view-switch" role="group" aria-label="任务工作台视图">
        <Button type="button" size="sm" variant={viewMode === "goal" ? "secondary" : "ghost"} aria-pressed={viewMode === "goal"} onClick={() => setViewMode("goal")}><Eye size={14}/>目标视图</Button>
        <Button type="button" size="sm" variant={viewMode === "expert" ? "secondary" : "ghost"} aria-pressed={viewMode === "expert"} onClick={() => setViewMode("expert")}><Settings2 size={14}/>专家视图</Button>
      </div>
      <div className="studio-workspace task-world-layout">
      {(eventWarning || (saveError && activePanel !== "inspector")) && <div className="studio-notices">
        {eventWarning && <p className="studio-event-warning" role="status">实时事件暂不可用：{eventWarning}</p>}
        {saveError && activePanel !== "inspector" && <p className="studio-command-error" role="alert">{saveError.message}</p>}
      </div>}
      <ContextRail
        className="studio-context-rail"
        label="画布上下文"
        items={[
          {
            id: "inspector",
            label: "节点属性",
            icon: Settings2,
            controls: "studio-inspector",
            expanded: activePanel === "inspector",
            statusTone: selectedExecutionTone,
            onSelect: () => changePanel(activePanel === "inspector" ? null : "inspector"),
          },
          {
            id: "trail",
            label: "执行轨迹",
            icon: ListTree,
            controls: "studio-trail",
            expanded: activePanel === "trail",
            badge: executedNodeCount > 99 ? "99+" : executedNodeCount || undefined,
            onSelect: () => changePanel(activePanel === "trail" ? null : "trail"),
          },
          {
            id: "tools",
            label: "画布工具",
            icon: SlidersHorizontal,
            controls: "studio-tools",
            expanded: activePanel === "tools",
            onSelect: () => changePanel(activePanel === "tools" ? null : "tools"),
          },
          {
            id: "proofloop",
            label: "证行验收",
            icon: FileCheck,
            controls: "studio-proofloop",
            expanded: activePanel === "proofloop",
            statusTone: proofReportTone,
            onSelect: () => changePanel(activePanel === "proofloop" ? null : "proofloop"),
          },
        ]}
      />
      <aside id="studio-tools" className="studio-panel studio-tools" aria-label="更多画布工具" hidden={activePanel!=="tools"}>
        <div className="studio-panel-heading"><h2>画布工具</h2><button type="button" aria-label="关闭更多工具" onClick={()=>changePanel(null)}><X size={16}/></button></div>
        <Button variant="secondary" size="sm" onClick={()=>{void reload();void capabilities.refresh(true);}}><RefreshCw size={14}/>刷新</Button>
      {modelUnavailable && <p className="mx-4 mt-2 text-xs text-[var(--warning-fg)]">还没有配置可用的模型服务。 <button type="button" className="underline" onClick={() => navigate("/settings?section=model")}>前往模型设置</button></p>}
      {view && <div className="mx-4 mt-2 flex flex-wrap items-center gap-2 rounded-[var(--radius-md)] border border-[var(--border-soft)] bg-[var(--surface-elevated)] px-3 py-2" aria-label="画布视图工具">
        <Button size="sm" variant="secondary" disabled={saving || view.selection.filter((nodeId) => !view.groups.some((group) => group.node_ids.includes(nodeId))).length < 2} onClick={createVisualGroup}>创建分组</Button>
        <Button size="sm" variant="secondary" disabled={reviewBusy} onClick={() => void runGraphReview()}>{reviewBusy ? "审查中…" : "AI 审查"}</Button>
        {view.groups.map((group) => <span key={group.id} className="inline-flex items-center gap-1 rounded-full border border-[var(--border-soft)] px-2 py-1 text-xs">
          {group.title}（{group.node_ids.length}）
          <button type="button" disabled={saving} onClick={() => toggleVisualGroup(group.id)}>{group.collapsed ? "展开" : "折叠"}</button>
          <button type="button" disabled={saving} onClick={() => removeVisualGroup(group.id)}>解散</button>
        </span>)}
      </div>}
      {reviewError && <p className="mx-4 mt-2 text-xs text-[var(--warning-fg)]" role="status">{reviewError}</p>}
      {review && <section className="mx-4 mt-2 rounded-[var(--radius-md)] border border-[var(--border-soft)] bg-[var(--surface-elevated)] p-3" aria-label="AI 图审查建议">
        <p className="text-sm font-medium">{review.summary}</p>
        {review.suggestions.length === 0 ? <p className="mt-2 text-xs text-[var(--text-secondary)]">未发现需要修改的节点。</p> : <div className="mt-2 grid gap-2 md:grid-cols-2">
          {review.suggestions.map((suggestion) => <article key={suggestion.suggestion_id} className="rounded-[var(--radius-md)] bg-[var(--surface-muted)] p-3 text-xs">
            <strong>{suggestion.title}</strong>
            <p className="mt-1 text-[var(--text-secondary)]">{suggestion.reason}</p>
            <p className="mt-1 line-clamp-3">{suggestion.instruction}</p>
            <div className="mt-2 flex gap-2">
              <Button size="sm" disabled={saving || graphLocked} onClick={() => void acceptReviewSuggestion(suggestion)}>接受建议</Button>
              <Button size="sm" variant="secondary" disabled={saving} onClick={() => setReview((current) => current ? { ...current, suggestions: current.suggestions.filter((item) => item.suggestion_id !== suggestion.suggestion_id) } : null)}>拒绝建议</Button>
            </div>
          </article>)}
        </div>}
      </section>}
      </aside>
      <aside id="studio-proofloop" className="studio-panel studio-proofloop" aria-label="目标验收与修复" hidden={activePanel!=="proofloop"}>
        <div className="studio-panel-heading"><h2>目标验收与修复</h2><button type="button" aria-label="关闭证行面板" onClick={()=>changePanel(null)}><X size={16}/></button></div>
        <div className="p-3">
          {proofLoading && (
            <div className="mb-2 text-xs text-[var(--text-faint)]">正在加载验收与证据状态…</div>
          )}
          {proofError && (
            <div className="mb-3 rounded-[var(--radius-md)] border border-[var(--danger-border)] bg-[var(--danger-soft)] p-2 text-xs text-[var(--danger-fg)]" role="alert">
              {proofError}
            </div>
          )}
          {proofGoal && <div className="mb-3 rounded-[var(--radius-md)] border border-[var(--border-soft)] p-2 text-xs">
            <span>{proofGoal.contract.budget.time_basis === "wall_clock" ? "总时限（含人工等待）" : "活动预算（人工等待不计入）"}：
              {proofGoal.remaining_ms != null ? `剩余 ${Math.ceil(proofGoal.remaining_ms / 1000)} 秒` : "当前剩余时间不可用"}</span>
            {proofGoal.time_usage && <p className="mt-1 text-[var(--text-faint)]">规划 {proofGoal.time_usage.planning_elapsed_ms == null ? "未知" : `${Math.ceil(proofGoal.time_usage.planning_elapsed_ms / 1000)} 秒`} · 执行活动 {Math.ceil(proofGoal.time_usage.execution_active_ms / 1000)} 秒
              {proofGoal.time_usage.accounting_state === "legacy_unknown" ? " · 旧规划用量未完整计量" : ""}</p>}
            {proofGoal.budget_error && <p className="mt-1 text-[var(--danger-fg)]">{proofGoal.budget_error.message}</p>}
          </div>}
          {activeRepairProposal ? (
            <RepairReviewPanel
              proposal={activeRepairProposal}
              currentGraphRevision={projection.revision}
              onApplyRepair={() => {
                setActiveRepairProposal(null);
                setProofReport(null);
                setProofSnapshot(null);
                setProofCurrentEvidence(null);
                void reload();
              }}
              onCancel={() => setActiveRepairProposal(null)}
            />
          ) : (
            <AcceptancePanel
              goalId={proofGoal?.goal_id || graphId}
              report={proofReport}
              evidenceSnapshot={proofSnapshot}
              contract={proofGoal?.contract}
              onRefreshEvaluation={() => void loadProofEvaluation(true)}
              onReportUpdated={(report) => { setProofReport(normalizeAcceptanceReport(report)); }}
              onOpenArtifact={(id) => { void downloadArtifact(id).then((blob) => { const url = URL.createObjectURL(blob); const link = document.createElement("a"); link.href = url; link.download = `${formatIdentifier("artifact", id)}.md`; link.click(); setTimeout(() => URL.revokeObjectURL(url), 1000); }).catch((error: unknown) => setProofError(error instanceof Error ? error.message : String(error))); }}
              onRequestRepair={(rep) => void handleRequestRepair(rep)}
            />
          )}
        </div>
      </aside>
          <div className="studio-expert-view" hidden={viewMode !== "expert"}>
            {view ? <TaskWorldCanvas
              projection={projection}
              view={view}
              focusedNodeId={focusedNodeId}
              onFocusNode={setFocusedNodeId}
              onLayoutSave={updateLayouts}
              onViewportChange={updateViewport}
              onSelectionChange={updateSelection}
              onConnect={(connection) => { if (connection.source && connection.target) void mutate(() => addTaskEdge(graphId, projection.revision, connection.source!, connection.target!)); }}
              onDeleteNodes={(nodes) => { const node = nodes[0]; if (node) void mutate(() => deleteTaskNode(graphId, node.id, projection.revision)); }}
              onDeleteEdges={(edges) => { const edge = edges[0]; if (edge) void mutate(() => deleteTaskEdge(graphId, edge.source, edge.target, projection.revision)); }}
              semanticLocked={graphLocked}
              trailOpen={activePanel==="trail"}
              onCloseTrail={()=>changePanel(null)}
              onAddNode={addNode}
              onAddCapability={()=>{setPickerOpen(true);void capabilities.refresh(true);}}
              canAddNode={!saving && !graphLocked}
              onRunSelected={runSelected}
              canRunSelected={canRun}
              runLabel={canRun ? `运行：${selectedNode?.title}` : "选择已配置执行器的节点；执行期间不可重复运行"}
            /> : <EmptyState title="画布视图不可用" description="真实图已加载，但视觉状态尚未就绪。" className="py-20" />}
          </div>
          <div className="studio-goal-view" hidden={viewMode !== "goal"}>
            {detail && <GoalOverview
              graphId={graphId}
              contract={proofGoal?.contract ?? null}
              detail={detail}
              report={proofReport}
              snapshot={proofSnapshot}
              currentEvidence={proofCurrentEvidence}
              artifacts={goalArtifacts}
              artifactError={goalArtifactError}
              proofError={proofError}
              proofLoading={proofLoading}
              onDownloadArtifact={downloadGoalArtifact}
              onViewExecution={(nodeId) => {
                setFocusedNodeId(nodeId);
                setViewMode("expert");
                changePanel("inspector");
              }}
            />}
          </div>
        <section id="studio-inspector" className="studio-panel studio-inspector" aria-label="节点属性面板" hidden={activePanel!=="inspector"}>
        <div className="studio-panel-heading"><h2>节点属性</h2><button type="button" aria-label="关闭节点属性" onClick={()=>changePanel(null)}><X size={16}/></button></div>
        <TaskInspector
          graphId={graphId}
          node={selectedNode}
          capabilities={capabilities}
          expectedRevision={projection.revision}
          revisions={projection.revisions}
          checkpoints={projection.checkpoints}
          dependencyEdges={projection.edges}
          dependencyCandidates={projection.nodes}
          saving={saving}
          saveError={saveError}
          graphLocked={graphLocked}
          onSave={(draft) => mutate(() => updateTaskNode(graphId, selectedNode!.id, projection.revision, draft as Pick<TaskGraphNodeDefinition, "kind" | "title" | "input" | "retry_policy">)).then(() => undefined)}
          onStart={() => mutate(() => startTaskNode(graphId, selectedNode!.id, projection.revision)).then(() => undefined)}
          onStartExecution={() => mutate(() => startTaskExecution(graphId, selectedNode!.id, projection.revision)).then(() => undefined)}
          onCancelExecution={() => mutate(() => cancelTaskExecution(
            graphId,
            selectedNode!.latest_execution!.execution_id,
            projection.revision,
          )).then(() => undefined)}
          onRerun={() => mutate(() => rerunTaskFromNode(
            graphId,
            selectedNode!.id,
            projection.revision,
          )).then(() => undefined)}
          onCheckpoint={() => mutate(() => createTaskCheckpoint(graphId, projection.revision)).then(() => undefined)}
          onRestore={(checkpointId) => mutate(() => restoreTaskCheckpoint(graphId, checkpointId, projection.revision)).then(() => undefined)}
          onAddDependency={(from, to) => mutate(() => addTaskEdge(graphId, projection.revision, from, to)).then(() => undefined)}
          onRemoveDependency={(from, to) => mutate(() => deleteTaskEdge(graphId, from, to, projection.revision)).then(() => undefined)}
        />
        </section>
      </div>
    </div>
  );
}

function TaskWorldLoading() {
  return <div className="flex h-full flex-col gap-3 p-4"><Skeleton className="h-12" /><Skeleton className="min-h-0 flex-1" /></div>;
}

function executionStatusTone(status: TaskExecutionStatus | null | undefined): ContextRailStatusTone {
  if (status === "failed" || status === "blocked") return "danger";
  if (status === "waiting_approval" || status === "stale") return "warning";
  if (status === "dispatching" || status === "running" || status === "validating") return "info";
  if (status === "succeeded") return "success";
  return "neutral";
}
