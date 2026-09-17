import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { ArrowLeft, RefreshCw } from "lucide-react";
import { useNavigate, useParams } from "react-router-dom";
import {
  addTaskEdge,
  addTaskNode,
  cancelTaskExecution,
  createTaskCheckpoint,
  deleteTaskEdge,
  deleteTaskNode,
  getCanvasView,
  getTaskGraphDetail,
  restoreTaskCheckpoint,
  reviewTaskGraph,
  saveCanvasView,
  startTaskExecution,
  startTaskNode,
  rerunTaskFromNode,
  updateTaskNode,
  type ApiError,
  type CanvasNodeLayout,
  type CanvasView,
  type CanvasViewport,
  type TaskGraphNodeDefinition,
  type TaskGraphDetail,
  type TaskGraphReview,
  type TaskGraphReviewSuggestion,
} from "../../api/taskWorld";
import { subscribeToEvents } from "../../api/events";
import { Button, EmptyState, ErrorState, PageHeader, Panel, Skeleton } from "../../components/ui";
import TaskExecutionTrail from "./TaskExecutionTrail";
import TaskWorldCanvas from "./TaskWorldCanvas";
import TaskWorldInspector from "./TaskWorldInspector";
import { buildAutoLayout, isActiveExecution, isTaskWorldEvent, projectTaskGraph } from "./taskGraphProjection";
import { createCanvasViewWriteQueue } from "./canvasViewWriter";
import { useGlobalVoiceContext } from "../voice/GlobalVoiceHost";

