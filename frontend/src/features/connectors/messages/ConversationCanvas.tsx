import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Link } from "react-router-dom";
import { createConversationBranch } from "../../../api/conversations";
import { bindResources } from "../../../api/resources";
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
import { sendMessage, stopGeneration, type AgentEvent } from "../../../api/chat";
import { getMessageCanvas, saveMessageCanvasLayout, type ConnectorMessage, type MessageCanvasOverview } from "../../../api/connectorMessages";
import { ancestorPath, layoutMessageTree, placeNewMessages, type MessagePosition } from "./messageTree";
import { conversationFamilies, conversationGroupEntries, layoutConversationGroups } from "./messageForest";
import { reconcileMessageNodes } from "./reconcileMessageNodes";
import { CONVERSATION_HISTORY_CHANGED, notifyConversationHistoryChanged } from "../../../components/chat/conversationHistoryEvents";
import { useGlobalVoiceContext } from "../../voice/GlobalVoiceHost";
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
interface Turn { conversation: string; controller: AbortController | null; cancelled: boolean; ended: boolean }
interface LiveNode { id: string; conversation: string; parentId: string | null; previousIds: string[]; content: string; reasoning: string; createdAt: number }

function Overview({ initialConversationId }: { initialConversationId?: string | null }) {
  const { conversationRefresh } = useGlobalVoiceContext();
  const flow = useReactFlow<SpeechNode>();
  const canvasWidth=useStore(state=>state.width);
  const canvasHeight=useStore(state=>state.height);
  const initialized = useNodesInitialized();
  const [overview, setOverview] = useState<MessageCanvasOverview | null>(null);
  const [sourceFilter,setSourceFilter]=useState("desktop");
  const [familyFilter,setFamilyFilter]=useState("");
  const [activePath,setActivePath]=useState<string|null>(null);
  const activePathRef = useRef(activePath);
  activePathRef.current = activePath;
  const [forking,setForking]=useState(false);
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
    if (!reopen) {
      const allFamilies = conversationFamilies(next.conversations);
      setFamilyFilter(current => allFamilies.some(family => family.id === current) ? current : allFamilies.find(family => family.conversationIds.includes(activePathRef.current ?? ""))?.id ?? "");
      setActivePath(current => next.conversations.some(conversation => conversation.id === current) ? current : null);
      const nodes = next.conversations.flatMap(conversation => conversation.nodes);
      setSelected(current => current && (current.startsWith("pending:") || nodes.some(node => node.id === current)) ? current : null);
      setBranchParent(current => current && nodes.some(node => node.id === current.id) ? current : null);
    }
    setPositions(current => {
      const automatic = layoutConversationGroups(next.conversations);
      const saved = Object.fromEntries(next.layout.nodes.map(point => [point.id, { x: point.x, y: point.y }]));
      const existing = reopen ? saved : { ...saved, ...current };
      return Object.assign({}, ...conversationFamilies(next.conversations).map(family => {
        const ids = new Set(next.conversations.filter(entry => family.conversationIds.includes(entry.id)).flatMap(entry => entry.nodes.map(node => node.id)));
        return placeNewMessages(Object.fromEntries(Object.entries(automatic).filter(([id]) => ids.has(id))), Object.fromEntries(Object.entries(existing).filter(([id]) => ids.has(id))));
      }));
    });
    if (reopen) {
      setViewport(next.layout.viewport);
      const latest = initialConversationId ? next.conversations.find(item => item.id === initialConversationId) : [...next.conversations].sort((a, b) => b.updated_at - a.updated_at)[0];
      const family = latest && conversationFamilies(next.conversations).find(item => item.conversationIds.includes(latest.id));
      const latestNode = latest && (latest.nodes[latest.nodes.length - 1] ?? next.conversations.flatMap(item => item.nodes).find(node => node.id === latest.branch?.parent_node_id));
      setFamilyFilter(family?.id ?? ""); setActivePath(latest?.id ?? null);
      setSourceFilter(latest?.origin?.kind || "desktop");
      setSelected(latestNode?.id ?? null); setBranchParent(latestNode ?? null);
      setCamera(latest ? `conversation:${latest.id}` : "fit");
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
    const update = () => { if (!loading) void refresh().catch(cause => { if (mounted.current) setError(explain(cause)); }); };
    window.addEventListener(CONVERSATION_HISTORY_CHANGED, update);
    const timer = window.setInterval(() => { if (!loading && document.visibilityState === "visible" && !active.current && !hasRunning) update(); }, 5000);
    return () => { window.removeEventListener(CONVERSATION_HISTORY_CHANGED, update); window.clearInterval(timer); };
  }, [refresh, hasRunning, loading]);
  useEffect(() => { if (!loading && conversationRefresh) void refresh().catch(cause => { if (mounted.current) setError(explain(cause)); }); }, [conversationRefresh, refresh, loading]);

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
    if (busy || forking) return;
    setForking(true); setError("");
    void createConversationBranch(message.conversation_id, message.id).then(async created => {
      notifyConversationHistoryChanged();
      const next = await refresh();
      if (!mounted.current || !next) return;
      const family = conversationFamilies(next.conversations).find(item => item.conversationIds.includes(created.id));
      setFamilyFilter(family!.id); setActivePath(created.id); setSelected(message.id);
      setBranchParent(message); setCamera(message.id);
    }).catch(cause => { if (mounted.current) setError(explain(cause)); })
      .finally(() => { if (mounted.current) setForking(false); });
  }, [busy, forking, refresh]);

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
    if (!turn.controller) return;
    const result = await stopGeneration(turn.conversation);
    if (!result) { if (mounted.current) setError("停止请求未成功，请重试"); return; }
    turn.controller?.abort(); await finish(turn);
  }

  async function send(text: string, resources: string[] = [], options?: ChatRequestOptions) {
    if (!activePath || busy || forking || (!text.trim() && !resources.length)) return false;
    const previousIds = overview?.conversations.find(item => item.id === activePath)?.nodes.map(node => node.id) ?? [];
    const turn: Turn = { conversation: activePath, controller: null, cancelled: false, ended: false };
    active.current = turn; setBusy(true); setError(""); setActivity([]); setActiveConversation(null);
    try {
      if (resources.length) await bindResources({kind:"conversation",conversation_id:activePath}, resources);
    } catch (cause) {
      setError(explain(cause)); active.current = null; setBusy(false); return false;
    }
    if (turn.cancelled) { active.current = null; setBusy(false); return false; }
    const onEvent = (event: AgentEvent) => {
      if (event.type === "connected") {
        turn.conversation = event.conversation_id;
        if (turn.cancelled) void stopGeneration(event.conversation_id).then(result => { if (result) { turn.controller?.abort(); void finish(turn); } else if (mounted.current) setError("停止请求未成功，请重试"); });
        if (mounted.current && active.current === turn && !turn.ended) {
          const id = `pending:${crypto.randomUUID()}`;
          setActiveConversation(event.conversation_id);
          setLiveNode({ id, conversation: event.conversation_id, parentId: branchParent?.id ?? null, previousIds, content: "", reasoning: "", createdAt: Date.now() });
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
    turn.controller = sendMessage(text, activePath, onEvent, null, {...options,resource_ids:resources});
    return true;
  }

  const families = useMemo(() => conversationFamilies(overview?.conversations ?? []), [overview]);
  const byConversation=useMemo(()=>new Map(overview?.conversations.map(conversation=>[conversation.id,conversation])||[]),[overview]);

  const sourceFamilies=useMemo(()=>families.filter(family=>sourceFilter==="all"||(byConversation.get(family.id)?.origin?.kind||"desktop")===sourceFilter),[families,sourceFilter,byConversation]);
  const familyEntries = conversationGroupEntries(overview?.conversations ?? [], familyFilter);
  function openPath(id: string) {
    const entry = byConversation.get(id)!;
    const tip = entry.nodes[entry.nodes.length - 1] ?? overview?.conversations.flatMap(item => item.nodes).find(node => node.id === entry.branch?.parent_node_id);
    setActivePath(id); setSelected(tip?.id ?? null); setBranchParent(tip ?? null);
    setCamera(`conversation:${id}`); setActivity([]); setActiveConversation(null);
  }
  function openFamily(id: string) {
    setFamilyFilter(id);
    const family = families.find(item => item.id === id);
    const latest = [...(overview?.conversations.filter(entry => family?.conversationIds.includes(entry.id)) ?? [])].sort((a,b) => b.updated_at-a.updated_at)[0];
    if (latest) openPath(latest.id);
    else { setActivePath(null); setSelected(null); setBranchParent(null); setCamera("fit"); }
  }
  function openSource(source: string) {
    setSourceFilter(source);
    const latest = [...(overview?.conversations.filter(entry => source === "all" || (entry.origin?.kind || "desktop") === source) ?? [])].sort((a,b)=>b.updated_at-a.updated_at)[0];
    const family = latest && families.find(item => item.conversationIds.includes(latest.id));
    openFamily(family?.id ?? "");
  }
  const messages = useMemo(() => {
    const visible = conversationGroupEntries(overview?.conversations ?? [],familyFilter).flatMap(entry=>entry.nodes);
    if (!liveNode) return visible;
    const previous = new Set(liveNode.previousIds);
    const current = visible.filter(message => message.conversation_id === liveNode.conversation && !previous.has(message.id));
    const user = [...current].reverse().find(message => message.role === "user");
    const replaced = new Set(current.filter(message => message.role === "assistant").map(message => message.id));
    const withoutAnswer = visible.filter(message => !replaced.has(message.id));
    const parent = user?.id ?? liveNode.parentId;
    if (!parent || !withoutAnswer.some(message => message.id === parent)) return withoutAnswer;
    return [...withoutAnswer, { id: liveNode.id, parent_id: parent, role: "assistant" as const, content: liveNode.content, reasoning: liveNode.reasoning, conversation_id: liveNode.conversation, created_at: liveNode.createdAt, status: "running", error: null }];
  },[overview,families,familyFilter,liveNode]);
  const displayPositions = useMemo(() => {
    const ids = new Set(messages.map(message => message.id));
    return placeNewMessages(layoutMessageTree(messages), Object.fromEntries(Object.entries(positions).filter(([id]) => ids.has(id))));
  }, [messages, positions]);
  const path = useMemo(() => new Set(ancestorPath(messages, selected)), [messages, selected]);
  useEffect(() => {
    const projection: SpeechNode[] = messages.map(message => ({ id: message.id, type: "conversationSpeech", position: displayPositions[message.id], draggable: !message.id.startsWith("pending:"), selected: selected === message.id, data: { message, title:byConversation.get(message.conversation_id)?.title||"对话",origin:originLabel(byConversation.get(message.conversation_id)?.origin),onPath: path.has(message.id), busy:busy||forking, onBranch: () => chooseBranch(message) } }));
    setNodes(current => reconcileMessageNodes(current, projection));
  }, [messages, displayPositions, selected, path, busy, chooseBranch, setNodes,byConversation,forking]);
  const byId = new Map(messages.map(message => [message.id, message]));
  const groupStatuses = new Map(overview?.conversations.map(conversation => [conversation.id, conversation.status]));
  const edges: Edge[] = messages.filter(message => message.parent_id && byId.has(message.parent_id)).map(message => {
    const source = byId.get(message.parent_id!)!;
    const branch = source.conversation_id !== message.conversation_id;
    return { id: `message:${message.id}`, source: source.id, target: message.id, ...messageEdgeHandles(displayPositions[source.id],displayPositions[message.id]), markerEnd:{type:MarkerType.ArrowClosed,width:13,height:13,color:branch||path.has(message.id)?"var(--accent-primary)":"var(--border-strong)"}, type: "default", label: branch ? "另一条路径" : undefined, animated: branch && running(groupStatuses.get(message.conversation_id) ?? ""), className: branch ? "conversation-canvas-branch-edge" : undefined, style: { stroke: branch || path.has(message.id) ? "var(--accent-primary)" : "var(--border-strong)", strokeWidth: branch || path.has(message.id) ? 2.4 : 1.5 } };
  });

  const focused=messages.find(message=>message.id===selected);

  return <section className="message-branch-workspace conversation-canvas" aria-label="对话组消息画布" data-conversation-group={familyFilter}>
    <header className="message-branch-toolbar">
      <div className="canvas-source-filter" role="group" aria-label="画布会话来源">{[{id:"desktop",label:"桌面"},{id:"channel",label:"渠道"},{id:"all",label:"全部来源"}].map(source=><button type="button" key={source.id} aria-pressed={sourceFilter===source.id} disabled={busy||forking} onClick={()=>openSource(source.id)}>{source.label}</button>)}</div>
      <label>对话组<select aria-label="当前对话组" value={familyFilter} disabled={busy||forking||loading} onChange={event=>openFamily(event.target.value)}>{!familyFilter&&<option value="">{sourceFamilies.length?"选择对话组":"暂无对话组"}</option>}{sourceFamilies.map(family=><option key={family.id} value={family.id}>{family.title}</option>)}</select></label>
      {familyEntries.length>0&&<label>续聊路径<select aria-label="当前续聊路径" value={activePath??""} disabled={busy||forking} onChange={event=>openPath(event.target.value)}>{familyEntries.map((entry,index)=><option key={entry.id} value={entry.id}>{entry.id===familyFilter?"主线":`分支 ${index}`} · {entry.title}</option>)}</select></label>}
      <Button variant="secondary" size="sm" onClick={()=>setCamera("fit")}><Maximize2 size={14}/>适应画布</Button><Button variant="secondary" size="sm" disabled={loading} onClick={()=>{setError("");void refresh().catch(cause=>setError(explain(cause)));}}><RefreshCw size={14}/>刷新</Button><span>{messages.length} 条消息 · {familyEntries.length} 条路径</span>
    </header>
    {error && <p className="message-branch-error" role="alert">{error}</p>}
    <div className="message-canvas-layout"><div className="message-canvas-primary"><div className="conversation-canvas-shell"><div className="message-branch-stage"><ReactFlow nodes={nodes} edges={edges} nodeTypes={nodeTypes} viewport={viewport} onViewportChange={setViewport} minZoom={0.1} maxZoom={2} nodesConnectable={false} deleteKeyCode={null}
      onNodesChange={onNodesChange}
      onNodeClick={(_, node) => setSelected(node.id)} onNodeDrag={(_, node) => setPositions(current => ({ ...current, [node.id]: node.position }))} onNodeDragStop={(_, node) => { const next = { ...positionsRef.current, [node.id]: node.position }; setPositions(next); void persist(next); }} onMoveEnd={(_, next) => { void persist(positionsRef.current, next); }}><Background color="var(--border-soft)" gap={22} /><Controls showInteractive={false} /></ReactFlow>
      {loading && <div className="message-branch-empty" role="status">正在读取已有对话记录…</div>}
      {!loading && messages.length === 0 && <div className="message-branch-empty"><GitBranch size={28} /><p>当前筛选下没有消息，切换来源或选择其他对话查看</p></div>}
    </div>

    </div>
    {(busy || activity.length > 0) && <div className="message-branch-activity" role="status">{activity.slice(-3).map((line, index) => <p key={index}>{line}</p>)}{activeConversation && <Link to={`/chat/${activeConversation}`}>打开当前执行对话与审批</Link>}</div>}
    <BranchComposer contextLabel="从当前路径的最新消息继续" parent={branchParent??undefined} onSend={send} onStop={()=>void stop()} busy={busy} disabled={!activePath||loading||forking} emptyHint="选择一个对话组开始" conversationId={activePath} conversationTitle={activePath?byConversation.get(activePath)?.title:undefined}/>
    </div>{focused&&<MessageSelectionPanel key={focused.id} message={focused} title={byConversation.get(focused.conversation_id)?.title} origin={originLabel(byConversation.get(focused.conversation_id)?.origin)} busy={busy||forking} onClose={()=>setSelected(null)} onBranch={()=>chooseBranch(focused)}/>}</div>
  </section>;
}

export default function ConversationCanvas(props: { initialConversationId?: string | null }) {
  return <ReactFlowProvider><Overview {...props} /></ReactFlowProvider>;
}
