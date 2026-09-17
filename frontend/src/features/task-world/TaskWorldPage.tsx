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
} from "../../api/taskWorld";
import { subscribeToEvents } from "../../api/events";
import { Button, EmptyState, ErrorState, PageHeader, Panel, Skeleton } from "../../components/ui";
import TaskExecutionTrail from "./TaskExecutionTrail";
import TaskWorldCanvas from "./TaskWorldCanvas";
import TaskWorldInspector from "./TaskWorldInspector";
import { isActiveExecution, isTaskWorldEvent, projectTaskGraph } from "./taskGraphProjection";
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