export default function TaskWorldPage() {
  const { graphId = "" } = useParams();
  const navigate = useNavigate();
  const { updateContext } = useGlobalVoiceContext();
  const [detail, setDetail] = useState<TaskGraphDetail | null>(null);
  const [view, setView] = useState<CanvasView | null>(null);
  const [focusedNodeId, setFocusedNodeId] = useState<string | null>(null);
  const [error, setError] = useState<ApiError | null>(null);
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState<ApiError | null>(null);
  const [eventWarning, setEventWarning] = useState<string | null>(null);
  const [review, setReview] = useState<TaskGraphReview | null>(null);
  const [reviewBusy, setReviewBusy] = useState(false);
  const [reviewError, setReviewError] = useState("");
  const detailSequence = useRef(0);
  const viewRef = useRef<CanvasView | null>(null);

  useEffect(() => {
    updateContext({
      conversational_anchor: null,
      active_task: graphId || null,
    });
  }, [graphId, updateContext]);

  const acceptView = useCallback((incoming: CanvasView) => {
    const current = viewRef.current;
    if (
      current
      && current.graph_id === incoming.graph_id
      && current.view_revision > incoming.view_revision
    ) return;
    viewRef.current = incoming;
    setView(incoming);
  }, []);

  const refreshDetail = useCallback(async () => {
    if (!graphId) return;
    const sequence = ++detailSequence.current;
    const result = await getTaskGraphDetail(graphId);
    if (sequence !== detailSequence.current) return;
    if (result.ok) {
      setDetail(result.data);
      setError(null);
      setFocusedNodeId((current) => current && result.data.nodes.some((node) => node.id === current) ? current : result.data.nodes[0]?.id || null);
    } else {
      setError(result.error);
    }
  }, [graphId]);

  const refreshView = useCallback(async () => {
    if (!graphId) return;
    const result = await getCanvasView(graphId);
    if (result.ok) acceptView(result.data);
    else setError(result.error);
  }, [acceptView, graphId]);

  const reload = useCallback(async () => {
    await Promise.all([refreshDetail(), refreshView()]);
  }, [refreshDetail, refreshView]);

  useEffect(() => { void reload(); }, [reload]);

  useEffect(() => {
    if (!graphId) return;
    const controller = subscribeToEvents((event) => {
      if (!isTaskWorldEvent(event, graphId)) return;
      // Running is intentionally treated as invalidation: the authoritative
      // projection is refreshed rather than reproducing TaskSupervisor in React.
      if (event.type === "task.node.running") void refreshDetail();
      else if (event.type === "task.canvas.updated") void refreshView();
      else void refreshDetail();
    }, (streamError) => setEventWarning(streamError.message));
    return () => controller.abort();
  }, [graphId, refreshDetail, refreshView]);

  const projection = useMemo(() => detail ? projectTaskGraph(detail) : null, [detail]);
  const selectedNode = projection?.nodes.find((node) => node.id === focusedNodeId) || null;
  const graphLocked = detail?.nodes.some((node) => isActiveExecution(node.latest_execution?.status)) || false;

  // A small refresh repairs missed events while an execution is in flight.
  useEffect(() => {
    const timer = window.setInterval(() => {
      if (graphLocked) void refreshDetail();
    }, 2000);
    return () => window.clearInterval(timer);
  }, [graphLocked, refreshDetail]);

  const mutate = useCallback(async (operation: () => Promise<{ ok: true; data: unknown } | { ok: false; error: ApiError }>) => {
    setSaving(true);
    setSaveError(null);
    const result = await operation();
    setSaving(false);
    if (!result.ok) {
      setSaveError(result.error);
      if (result.error.code === "stale_revision" || result.error.code === "stale_view_revision") await reload();
      return false;
    }
    await reload();
    return true;
  }, [reload]);

  const viewWriter = useMemo(() => createCanvasViewWriteQueue({
    readCurrent: () => viewRef.current,
    acceptCurrent: acceptView,
    save: (next) => saveCanvasView(graphId, next),
    onError: async (writeError) => {
      setSaveError(writeError);
      if (writeError.code === "stale_view_revision") await refreshView();
    },
  }), [acceptView, graphId, refreshView]);

  const persistView = useCallback((update: (current: CanvasView) => CanvasView) => {
    setSaveError(null);
    void viewWriter.enqueue(update);
  }, [viewWriter]);

  const updateLayouts = useCallback((changed: CanvasNodeLayout[]) => {
    persistView((current) => {
      const changedById = new Map(changed.map((layout) => [layout.node_id, layout]));
      const node_layouts = current.node_layouts.map((layout) => changedById.get(layout.node_id) || layout);
      for (const layout of changed) if (!node_layouts.some((item) => item.node_id === layout.node_id)) node_layouts.push(layout);
      return { ...current, node_layouts };
    });
  }, [persistView]);

  const updateViewport = useCallback((viewport: CanvasViewport) => {
    persistView((current) => (
      current.viewport.x === viewport.x
      && current.viewport.y === viewport.y
      && current.viewport.zoom === viewport.zoom
        ? current
        : { ...current, viewport }
    ));
  }, [persistView]);

  const updateSelection = useCallback((selection: string[]) => {
    persistView((current) => selection.join("\0") === current.selection.join("\0")
      ? current
      : { ...current, selection });
  }, [persistView]);

  const createVisualGroup = useCallback(() => {
    if (!view) return;
    const alreadyGrouped = new Set(view.groups.flatMap((group) => group.node_ids));
    const node_ids = view.selection.filter((nodeId) => !alreadyGrouped.has(nodeId));
    if (node_ids.length < 2) return;
    persistView((current) => ({
      ...current,
      selection: [],
      groups: [...current.groups, {
        id: `group-${crypto.randomUUID()}`,
        title: `分组 ${current.groups.length + 1}`,
        node_ids,
        collapsed: false,
      }],
    }));
  }, [persistView, view]);

  const toggleVisualGroup = useCallback((groupId: string) => {
    const group = view?.groups.find((candidate) => candidate.id === groupId);
    if (group && !group.collapsed && focusedNodeId && group.node_ids.includes(focusedNodeId)) {
      setFocusedNodeId(null);
    }
    persistView((current) => ({
      ...current,
      groups: current.groups.map((candidate) => candidate.id === groupId
        ? { ...candidate, collapsed: !candidate.collapsed }
        : candidate),
    }));
  }, [focusedNodeId, persistView, view]);

  const removeVisualGroup = useCallback((groupId: string) => {
    persistView((current) => ({
      ...current,
      groups: current.groups.filter((group) => group.id !== groupId),
    }));
  }, [persistView]);

  const runGraphReview = useCallback(async () => {
    if (!projection || reviewBusy) return;
    setReviewBusy(true);
    setReviewError("");
    const result = await reviewTaskGraph(graphId, projection.revision);
    setReviewBusy(false);
    if (result.ok) setReview(result.data);
    else {
      setReview(null);
      setReviewError(result.error.message);
      if (result.error.code === "stale_revision") await reload();
    }
  }, [graphId, projection, reload, reviewBusy]);

  const acceptReviewSuggestion = useCallback(async (suggestion: TaskGraphReviewSuggestion) => {
    if (!projection || !review) return;
    if (projection.revision !== review.reviewed_revision) {
      setReviewError("任务图已更新，旧审查建议未应用；请重新审查。");
      setReview(null);
      return;
    }
    const node = projection.nodes.find((candidate) => candidate.id === suggestion.node_id);
    if (!node) {
      setReviewError("建议引用的节点已不存在，请重新审查。");
      setReview(null);
      return;
    }
    const input: Record<string, unknown> = {
      instruction: suggestion.instruction,
      acceptance_criteria: suggestion.acceptance_criteria,
    };
    if (node.executor_ref) input.executor_ref = node.executor_ref;
    const accepted = await mutate(() => updateTaskNode(graphId, node.id, review.reviewed_revision, {
      kind: node.kind,
      title: suggestion.title,
      input,
      retry_policy: node.retry_policy || { max_attempts: 1 },
    }));
    if (accepted) {
      setReview(null);
      setReviewError("图已按该建议更新；如需继续，请基于新版本重新执行 AI 审查。");
    }
  }, [graphId, mutate, projection, review]);

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
      {view && <div className="mx-4 mt-2 flex flex-wrap items-center gap-2 rounded-[var(--radius-md)] border border-[var(--border-soft)] bg-[var(--surface-elevated)] px-3 py-2" aria-label="画布视图工具">
        <Button size="sm" variant="secondary" disabled={saving} onClick={() => updateLayouts(buildAutoLayout(projection))}>自动布局</Button>
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
        <TaskExecutionTrail projection={projection} focusedNodeId={focusedNodeId} onFocus={setFocusedNodeId} />
        <Panel padding={false} className="min-h-0 overflow-hidden">
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
        </Panel>
        <TaskWorldInspector
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
