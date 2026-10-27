import { useEffect, useMemo, useRef, useState } from "react";
import { Link } from "react-router-dom";
import { Background, Controls, Handle, Position, ReactFlow, ReactFlowProvider, useNodesInitialized, useReactFlow, type Node, type NodeProps, type Viewport } from "@xyflow/react";
import { GitBranch, Plus } from "lucide-react";
import { Button } from "../../../components/ui";
import { createConversation } from "../../../api/conversations";
import { bindConnectorResource } from "../../../api/connectors";
import { stopGeneration, type AgentEvent } from "../../../api/chat";
import { createMessageGraph, getMessageGraph, getMessageSources, saveMessageLayout, selectMessageNode, streamMessageTurn, type ConnectorMessage, type ConnectorMessageGraph, type MessageSources } from "../../../api/connectorMessages";
import { ancestorPath, layoutMessageTree, nextAssistantParent, type MessagePosition } from "./messageTree";
import "@xyflow/react/dist/style.css";
import "./messageBranches.css";
import BranchComposer from "./BranchComposer";
import { useSelectedMessageFocus } from "./useSelectedMessageFocus";
import { ResourceCards } from "../../resources/ResourceCard";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";

type SpeechNode = Node<{ message: ConnectorMessage; onPath: boolean; onBranch: () => void; busy: boolean }, "speech">;
function Speech({ data, selected }: NodeProps<SpeechNode>) {
  const message = data.message;
  return <article className="message-branch-node" data-role={message.role} data-selected={selected} data-path={data.onPath}>
    <Handle type="target" position={Position.Top} />
    <header><strong>{message.role === "user" ? "我的提问" : "小涟的回答"}</strong><span>{message.status === "streaming" ? "正在回复" : message.role === "assistant" ? "回答" : "提问"}</span></header>
    <p>{message.content || (message.status === "streaming" ? "等待模型回复…" : "（无正文）")}</p>
    {message.role === "assistant" && !message.id.startsWith("pending-") && <button type="button" className="message-branch-node-action nodrag nopan" aria-label="从此回答继续提问" disabled={data.busy} onClick={event => { event.stopPropagation(); data.onBranch(); }}><GitBranch size={11} />继续提问</button>}
    {message.error && <small role="alert">{message.error}</small>}
    <Handle type="source" position={Position.Bottom} />
  </article>;
}
const nodeTypes = { speech: Speech };
interface Props { initialConversationId?: string | null; canvasId: string; agentId: string; onResourcesChanged?: () => void }
interface ActiveTurn {
  root: string; canvas: string; conversation: string | null; controller: AbortController | null;
  userId: string; answerId: string; parentId: string | null; cancelled: boolean; ended: boolean;
}
const explain = (cause: unknown) => cause instanceof Error ? cause.message : String(cause);
const preferenceKey = (canvas: string) => `yilian.connector.message-root.${canvas}`;

