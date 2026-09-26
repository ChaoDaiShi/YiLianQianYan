import { useCallback, useEffect, useMemo, useState } from "react";
import { ArrowLeft, RefreshCw } from "lucide-react";
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
import { Button, EmptyState, ErrorState, PageHeader, Skeleton } from "../../components/ui";
import { useGlobalVoiceContext } from "../voice/GlobalVoiceHost";
import TaskWorldCanvas from "./canvas/TaskWorldCanvas";
import TaskInspector from "./inspector/TaskInspector";
import { useCanvasView } from "./hooks/useCanvasView";
import { useTaskCommands } from "./hooks/useTaskCommands";
import { useTaskWorldEvents } from "./hooks/useTaskEvents";
import { useTaskGraph } from "./hooks/useTaskGraph";
import { useTaskGraphReview } from "./hooks/useTaskReview";
import { isActiveExecution, projectTaskGraph } from "./taskGraphProjection";

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

  return (
    <div className="page-canvas flex h-full min-h-0 flex-col">
      <PageHeader
        title={`任务画布 · ${projection.graphId}`}
        description={`语义图 r${projection.revision} · 画布视图 r${view?.view_revision ?? "—"}`}
        actions={<div className="flex gap-2"><Button variant="ghost" size="sm" onClick={() => navigate("/tasks")}><ArrowLeft className="h-4 w-4" />任务中心</Button><Button variant="secondary" size="sm" disabled={saving || graphLocked} onClick={() => void mutate(() => addTaskNode(graphId, projection.revision, { id: crypto.randomUUID(), kind: "work", title: "新任务", input: { instruction: "", acceptance_criteria: [] }, retry_policy: { max_attempts: 1 } }))}>添加任务节点</Button><Button variant="secondary" size="sm" onClick={() => void reload()}><RefreshCw className="h-4 w-4" />刷新</Button></div>}
      />
      {eventWarning && <p className="mx-4 mt-2 text-xs text-[var(--warning-fg)]" role="status">实时事件暂不可用：{eventWarning}</p>}
      {canvasSaveError && <p className="mx-4 mt-2 text-xs text-[var(--danger-fg)]" role="alert">{canvasSaveError.message} <button type="button" className="underline" onClick={retrySave}>重试保存画布</button></p>}
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
      <div className="task-world-layout min-h-0 flex-1 px-4 pb-4 pt-3">
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
          /> : <EmptyState title="画布视图不可用" description="真实图已加载，但视觉状态尚未就绪。" className="py-20" />}
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
      </div>
    </div>
  );
}

function TaskWorldLoading() {
  return <div className="grid h-full grid-cols-[180px_1fr_300px] gap-3 p-4"><Skeleton className="h-full" /><Skeleton className="h-full" /><Skeleton className="h-full" /></div>;
}
