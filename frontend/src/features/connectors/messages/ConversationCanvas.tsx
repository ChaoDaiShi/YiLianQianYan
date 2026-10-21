import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Link } from "react-router-dom";
import { Background, Controls, Handle, Position, ReactFlow, ReactFlowProvider, useNodesInitialized, useReactFlow, type Edge, type Node, type NodeProps, type Viewport } from "@xyflow/react";
import { GitBranch, Maximize2, RefreshCw, Send, Square, X } from "lucide-react";
import { Button } from "../../../components/ui";
import { stopGeneration, type AgentEvent } from "../../../api/chat";
import { getMessageCanvas, saveMessageCanvasLayout, streamMessageCanvasBranch, type ConnectorMessage, type MessageCanvasOverview } from "../../../api/connectorMessages";
import { ancestorPath, type MessagePosition } from "./messageTree";
import { conversationFamilies, layoutMessageForest } from "./messageForest";
import "@xyflow/react/dist/style.css";
import "./messageBranches.css";

const statusLabels: Record<string, string> = { idle: "就绪", running: "正在执行", waiting_approval: "等待审批", completed: "已完成", failed: "执行失败", cancelled: "已停止", interrupted: "已中断" };
const running = (status: string) => status === "running" || status === "waiting_approval";
type SpeechNode = Node<{ message: ConnectorMessage; onPath: boolean; onBranch: () => void; busy: boolean }, "conversationSpeech">;

function ConversationSpeech({ data, selected }: NodeProps<SpeechNode>) {
  const message = data.message;
  return <article className="message-branch-node" data-role={message.role} data-selected={selected} data-path={data.onPath} data-status={message.status}>
    <Handle id="previous" type="target" position={Position.Top} /><Handle id="branch-target" type="target" position={Position.Left} />
    <header><strong>{message.role === "user" ? "我的提问" : "小昔涟的回答"}</strong><span>{message.role === "user" ? "提问" : statusLabels[message.status] ?? message.status}</span></header>
    <p>{message.content || "（无正文）"}</p>
    {message.reasoning && <span className="conversation-canvas-reasoning-label">含思考过程</span>}
    {message.role === "assistant" && <button type="button" className="message-branch-node-action nodrag nopan" disabled={data.busy} aria-label="从此回答创建分支对话" onClick={event => { event.stopPropagation(); data.onBranch(); }}><GitBranch size={11} />创建分支</button>}
    <Handle id="next" type="source" position={Position.Bottom} /><Handle id="branch-source" type="source" position={Position.Right} />
  </article>;
}

const nodeTypes = { conversationSpeech: ConversationSpeech };
const explain = (cause: unknown) => cause instanceof Error ? cause.message : String(cause);
interface Turn { conversation: string | null; controller: AbortController | null; cancelled: boolean; ended: boolean }