function Workspace({ canvasId, agentId, initialConversationId, onResourcesChanged }: Props) {
  const flow = useReactFlow<SpeechNode>();
  const nodesInitialized = useNodesInitialized();
  const [sources, setSources] = useState<MessageSources>({ conversations: [], graphs: [] });
  const [root, setRoot] = useState("");
  const [graph, setGraph] = useState<ConnectorMessageGraph | null>(null);
  const [selected, setSelected] = useState<string | null>(null);
  const [draft, setDraft] = useState("");
  const [branchParent, setBranchParent] = useState<string | null>(null);
  const [pending, setPending] = useState<ConnectorMessage[]>([]);
  const [busy, setBusy] = useState(false);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [activity, setActivity] = useState<string[]>([]);
  const [approvalConversation, setApprovalConversation] = useState<string | null>(null);
  const [viewport, setViewport] = useState<Viewport>({ x: 0, y: 0, zoom: 0.8 });
  const [positions, setPositions] = useState<Record<string, MessagePosition>>({});
  const [measurements, setMeasurements] = useState<Record<string, { width: number; height: number }>>({});
  const [camera, setCamera] = useState<{ kind: "fit" | "center"; id?: string } | null>(null);
  const composerRef = useRef<HTMLTextAreaElement>(null);
  useSelectedMessageFocus(selected,nodesInitialized);
  const positionsRef = useRef(positions);
  const active = useRef<ActiveTurn | null>(null);
  const mounted = useRef(true);
  const requestVersion = useRef(0);
  positionsRef.current = positions;

  function acceptGraph(next: ConnectorMessageGraph, options: { reopen?: boolean; turn?: ActiveTurn } = {}) {
    setGraph(next); setSelected(next.selected_node_id); setPending([]);
    const parentId = nextAssistantParent(next.nodes, { selectedId: next.selected_node_id, turnConversationId: options.turn?.conversation, previousParentId: options.turn?.parentId });
    setBranchParent(parentId);
    const auto = layoutMessageTree(next.nodes);
    for (const point of next.layout.nodes) auto[point.id] = { x: point.x, y: point.y };
    setPositions(auto);
    if (options.reopen) {
      setViewport(next.layout.viewport);
      if (next.nodes.length && next.layout.nodes.length === 0) setCamera({ kind: "fit" });
    } else if (options.turn && parentId) setCamera({ kind: "center", id: parentId });
  }

  useEffect(() => {
    if (!camera || !nodesInitialized) return;
    if (camera.kind === "fit") void flow.fitView({ padding: 0.25, minZoom: 0.45, maxZoom: 0.95 });
    else {
      const node = flow.getNode(camera.id!);
      if (!node) return;
      void flow.setCenter(node.position.x + 130, node.position.y + 59, { zoom: Math.max(0.7, viewport.zoom), duration: 250 });
    }
    setCamera(null);
  }, [camera, nodesInitialized, flow, viewport.zoom]);

  function chooseBranch(id: string) {
    setBranchParent(id); setSelected(id); composerRef.current?.focus();
    void selectMessageNode(canvasId, root, id).catch(cause => setError(explain(cause)));
  }

  async function openRoot(id: string, catalog = sources) {
    const version = ++requestVersion.current;
    setLoading(true); setError(""); setBranchParent(null); setApprovalConversation(null); setActivity([]);
    try {
      const next = catalog.graphs.some(item => item.conversation_id === id)
        ? await getMessageGraph(canvasId, id) : await createMessageGraph(canvasId, id);
      if (!mounted.current || version !== requestVersion.current) return;
      setRoot(id); acceptGraph(next, { reopen: true }); localStorage.setItem(preferenceKey(canvasId), id);
      setSources(current => ({ ...current, graphs: current.graphs.some(item => item.conversation_id === id) ? current.graphs : [...current.graphs, { conversation_id: id }] }));
    } catch (cause) { if (mounted.current && version === requestVersion.current) setError(explain(cause)); }
    finally { if (mounted.current && version === requestVersion.current) setLoading(false); }
  }

  useEffect(() => {
    mounted.current = true;
    void (async () => {
      try {
        const catalog = await getMessageSources(canvasId);
        if (!mounted.current) return;
        setSources(catalog);
        const preferred = localStorage.getItem(preferenceKey(canvasId));
        const id = initialConversationId || catalog.conversations.find(item => item.id === preferred)?.id || catalog.conversations[0]?.id;
        if (id) await openRoot(id, catalog);
        else setLoading(false);
      } catch (cause) { if (mounted.current) { setError(explain(cause)); setLoading(false); } }
    })();
    return () => {
      mounted.current = false; ++requestVersion.current;

    };
  }, [canvasId, initialConversationId]);

  useEffect(() => {
    if (!root || busy || !graph?.runs.some(run => run.status === "running" || run.status === "waiting_approval")) return;
    let current = true;
    const timer = window.setInterval(() => {
      void getMessageGraph(canvasId, root).then(next => { if (current) acceptGraph(next); }).catch(cause => { if (current) setError(explain(cause)); });
    }, 1500);
    return () => { current = false; window.clearInterval(timer); };
  }, [canvasId, root, graph?.runs, busy]);

  async function newRoot() {
    setLoading(true); setError("");
    try {
      const conversation = await createConversation("画布对话");
      await bindConnectorResource(canvasId, { kind: "conversation_agent", source_id: `conversation:${conversation.id}`, target_id: `agent:${agentId}` });
      const next = await createMessageGraph(canvasId, conversation.id);
      if (!mounted.current) return;
      const catalog = await getMessageSources(canvasId);
      setSources(catalog); setRoot(conversation.id); acceptGraph(next, { reopen: true }); setDraft(""); setActivity([]); setApprovalConversation(null);
      localStorage.setItem(preferenceKey(canvasId), conversation.id); onResourcesChanged?.();
    } catch (cause) { if (mounted.current) setError(explain(cause)); }
    finally { if (mounted.current) setLoading(false); }
  }

  async function persistLayout(nextPositions: Record<string, MessagePosition>, nextViewport: Viewport) {
    if (!graph) return;
    try {
      await saveMessageLayout(canvasId, root, { nodes: graph.nodes.map(node => ({ id: node.id, ...nextPositions[node.id] })), viewport: nextViewport });
    } catch (cause) { if (mounted.current) setError(explain(cause)); }
  }

  async function finish(turn: ActiveTurn) {
    if (turn.ended) return;
    turn.ended = true;
    try {
      const next = await getMessageGraph(turn.canvas, turn.root);
      if (mounted.current && active.current === turn) acceptGraph(next, { turn });
    } catch (cause) { if (mounted.current && active.current === turn) setError(explain(cause)); }
    finally {
      if (active.current === turn) {
        active.current = null;
        if (mounted.current) setBusy(false);
      }
    }
  }

  async function stop() {
    const turn = active.current;
    if (!turn) return;
    turn.cancelled = true;
    if (!turn.conversation) { setActivity(current => [...current, "正在等待执行会话，以停止生成…"]); return; }
    const result = await stopGeneration(turn.conversation);
    if (!result) { setError("停止请求未成功，请重试"); return; }
    turn.controller?.abort(); await finish(turn);
  }

  function send() {
    if (!graph || busy || !draft.trim()) return;
    const text = draft.trim();
    const turn: ActiveTurn = { root, canvas: canvasId, conversation: null, controller: null, parentId: branchParent, userId: `pending-user-${crypto.randomUUID()}`, answerId: `pending-answer-${crypto.randomUUID()}`, cancelled: false, ended: false };
    active.current = turn; setBusy(true); setError(""); setActivity([]); setApprovalConversation(null); setDraft("");
    const base = { conversation_id: root, created_at: Date.now(), status: "streaming", error: null };
    setPending([
      { ...base, id: turn.userId, parent_id: branchParent, role: "user", content: text },
      { ...base, id: turn.answerId, parent_id: turn.userId, role: "assistant", content: "" },
    ]);
    setCamera({ kind: "center", id: turn.answerId });
    const event = (item: AgentEvent) => {
      if (item.type === "connected") {
        turn.conversation = item.conversation_id;
        if (turn.cancelled) {
          void stopGeneration(item.conversation_id).then(() => { turn.controller?.abort(); if (mounted.current) void finish(turn); });
        }
      }
      if (!mounted.current || active.current !== turn || turn.ended) return;
      if (item.type === "token") setPending(current => current.map(node => node.id === turn.answerId ? { ...node, content: node.content + (item.token ?? ""), conversation_id: turn.conversation ?? root } : node));
      if (item.type === "model_route") {
        if (item.replace_content) setPending(current => current.map(node => node.id === turn.answerId ? { ...node, content: "" } : node));
        const phase = { selected: "已选择模型", switched: "已切换模型", recovered: "已恢复模型", exhausted: "模型组候选已用尽" };
        setActivity(current => [...current, `${item.phase ? phase[item.phase] : "模型路由"}${item.to_tier != null ? ` · 模型组第 ${item.to_tier} 级` : ""}`]);
      }
      if (item.type === "tool_start" || item.type === "tool_end") setActivity(current => [...current, `${item.tool_name ?? "工具"} · ${item.type === "tool_start" ? "执行中" : item.status ?? "结束"}${item.error ? ` · ${item.error}` : ""}`]);
      if (item.type === "approval_required") {
        setApprovalConversation(item.conversation_id || turn.conversation);
        setActivity(current => [...current, `等待审批：${item.tool_name ?? "工具"}${item.reason ? ` · ${item.reason}` : ""}`]);
      }
      if (item.type === "error") setError(item.error ?? "模型执行失败");
      if (item.type === "stream_end") void finish(turn);
    };
    turn.controller = streamMessageTurn(canvasId, root, branchParent, text, event);
  }

  const messages = useMemo(() => [...(graph?.nodes ?? []), ...pending], [graph, pending]);
  const path = useMemo(() => new Set(ancestorPath(messages, selected)), [messages, selected]);
  const automatic = useMemo(() => layoutMessageTree(messages), [messages]);
  const nodes: SpeechNode[] = messages.map(message => ({ id: message.id, type: "speech", position: positions[message.id] ?? automatic[message.id], measured: measurements[message.id], selected: selected === message.id, data: { message, onPath: path.has(message.id), busy, onBranch: () => chooseBranch(message.id) } }));
  const edges = messages.filter(message => message.parent_id).map(message => ({ id: `${message.parent_id}-${message.id}`, source: message.parent_id!, target: message.id, type: "default", style: { stroke: path.has(message.id) ? "var(--accent-primary)" : "var(--border-strong)", strokeWidth: path.has(message.id) ? 2.5 : 1.5 } }));
  const focused = messages.find(message => message.id === selected);
  const parent = messages.find(message => message.id === branchParent);

  return <section className="message-branch-workspace" aria-label="消息分支画布">
    <header className="message-branch-toolbar">
      <label>对话目录<select aria-label="消息画布对话目录" value={root} disabled={busy || loading} onChange={event => void openRoot(event.target.value)}><option value="" disabled>选择对话</option>{sources.conversations.map(item => <option key={item.id} value={item.id}>{item.title}</option>)}</select></label>
      <Button variant="secondary" size="sm" disabled={busy || loading} onClick={() => void newRoot()}><Plus size={14} />新建对话</Button>
      {root && <Link className="text-sm text-[var(--accent-primary)]" to={`/chat/${root}`}>在主页打开对话</Link>}
      <span>拖动画布平移 · 滚轮缩放 · 点击消息查看全文</span>
    </header>
    {error && <p className="message-branch-error" role="alert">{error}</p>}
    <div className="conversation-canvas-shell"><div className="message-branch-stage">
      <ReactFlow nodes={nodes} edges={edges} nodeTypes={nodeTypes} viewport={viewport} onViewportChange={setViewport}
        minZoom={0.25} maxZoom={1.8} nodesDraggable={!busy} nodesConnectable={false} deleteKeyCode={null}
        onNodesChange={changes => {
          const dimensions = changes.filter(change => change.type === "dimensions");
          if (dimensions.length) setMeasurements(current => {
            const next = { ...current };
            for (const change of dimensions) if (change.dimensions) next[change.id] = change.dimensions;
            return next;
          });
        }}
        onNodeClick={(_, node) => {
          setSelected(node.id);
          if (!node.id.startsWith("pending-")) void selectMessageNode(canvasId, root, node.id).catch(cause => setError(explain(cause)));
        }}
        onNodeDrag={(_, node) => setPositions(current => ({ ...current, [node.id]: node.position }))}
        onNodeDragStop={(_, node) => { const next = { ...positionsRef.current, [node.id]: node.position }; setPositions(next); void persistLayout(next, viewport); }}
        onMoveEnd={(_, next) => { if (!busy) void persistLayout(positionsRef.current, next); }}>
        <Background color="var(--border-soft)" gap={22} /><Controls showInteractive={false} />
      </ReactFlow>
      {loading && <div className="message-branch-empty" role="status">正在加载消息画布…</div>}
      {!loading && messages.length === 0 && <div className="message-branch-empty"><GitBranch size={28} /><p>{root ? "输入第一条提问，开始绘制消息链" : "新建对话后，每条提问与回答都会成为独立节点"}</p></div>}
    </div>
    {focused&&<aside className="message-branch-detail conversation-canvas-detail" aria-label="选中消息详情"><header><strong>{focused.role==="user"?"提问全文":"回答全文"}</strong><button type="button" aria-label="关闭消息详情" onClick={()=>{setSelected(null);void selectMessageNode(canvasId,root,null).catch(cause=>setError(explain(cause)));}}>关闭详情</button></header>{focused.attachments?.length?<ResourceCards files={focused.attachments}/>:null}<div className="canvas-detail-content conversation-markdown prose prose-sm max-w-none"><ReactMarkdown remarkPlugins={[remarkGfm]}>{focused.content||"等待正文"}</ReactMarkdown></div><Link to={`/chat/${focused.conversation_id}`}>打开这条路径的完整记录</Link>{focused.error&&<p role="alert">{focused.error}</p>}{focused.role==="assistant"&&!focused.id.startsWith("pending-")&&<Button size="sm" variant="secondary" disabled={busy} onClick={()=>chooseBranch(focused.id)}><GitBranch size={14}/>从此回答继续提问</Button>}</aside>}
    </div>
    {(activity.length > 0 || approvalConversation || graph?.runs.some(run => run.error || run.status === "waiting_approval")) && <div className="message-branch-activity" role="status">{activity.slice(-5).map((line, index) => <p key={index}>{line}</p>)}{graph?.runs.filter(run => run.error || run.status === "waiting_approval").map(run => <p key={run.conversation_id}>{run.error || "等待审批"} · <Link to={`/chat/${run.conversation_id}`}>打开执行对话</Link></p>)}{approvalConversation && <Link to={`/chat/${approvalConversation}`}>打开执行对话处理审批</Link>}</div>}
    <BranchComposer parent={parent} draft={draft} onChange={setDraft} onSend={send} onStop={()=>void stop()} busy={busy} disabled={!root||loading} inputRef={composerRef} conversationTitle={sources.conversations.find(item=>item.id===root)?.title}/>
  </section>;
}

export default function MessageBranchWorkspace(props: Props) {
  return <ReactFlowProvider><Workspace {...props} /></ReactFlowProvider>;
}
