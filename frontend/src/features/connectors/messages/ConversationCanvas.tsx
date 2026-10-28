import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Link } from "react-router-dom";
import { Background, Controls, MarkerType, ReactFlow, ReactFlowProvider, useNodesInitialized, useNodesState, useReactFlow, useStore, type Edge, type Node, type NodeProps, type Viewport } from "@xyflow/react";
import { GitBranch, Maximize2, Paperclip, RefreshCw } from "lucide-react";
import BranchComposer from "./BranchComposer";
import MessageNodePorts from "./MessageNodePorts";
import MessageSelectionPanel from "./MessageSelectionPanel";
import { messageEdgeHandles } from "./messageGeometry";
import type { ChatRequestOptions } from "../../../components/chat/chatRequestOptions";
import { useSelectedMessageFocus } from "./useSelectedMessageFocus";
import { originLabel } from "../../../components/chat/conversationHistoryModel";
import { Button } from "../../../components/ui";
import { stopGeneration, type AgentEvent } from "../../../api/chat";
import { getMessageCanvas, saveMessageCanvasLayout, streamMessageCanvasBranch, type ConnectorMessage, type MessageCanvasOverview } from "../../../api/connectorMessages";
import { ancestorPath, layoutMessageTree, placeNewMessages, type MessagePosition } from "./messageTree";
import { conversationFamilies, layoutMessageForest } from "./messageForest";
import { reconcileMessageNodes } from "./reconcileMessageNodes";
import "@xyflow/react/dist/style.css";
import "./messageBranches.css";

const statusLabels: Record<string, string> = { idle: "就绪", running: "正在执行", waiting_approval: "等待审批", completed: "已完成", failed: "执行失败", cancelled: "已停止", interrupted: "已中断" };
const running = (status: string) => status === "running" || status === "waiting_approval";
type SpeechNode = Node<{ message: ConnectorMessage; title:string; origin:string; onPath: boolean; onBranch: () => void; busy: boolean }, "conversationSpeech">;

function ConversationSpeech({ data, selected }: NodeProps<SpeechNode>) {
  const message = data.message;
  const reasoning = message.role === "assistant" ? message.reasoning : undefined;
  return <article className="message-branch-node" data-role={message.role} data-selected={selected} data-path={data.onPath} data-status={message.status}>
    <MessageNodePorts />
    <div className="canvas-node-context"><span>{data.origin}</span><span title={data.title}>{data.title}</span></div>
    <header><strong>{message.role === "user" ? "我的提问" : "小涟的回答"}</strong><span>{message.role === "user" ? "提问" : statusLabels[message.status] ?? message.status}</span></header>
    {(message.content || reasoning) && <div className="canvas-node-body nodrag nowheel"><p>{message.content || reasoning}</p></div>}
    <footer className="message-node-footer">
    {!!message.attachments?.length&&<span className="canvas-node-attachments"><Paperclip size={12}/>{message.attachments.map(item => item.name).join("、")}</span>}
    {reasoning && <span className="conversation-canvas-reasoning-label">含思考过程</span>}
    {message.role === "assistant" && !message.id.startsWith("pending:") && <button type="button" className="message-branch-node-action nodrag nopan" disabled={data.busy} aria-label="从此回答创建分支对话" onClick={event => { event.stopPropagation(); data.onBranch(); }}><GitBranch size={11} />创建分支</button>}
    </footer>
  </article>;
}

const nodeTypes = { conversationSpeech: ConversationSpeech };
const explain = (cause: unknown) => cause instanceof Error ? cause.message : String(cause);
interface Turn { conversation: string | null; controller: AbortController | null; cancelled: boolean; ended: boolean }
interface LiveNode { id: string; conversation: string; parentId: string; content: string; reasoning: string; createdAt: number }