function Overview({ initialConversationId }: { initialConversationId?: string | null }) {
  const flow = useReactFlow<SpeechNode>();
  const initialized = useNodesInitialized();
  const [overview, setOverview] = useState<MessageCanvasOverview | null>(null);
  const [positions, setPositions] = useState<Record<string, MessagePosition>>({});
  const [measurements, setMeasurements] = useState<Record<string, { width: number; height: number }>>({});
  const [viewport, setViewport] = useState<Viewport>({ x: 0, y: 0, zoom: 0.8 });
  const [selected, setSelected] = useState<string | null>(null);
  const [branchParent, setBranchParent] = useState<ConnectorMessage | null>(null);
  const [draft, setDraft] = useState("");
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [streamText, setStreamText] = useState("");
  const [activity, setActivity] = useState<string[]>([]);
  const [activeConversation, setActiveConversation] = useState<string | null>(null);
  const [camera, setCamera] = useState<string | "fit" | null>(null);
  const positionsRef = useRef(positions);
  const mounted = useRef(true);
  const active = useRef<Turn | null>(null);
  const composer = useRef<HTMLTextAreaElement>(null);
  positionsRef.current = positions;

  const refresh = useCallback(async (reopen = false) => {
    const next = await getMessageCanvas();
    if (!mounted.current) return;
    setOverview(next);
    setPositions(current => {
      const automatic = layoutMessageForest(next.conversations);
      const saved = Object.fromEntries(next.layout.nodes.map(point => [point.id, { x: point.x, y: point.y }]));
      return reopen ? { ...automatic, ...saved } : { ...automatic, ...saved, ...current };
    });
    if (reopen) {
      setViewport(next.layout.viewport);
      setCamera(initialConversationId ? `conversation:${initialConversationId}` : next.layout.nodes.length ? null : "fit");
    }
  }, [initialConversationId]);

  useEffect(() => {
    mounted.current = true;
    setLoading(true);
    void refresh(true).catch(cause => { if (mounted.current) setError(explain(cause)); }).finally(() => { if (mounted.current) setLoading(false); });
    return () => { mounted.current = false; };
  }, [refresh]);

  const hasRunning = overview?.conversations.some(conversation => running(conversation.status)) ?? false;
  useEffect(() => {
    if (!hasRunning && !busy) return;
    const timer = window.setInterval(() => { void refresh().catch(cause => { if (mounted.current) setError(explain(cause)); }); }, 1500);
    return () => window.clearInterval(timer);
  }, [hasRunning, busy, refresh]);

  useEffect(() => {
    if (!initialized || !camera) return;
    if (camera === "fit") void flow.fitView({ padding: 0.15, minZoom: 0.1, maxZoom: 0.85 });
    else {
      const conversation = camera.startsWith("conversation:") ? overview?.conversations.find(item => item.id === camera.slice("conversation:".length)) : null;
      const id = conversation ? conversation.nodes[0]?.id ?? conversation.branch?.parent_node_id : camera;
      const node = id ? flow.getNode(id) : undefined;
      if (!node) return;
      void flow.setCenter(node.position.x + 144, node.position.y + 64, { zoom: 0.85, duration: 220 });
    }
    setCamera(null);
  }, [camera, initialized, flow, overview]);

  async function persist(next: Record<string, MessagePosition>, cameraPosition = viewport) {
    const messageIds = new Set(overview?.conversations.flatMap(conversation => conversation.nodes.map(message => message.id)));
    try { await saveMessageCanvasLayout({ nodes: Object.entries(next).filter(([id]) => messageIds.has(id)).map(([id, point]) => ({ id, ...point })), viewport: cameraPosition }); }
    catch (cause) { if (mounted.current) setError(explain(cause)); }
  }

  function chooseBranch(message: ConnectorMessage) {
    setBranchParent(message); setSelected(message.id); composer.current?.focus();
  }

  async function finish(turn: Turn) {
    if (turn.ended) return;
    turn.ended = true;
    try { await refresh(); }
    catch (cause) { if (mounted.current) setError(explain(cause)); }
    finally { if (active.current === turn) { active.current = null; if (mounted.current) { setBusy(false); setStreamText(""); } } }
  }

  async function stop() {
    const turn = active.current;
    if (!turn) return;
    turn.cancelled = true;
    if (!turn.conversation) return;
    const result = await stopGeneration(turn.conversation);
    if (!result) { if (mounted.current) setError("停止请求未成功，请重试"); return; }
    turn.controller?.abort(); await finish(turn);
  }

  function send() {
    if (!branchParent || busy || !draft.trim()) return;
    const text = draft.trim();
    const turn: Turn = { conversation: null, controller: null, cancelled: false, ended: false };
    active.current = turn; setBusy(true); setError(""); setStreamText(""); setActivity([]); setDraft(""); setActiveConversation(null);
    const onEvent = (event: AgentEvent) => {
      if (event.type === "connected") {
        turn.conversation = event.conversation_id;
        if (turn.cancelled) void stopGeneration(event.conversation_id).then(result => { if (result) { turn.controller?.abort(); void finish(turn); } else if (mounted.current) setError("停止请求未成功，请重试"); });
        if (mounted.current) { setActiveConversation(event.conversation_id); void refresh().catch(cause => setError(explain(cause))); setCamera(`conversation:${event.conversation_id}`); }
      }
      if (!mounted.current || active.current !== turn || turn.ended) return;
      if (event.type === "token") setStreamText(current => current + (event.token ?? ""));
      if (event.type === "model_route") {
        if (event.replace_content) setStreamText("");
        setActivity(current => [...current, event.phase === "exhausted" ? "模型组候选已用尽" : "模型路由已更新"]);
      }
      if (event.type === "tool_start" || event.type === "tool_end") setActivity(current => [...current, `${event.tool_name ?? "工具"} · ${event.type === "tool_start" ? "执行中" : event.status ?? "结束"}${event.error ? ` · ${event.error}` : ""}`]);
      if (event.type === "approval_required") setActivity(current => [...current, `等待审批：${event.tool_name ?? "工具"}${event.reason ? ` · ${event.reason}` : ""}`]);
      if (event.type === "error") setError(event.error ?? "模型执行失败");
      if (event.type === "stream_end") void finish(turn);
    };
    turn.controller = streamMessageCanvasBranch(branchParent.conversation_id, branchParent.id, text, onEvent);
  }

  const messages = useMemo(() => overview?.conversations.flatMap(conversation => conversation.nodes) ?? [], [overview]);
  const families = useMemo(() => conversationFamilies(overview?.conversations ?? []), [overview]);
  const path = useMemo(() => new Set(ancestorPath(messages, selected)), [messages, selected]);
  const nodes = useMemo<SpeechNode[]>(() => messages.map(message => ({ id: message.id, type: "conversationSpeech", measured: measurements[message.id], position: positions[message.id], draggable: true, selected: selected === message.id, data: { message, onPath: path.has(message.id), busy, onBranch: () => chooseBranch(message) } })), [messages, positions, measurements, selected, path, busy]);
  const byId = new Map(messages.map(message => [message.id, message]));
  const groupStatuses = new Map(overview?.conversations.map(conversation => [conversation.id, conversation.status]));
  const edges: Edge[] = messages.filter(message => message.parent_id && byId.has(message.parent_id)).map(message => {
    const source = byId.get(message.parent_id!)!;
    const branch = source.conversation_id !== message.conversation_id;
    return { id: `message:${message.id}`, source: source.id, target: message.id, sourceHandle: branch ? "branch-source" : "next", targetHandle: branch ? "branch-target" : "previous", type: "default", label: branch ? "另一条路径" : undefined, animated: branch && running(groupStatuses.get(message.conversation_id) ?? ""), className: branch ? "conversation-canvas-branch-edge" : undefined, style: { stroke: branch || path.has(message.id) ? "var(--accent-primary)" : "var(--border-strong)", strokeWidth: branch || path.has(message.id) ? 2.4 : 1.5 } };
  });
  const focused = messages.find(message => message.id === selected);

  return <section className="message-branch-workspace conversation-canvas" aria-label="全部对话消息画布">
    <header className="message-branch-toolbar"><label>定位对话<select aria-label="定位画布对话" value="" onChange={event => setCamera(`conversation:${event.target.value}`)}><option value="">全部对话（{families.length}）</option>{families.map(family => <option key={family.id} value={family.id}>{family.title}{family.conversationIds.length > 1 ? ` · ${family.conversationIds.length} 条路径` : ""}</option>)}</select></label><Button variant="secondary" size="sm" onClick={() => setCamera("fit")}><Maximize2 size={14} />适应画布</Button><Button variant="secondary" size="sm" disabled={loading} onClick={() => { setError(""); void refresh().catch(cause => setError(explain(cause))); }}><RefreshCw size={14} />刷新</Button><span>点击消息查看全文 · 从回答展开另一条路径 · 独立对话互不连线</span></header>
    {error && <p className="message-branch-error" role="alert">{error}</p>}
    <div className="message-branch-stage"><ReactFlow nodes={nodes} edges={edges} nodeTypes={nodeTypes} viewport={viewport} onViewportChange={setViewport} minZoom={0.1} maxZoom={2} nodesConnectable={false} deleteKeyCode={null}
      onNodesChange={changes => { const dimensions = changes.filter(change => change.type === "dimensions"); if (dimensions.length) setMeasurements(current => { const next = { ...current }; for (const change of dimensions) if (change.dimensions) next[change.id] = change.dimensions; return next; }); }}
      onNodeClick={(_, node) => setSelected(node.id)} onNodeDrag={(_, node) => setPositions(current => ({ ...current, [node.id]: node.position }))} onNodeDragStop={(_, node) => { const next = { ...positionsRef.current, [node.id]: node.position }; setPositions(next); void persist(next); }} onMoveEnd={(_, next) => { void persist(positionsRef.current, next); }}><Background color="var(--border-soft)" gap={22} /><Controls showInteractive={false} /></ReactFlow>
      {loading && <div className="message-branch-empty" role="status">正在读取已有对话记录…</div>}
      {!loading && overview?.conversations.length === 0 && <div className="message-branch-empty"><GitBranch size={28} /><p>还没有对话，在主页发送问题后会自动出现在这里</p></div>}
    </div>
    {focused && <aside className="message-branch-detail conversation-canvas-detail" aria-label="选中消息详情"><header><strong>{focused.role === "user" ? "提问全文" : "回答全文"}</strong><button type="button" aria-label="关闭消息详情" onClick={() => setSelected(null)}><X size={14} /></button></header><p>{focused.content}</p>{focused.reasoning && <details className="conversation-canvas-reasoning"><summary>思考过程</summary><p>{focused.reasoning}</p></details>}<Link to={`/chat/${focused.conversation_id}`}>打开此路径的完整记录</Link>{focused.error && <p role="alert">{focused.error}</p>}{focused.role === "assistant" && <Button size="sm" variant="secondary" disabled={busy} onClick={() => chooseBranch(focused)}><GitBranch size={14} />从此回答创建分支</Button>}</aside>}
    {(busy || activity.length > 0) && <div className="message-branch-activity" role="status">{streamText && <p className="conversation-canvas-stream">{streamText}</p>}{activity.slice(-3).map((line, index) => <p key={index}>{line}</p>)}{activeConversation && <Link to={`/chat/${activeConversation}`}>打开当前执行对话与审批</Link>}</div>}
    {branchParent && <form className="message-branch-composer" onSubmit={event => { event.preventDefault(); send(); }}><div className="message-branch-composer-context"><GitBranch size={14} /><span>另一条路径 · {branchParent.content.slice(0, 60)}</span><button type="button" disabled={busy} onClick={() => setBranchParent(null)}>取消</button></div><textarea ref={composer} aria-label="分支对话提问" placeholder="从此回答继续提问，展开另一条消息路径…" value={draft} disabled={busy} onChange={event => setDraft(event.target.value)} rows={2} /><div className="message-branch-send">{busy ? <Button type="button" variant="danger" size="sm" onClick={() => void stop()}><Square size={14} />停止生成</Button> : <Button type="submit" size="sm" disabled={!draft.trim()}><Send size={14} />发送到新分支</Button>}</div></form>}
  </section>;
}

export default function ConversationCanvas(props: { initialConversationId?: string | null }) {
  return <ReactFlowProvider><Overview {...props} /></ReactFlowProvider>;
}
