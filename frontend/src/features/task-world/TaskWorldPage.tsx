import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { X, RefreshCw } from "lucide-react";
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
  type TaskGraphNodeDefinition,
} from "../../api/taskWorld";
import { getProviderReadiness } from "../../api/providerConnection";
import { Button, EmptyState, ErrorState, Skeleton } from "../../components/ui";
import { useGlobalVoiceContext } from "../voice/GlobalVoiceHost";
import StudioHeader, { type StudioPanel } from "./StudioHeader";
import "./canvas/studio.css";
import TaskWorldCanvas from "./canvas/TaskWorldCanvas";
import TaskInspector from "./inspector/TaskInspector";
import { useCanvasView } from "./hooks/useCanvasView";
import { useTaskCommands } from "./hooks/useTaskCommands";
import { useTaskWorldEvents } from "./hooks/useTaskEvents";
import { useTaskGraph } from "./hooks/useTaskGraph";
import { useTaskGraphReview } from "./hooks/useTaskReview";
import { getExecutorAvailability, isActiveExecution, projectTaskGraph } from "./taskGraphProjection";

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
  const [activePanel, setActivePanel] = useState<StudioPanel>(null);
  const panelTrigger = useRef<HTMLElement | null>(null);
  const changePanel = (panel: StudioPanel) => {
    if (panel) panelTrigger.current = document.activeElement as HTMLElement;
    setActivePanel(panel);
    if (!panel) panelTrigger.current?.focus();
  };
  const { updateContext } = useGlobalVoiceContext();
  const [focusedNodeId, setFocusedNodeId] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState<ApiError | null>(null);
  const [modelUnavailable, setModelUnavailable] = useState(false);

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

  const projection = useMemo(() => detail ? projectTaskGraph(detail) : null, [detail]);
  const selectedNode = projection?.nodes.find((node) => node.id === focusedNodeId) || null;
  const graphLocked = detail?.nodes.some((node) => isActiveExecution(node.latest_execution?.status)) || false;

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

  if (!graphId) return <ErrorState title="任务图地址无效" description="缺少 graph id。" />;
  if (error && !detail) return <ErrorState title="任务图暂时无法加载" description={error.message} action={<Button onClick={() => void reload()}>重试</Button>} />;
  if (!projection) return <TaskWorldLoading />;

  const addNode = () => { void mutate(() => addTaskNode(graphId, projection.revision, { id: crypto.randomUUID(), kind: "work", title: "新任务", input: { instruction: "", acceptance_criteria: [] }, retry_policy: { max_attempts: 1 } })); };
  const canRun = !!selectedNode && !saving && !graphLocked && getExecutorAvailability(selectedNode.executor_ref).kind === "configured";
  const runSelected = () => { if (canRun && selectedNode) void mutate(() => startTaskExecution(graphId, selectedNode.id, projection.revision)); };

  return (
    <div className="canvas-studio page-canvas" onKeyDown={(event) => { if (event.key === "Escape" && activePanel) { event.stopPropagation(); changePanel(null); } }}>
      <StudioHeader graphId={projection.graphId} revision={view?.view_revision ?? null} pending={canvasSavePending} error={canvasSaveError?.message ?? null} onRetry={retrySave} onBack={()=>navigate("/tasks")} panel={activePanel} onPanel={changePanel}/>
      <div className="studio-workspace task-world-layout">
      {(eventWarning || (saveError && activePanel !== "inspector")) && <div className="studio-notices">
        {eventWarning && <p className="studio-event-warning" role="status">实时事件暂不可用：{eventWarning}</p>}
        {saveError && activePanel !== "inspector" && <p className="studio-command-error" role="alert">{saveError.message}</p>}
      </div>}
      <aside id="studio-tools" className="studio-panel studio-tools" aria-label="更多画布工具" hidden={activePanel!=="tools"}>
        <div className="studio-panel-heading"><h2>画布工具</h2><button type="button" aria-label="关闭更多工具" onClick={()=>changePanel(null)}><X size={16}/></button></div>
        <Button variant="secondary" size="sm" onClick={()=>void reload()}><RefreshCw size={14}/>刷新</Button>
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
            canAddNode={!saving && !graphLocked}
            onRunSelected={runSelected}
            canRunSelected={canRun}
            runLabel={canRun ? `运行：${selectedNode?.title}` : "选择已配置执行器的节点；执行期间不可重复运行"}
          /> : <EmptyState title="画布视图不可用" description="真实图已加载，但视觉状态尚未就绪。" className="py-20" />}
        <section id="studio-inspector" className="studio-panel studio-inspector" aria-label="节点属性面板" hidden={activePanel!=="inspector"}>
        <div className="studio-panel-heading"><h2>节点属性</h2><button type="button" aria-label="关闭节点属性" onClick={()=>changePanel(null)}><X size={16}/></button></div>
        <TaskInspector
          graphId={graphId}
          node={selectedNode}
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