function Overview({ initialConversationId }: { initialConversationId?: string | null }) {
  const flow = useReactFlow<SpeechNode>();
  const canvasWidth=useStore(state=>state.width);
  const canvasHeight=useStore(state=>state.height);
  const initialized = useNodesInitialized();
  const [overview, setOverview] = useState<MessageCanvasOverview | null>(null);
  const [sourceFilter,setSourceFilter]=useState("desktop");
  const [familyFilter,setFamilyFilter]=useState("");
  const [positions, setPositions] = useState<Record<string, MessagePosition>>({});
  const [nodes, setNodes, onNodesChange] = useNodesState<SpeechNode>([]);
  const [viewport, setViewport] = useState<Viewport>({ x: 0, y: 0, zoom: 0.8 });
  const [selected, setSelected] = useState<string | null>(null);
  const [branchParent, setBranchParent] = useState<ConnectorMessage | null>(null);
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [activity, setActivity] = useState<string[]>([]);
  const [activeConversation, setActiveConversation] = useState<string | null>(null);
  const [liveNode, setLiveNode] = useState<LiveNode | null>(null);
  const refreshVersion = useRef(0);
  const [camera, setCamera] = useState<string | "fit" | null>(null);
  const positionsRef = useRef(positions);
  const mounted = useRef(true);
  const active = useRef<Turn | null>(null);
  useSelectedMessageFocus(selected,initialized);
  positionsRef.current = positions;

  const refresh = useCallback(async (reopen = false) => {
    const version = ++refreshVersion.current;
    const next = await getMessageCanvas();
    if (!mounted.current || version !== refreshVersion.current) return;
    setOverview(next);
    setPositions(current => {
      const automatic = layoutMessageForest(next.conversations);
      const saved = Object.fromEntries(next.layout.nodes.map(point => [point.id, { x: point.x, y: point.y }]));
      return placeNewMessages(automatic, reopen ? saved : { ...saved, ...current });
    });
    if (reopen) {
      setViewport(next.layout.viewport);
      const latest = initialConversationId ? next.conversations.find(item => item.id === initialConversationId) : [...next.conversations].filter(item => item.nodes.length).sort((a, b) => b.updated_at - a.updated_at)[0];
      const latestNode = latest?.nodes[latest.nodes.length - 1];
      if (latestNode) { setSourceFilter(latest.origin?.kind || "desktop"); setSelected(latestNode.id); setBranchParent(latestNode.role === "assistant" ? latestNode : null); setCamera(latestNode.id); }
      else setCamera("fit");
    }
    return next;
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
    const timer = window.setInterval(() => { if (active.current?.ended) return; void refresh().catch(cause => { if (mounted.current) setError(explain(cause)); }); }, 1500);
    return () => window.clearInterval(timer);
  }, [hasRunning, busy, refresh]);

  useEffect(() => {
    if (!initialized || !camera || !canvasWidth || !canvasHeight) return;
    const frame=requestAnimationFrame(() => {
      const all=overview?.conversations.flatMap(item=>item.nodes)??[];
      const conversation=camera.startsWith("conversation:")?overview?.conversations.find(item=>item.id===camera.slice("conversation:".length)):null;
      const target=conversation?.nodes[conversation.nodes.length-1]?.id??camera;
      const family=conversation?conversationFamilies(overview?.conversations??[]).find(item=>item.conversationIds.includes(conversation.id)):null;
      const ids=camera==="fit"?null:new Set(family?(overview?.conversations.filter(item=>family.conversationIds.includes(item.id)).flatMap(item=>item.nodes.map(node=>node.id))??[]):ancestorPath(all,target).slice(-3));
      if(ids&&liveNode?.id===target){ids.add(target);ancestorPath(all,liveNode.parentId).slice(-2).forEach(id=>ids.add(id));}
      const visible=flow.getNodes().filter(node=>!ids||ids.has(node.id));
      if(!visible.length)return;
      void flow.fitView({nodes:visible,padding:.18,minZoom:camera==="fit"||Boolean(conversation)?0.1:0.45,maxZoom:.95,duration:180}).then(fitted=>{if(fitted)setCamera(current=>current===camera?null:current);});
    });
    return()=>cancelAnimationFrame(frame);
  }, [camera, initialized, flow, overview, canvasWidth, canvasHeight, liveNode?.id]);

  async function persist(next: Record<string, MessagePosition>, cameraPosition = viewport) {
    const messageIds = new Set(overview?.conversations.flatMap(conversation => conversation.nodes.map(message => message.id)));
    try { await saveMessageCanvasLayout({ nodes: Object.entries(next).filter(([id]) => messageIds.has(id)).map(([id, point]) => ({ id, ...point })), viewport: cameraPosition }); }
    catch (cause) { if (mounted.current) setError(explain(cause)); }
  }

  const chooseBranch = useCallback((message: ConnectorMessage) => {
    setBranchParent(message); setSelected(message.id);
  }, []);

  async function finish(turn: Turn) {
    if (turn.ended) return;
    turn.ended = true;
    try {
      const next = await refresh();
      const conversation = next?.conversations.find(item => item.id === turn.conversation);
      const latest = conversation?.nodes[conversation.nodes.length - 1];
      if (mounted.current && latest) { setSelected(latest.id); setBranchParent(latest.role === "assistant" ? latest : null); setCamera(latest.id); }
    }
    catch (cause) { if (mounted.current) setError(explain(cause)); }
    finally { if (active.current === turn) { active.current = null; if (mounted.current) { setBusy(false); setLiveNode(null); } } }
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

  function send(text: string, resources: string[] = [], options?: ChatRequestOptions) {
    if (!branchParent || busy || (!text.trim() && !resources.length)) return false;
    const turn: Turn = { conversation: null, controller: null, cancelled: false, ended: false };
    active.current = turn; setBusy(true); setError(""); setActivity([]); setActiveConversation(null);
    const onEvent = (event: AgentEvent) => {
      if (event.type === "connected") {
        turn.conversation = event.conversation_id;
        if (turn.cancelled) void stopGeneration(event.conversation_id).then(result => { if (result) { turn.controller?.abort(); void finish(turn); } else if (mounted.current) setError("停止请求未成功，请重试"); });
        if (mounted.current && active.current === turn && !turn.ended) {
          const id = `pending:${crypto.randomUUID()}`;
          setActiveConversation(event.conversation_id);
          setLiveNode({ id, conversation: event.conversation_id, parentId: branchParent.id, content: "", reasoning: "", createdAt: Date.now() });
          setSelected(id); setCamera(id);
          void refresh().catch(cause => setError(explain(cause)));
        }
      }
      if (!mounted.current || active.current !== turn || turn.ended) return;
      if (event.type === "token" || event.type === "reasoning" || event.type === "reasoning_snapshot") {
        setLiveNode(current => current && current.conversation === turn.conversation ? { ...current,
          content: event.type === "token" ? current.content + (event.token ?? "") : current.content,
          reasoning: event.type === "reasoning_snapshot" ? event.token ?? "" : event.type === "reasoning" ? current.reasoning + (event.token ?? "") : current.reasoning,
        } : current);
      }
      if (event.type === "model_route") {
        if (event.replace_content) setLiveNode(current => current ? { ...current, content: "" } : current);
        setActivity(current => [...current, event.phase === "exhausted" ? "模型组候选已用尽" : "模型路由已更新"]);
      }
      if (event.type === "tool_start" || event.type === "tool_end") setActivity(current => [...current, `${event.tool_name ?? "工具"} · ${event.type === "tool_start" ? "执行中" : event.status ?? "结束"}${event.error ? ` · ${event.error}` : ""}`]);
      if (event.type === "approval_required") setActivity(current => [...current, `等待审批：${event.tool_name ?? "工具"}${event.reason ? ` · ${event.reason}` : ""}`]);
      if (event.type === "error") setError(event.error ?? "模型执行失败");
      if (event.type === "stream_end") void finish(turn);
    };
    turn.controller = streamMessageCanvasBranch(branchParent.conversation_id, branchParent.id, text, onEvent, resources, options);
    return true;
  }

  const families = useMemo(() => conversationFamilies(overview?.conversations ?? []), [overview]);
  const byConversation=useMemo(()=>new Map(overview?.conversations.map(conversation=>[conversation.id,conversation])||[]),[overview]);
  useEffect(()=>{const target=initialConversationId&&byConversation.get(initialConversationId);if(target){setSourceFilter(target.origin?.kind||"desktop");setFamilyFilter(families.find(family=>family.conversationIds.includes(target.id))?.id||"");}},[initialConversationId,!!overview]);
  const sourceFamilies=useMemo(()=>families.filter(family=>sourceFilter==="all"||(byConversation.get(family.id)?.origin?.kind||"desktop")===sourceFilter),[families,sourceFilter,byConversation]);
  const messages = useMemo(() => {
    const family=familyFilter?families.find(family=>family.id===familyFilter):null;
    const visible = overview?.conversations.filter(conversation=>(sourceFilter==="all"||(conversation.origin?.kind||"desktop")===sourceFilter)&&(!family||family.conversationIds.includes(conversation.id))).flatMap(conversation=>conversation.nodes)||[];
    if (!liveNode) return visible;
    const current = visible.filter(message => message.conversation_id === liveNode.conversation);
    const user = [...current].reverse().find(message => message.role === "user");
    const withoutAnswer = visible.filter(message => message.conversation_id !== liveNode.conversation || message.role !== "assistant");
    const parent = user?.id || liveNode.parentId;
    if (!withoutAnswer.some(message => message.id === parent)) return withoutAnswer;
    return [...withoutAnswer, { id: liveNode.id, parent_id: parent, role: "assistant" as const, content: liveNode.content, reasoning: liveNode.reasoning, conversation_id: liveNode.conversation, created_at: liveNode.createdAt, status: "running", error: null }];
  },[overview,families,familyFilter,sourceFilter,liveNode]);
  const displayPositions = useMemo(() => placeNewMessages(layoutMessageTree(messages), positions), [messages, positions]);
  const path = useMemo(() => new Set(ancestorPath(messages, selected)), [messages, selected]);
  useEffect(() => {
    const projection: SpeechNode[] = messages.map(message => ({ id: message.id, type: "conversationSpeech", position: displayPositions[message.id], draggable: !message.id.startsWith("pending:"), selected: selected === message.id, data: { message, title:byConversation.get(message.conversation_id)?.title||"对话",origin:originLabel(byConversation.get(message.conversation_id)?.origin),onPath: path.has(message.id), busy, onBranch: () => chooseBranch(message) } }));
    setNodes(current => reconcileMessageNodes(current, projection));
  }, [messages, displayPositions, selected, path, busy, chooseBranch, setNodes,byConversation]);
  const byId = new Map(messages.map(message => [message.id, message]));
  const groupStatuses = new Map(overview?.conversations.map(conversation => [conversation.id, conversation.status]));
  const edges: Edge[] = messages.filter(message => message.parent_id && byId.has(message.parent_id)).map(message => {
    const source = byId.get(message.parent_id!)!;
    const branch = source.conversation_id !== message.conversation_id;
    return { id: `message:${message.id}`, source: source.id, target: message.id, ...messageEdgeHandles(displayPositions[source.id],displayPositions[message.id]), markerEnd:{type:MarkerType.ArrowClosed,width:13,height:13,color:branch||path.has(message.id)?"var(--accent-primary)":"var(--border-strong)"}, type: "default", label: branch ? "另一条路径" : undefined, animated: branch && running(groupStatuses.get(message.conversation_id) ?? ""), className: branch ? "conversation-canvas-branch-edge" : undefined, style: { stroke: branch || path.has(message.id) ? "var(--accent-primary)" : "var(--border-strong)", strokeWidth: branch || path.has(message.id) ? 2.4 : 1.5 } };
  });

  const focused=messages.find(message=>message.id===selected);

  return <section className="message-branch-workspace conversation-canvas" aria-label="全部对话消息画布">
    <header className="message-branch-toolbar">
      <div className="canvas-source-filter" role="group" aria-label="画布会话来源">{[{id:"desktop",label:"桌面"},{id:"channel",label:"渠道"},{id:"all",label:"全部"}].map(source=><button type="button" key={source.id} aria-pressed={sourceFilter===source.id} onClick={()=>{setSourceFilter(source.id);setFamilyFilter("");setSelected(null);setBranchParent(null);setCamera("fit");}}>{source.label}</button>)}</div>
      <label>定位对话<select aria-label="定位画布对话" value={familyFilter} onChange={event=>{setFamilyFilter(event.target.value);setSelected(null);setBranchParent(null);setCamera(event.target.value?`conversation:${event.target.value}`:"fit");}}><option value="">全部对话（{sourceFamilies.length}）</option>{sourceFamilies.map(family=><option key={family.id} value={family.id}>{family.title}{family.conversationIds.length>1?` · ${family.conversationIds.length} 条路径`:""}</option>)}</select></label>
      <Button variant="secondary" size="sm" onClick={()=>setCamera("fit")}><Maximize2 size={14}/>适应画布</Button><Button variant="secondary" size="sm" disabled={loading} onClick={()=>{setError("");void refresh().catch(cause=>setError(explain(cause)));}}><RefreshCw size={14}/>刷新</Button><span>{messages.length} 条消息 · {sourceFamilies.length} 组对话</span>
    </header>
    {error && <p className="message-branch-error" role="alert">{error}</p>}
    <div className="message-canvas-layout"><div className="message-canvas-primary"><div className="conversation-canvas-shell"><div className="message-branch-stage"><ReactFlow nodes={nodes} edges={edges} nodeTypes={nodeTypes} viewport={viewport} onViewportChange={setViewport} minZoom={0.1} maxZoom={2} nodesConnectable={false} deleteKeyCode={null}
      onNodesChange={onNodesChange}
      onNodeClick={(_, node) => { setSelected(node.id); if (!node.id.startsWith("pending:")) setBranchParent(node.data.message.role === "assistant" ? node.data.message : null); }} onNodeDrag={(_, node) => setPositions(current => ({ ...current, [node.id]: node.position }))} onNodeDragStop={(_, node) => { const next = { ...positionsRef.current, [node.id]: node.position }; setPositions(next); void persist(next); }} onMoveEnd={(_, next) => { void persist(positionsRef.current, next); }}><Background color="var(--border-soft)" gap={22} /><Controls showInteractive={false} /></ReactFlow>
      {loading && <div className="message-branch-empty" role="status">正在读取已有对话记录…</div>}
      {!loading && messages.length === 0 && <div className="message-branch-empty"><GitBranch size={28} /><p>当前筛选下没有消息，切换来源或选择其他对话查看</p></div>}
    </div>

    </div>
    {(busy || activity.length > 0) && <div className="message-branch-activity" role="status">{activity.slice(-3).map((line, index) => <p key={index}>{line}</p>)}{activeConversation && <Link to={`/chat/${activeConversation}`}>打开当前执行对话与审批</Link>}</div>}
    <BranchComposer parent={branchParent??undefined} onSend={send} onStop={()=>void stop()} onCancel={()=>setBranchParent(null)} busy={busy} disabled={!branchParent||loading} emptyHint="选择一条回答，沿当前思路继续" conversationId={branchParent?.conversation_id} conversationTitle={branchParent?byConversation.get(branchParent.conversation_id)?.title:undefined}/>
    </div>{focused&&<MessageSelectionPanel key={focused.id} message={focused} title={byConversation.get(focused.conversation_id)?.title} origin={originLabel(byConversation.get(focused.conversation_id)?.origin)} busy={busy} onClose={()=>setSelected(null)} onBranch={()=>chooseBranch(focused)}/>}</div>
  </section>;
}

export default function ConversationCanvas(props: { initialConversationId?: string | null }) {
  return <ReactFlowProvider><Overview {...props} /></ReactFlowProvider>;
}
