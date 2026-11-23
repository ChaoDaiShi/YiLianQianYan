import {useCallback, useEffect, useRef, useState} from "react";
import {Plus, RefreshCw, Monitor, PanelLeft} from "lucide-react";
import {useNavigate} from "react-router-dom";
import WechatExcerptDialog from "./WechatExcerptDialog";
import {chatExcerptDraft} from "./chatExcerpt";
import SoftwareMountShell from "./SoftwareMountShell";
import {Button, EmptyState, ErrorState, Input, Skeleton} from "../../components/ui";
import {listWorkspaces, type Workspace} from "../../api/workspaces";
import {
 acknowledgeSoftware, acknowledgeSoftwareLaunch, attachSoftware, deleteSoftware, discoverSoftware, launchSoftware, listSoftware,
 readWechatConversation, readWechatFrame, presentSoftware, releaseSoftware, resumeSoftware, saveSoftware, setSoftwareMonitor, SoftwareApiError, stopSoftware, isSoftwareSession, initializeSoftwareHost,
 type SoftwareCandidate, type SoftwareSession, type SoftwareSnapshot, type SoftwareRegistration,
} from "../../api/software";
import SoftwareRegistrationDialog from "./SoftwareRegistrationDialog";
import SoftwareSessionPanel from "./SoftwareSessionPanel";
import SoftwareDialogs, {type SoftwareDialog} from "./SoftwareDialogs";
import {emptySoftwareDraft, softwareError, softwareRegistrationDraft, validateSoftwareDraft, type SoftwareDraft} from "./softwareForm";
import {softwareVisibleViewport, filterSoftware, softwareHistory, softwareSessionActive, softwareSessionEmbedded} from "./softwarePolicy";

const blank: SoftwareSnapshot = {available: false, registrations: [], sessions: [], history: []};
export default function SoftwarePage() {
 const navigate = useNavigate();
 const [chatSource, setChatSource] = useState<{png: string} | {text: string} | null>(null);
 const [data, setData] = useState<SoftwareSnapshot>(blank), [loaded, setLoaded] = useState(false);
 const [loadError, setLoadError] = useState(""), [error, setError] = useState(""), [busy, setBusy] = useState(false);
 const [workspaces, setWorkspaces] = useState<Workspace[]>([]), [workspaceError, setWorkspaceError] = useState("");
 const [selectedId, setSelectedId] = useState(""), [sessionId, setSessionId] = useState("");
 const [catalogOpen, setCatalogOpen] = useState(false);
 const [query, setQuery] = useState(""), [workspaceFilter, setWorkspaceFilter] = useState("");
 const [dialog, setDialog] = useState<SoftwareDialog>(null), [draft, setDraft] = useState<SoftwareDraft>(emptySoftwareDraft);
 const [candidates, setCandidates] = useState<SoftwareCandidate[]>([]), [candidate, setCandidate] = useState("");
 const [reviewed, setReviewed] = useState(false), [uncertain, setUncertain] = useState<string | null>(null);
 const [launchOperationId, setLaunchOperationId] = useState("");
 const [approvedLaunch, setApprovedLaunch] = useState<SoftwareRegistration | null>(null);
 const [editingRegistration, setEditingRegistration] = useState<SoftwareRegistration | undefined>();
 const [removingRegistration, setRemovingRegistration] = useState<SoftwareRegistration | null>(null);
 const alive = useRef(true), flight = useRef(false), refreshSequence = useRef(0), presentation = useRef<Promise<unknown> | null>(null);
 const initialization = useRef<Promise<void> | null>(null), bindingReady = useRef(false);
 const dataRef = useRef(data), idRef = useRef(sessionId), viewport = useRef<HTMLDivElement>(null);
 dataRef.current = data; idRef.current = sessionId;
 const registration = data.registrations.find(item => item.id === selectedId);
 const catalogVisible = data.registrations.length > 0 && (catalogOpen || !registration);
 const sessions = data.sessions.filter(item => item.registration_id === selectedId);
 const session = sessions.find(item => item.id === sessionId);
 const shown = filterSoftware(data.registrations, query, workspaceFilter);
 const launchBlocked = Boolean(uncertain) || data.history.some(item => item.registration_id === selectedId && item.action === "launch" && item.state === "result_unknown" && !item.session_id);
 const currentSession = useCallback(() => dataRef.current.sessions.find(item => item.id === idRef.current), []);
 function visibleViewport() {
  const element = viewport.current;
  if (!element) return null;
  const main = element.closest(".software-main")?.getBoundingClientRect();
  const screen = {x: 0, y: 0, width: window.innerWidth, height: window.innerHeight};
  const clip = main ? {x: Math.max(0, main.x), y: Math.max(0, main.y), width: Math.min(main.right, screen.width) - Math.max(0, main.x), height: Math.min(main.bottom, screen.height) - Math.max(0, main.y)} : screen;
  return softwareVisibleViewport(element.getBoundingClientRect(), clip);
 }
 const commitSession = useCallback((next: SoftwareSession) => {
  dataRef.current = {...dataRef.current, sessions: [...dataRef.current.sessions.filter(item => item.id !== next.id), next]};
  if (alive.current) setData(dataRef.current);
 }, []);
 const refresh = useCallback(async () => {
  const ticket = ++refreshSequence.current;
  try {
   const next = await listSoftware();
   if (!alive.current || ticket !== refreshSequence.current) return;
   dataRef.current = next; setData(next); setLoaded(true); setLoadError("");
   setUncertain(previous => previous && !next.history.some(item => item.operation_id === previous) ? previous : null);
  } catch (cause) {if (alive.current && ticket === refreshSequence.current) {setLoaded(true); setLoadError(softwareError(cause));}}
 }, []);
 const initializeAndRefresh = useCallback(() => {
  if (initialization.current) return initialization.current;
  bindingReady.current = false; ++refreshSequence.current;
  const pending = (async () => {
   try {
    await initializeSoftwareHost();
    if (!alive.current) return;
    bindingReady.current = true;
    await refresh();
   } catch (cause) {
    if (alive.current) {setLoaded(true); setLoadError(softwareError(cause));}
   }
  })().finally(() => {if (initialization.current === pending) initialization.current = null;});
  initialization.current = pending;
  return pending;
 }, [refresh]);
 useEffect(() => {
  alive.current = true; void initializeAndRefresh();
  void listWorkspaces().then(result => {if (!alive.current) return; if (result.ok) setWorkspaces(result.data); else setWorkspaceError(result.error);});
  const resumeHost = () => {if (!document.hidden) void initializeAndRefresh();};
  window.addEventListener("focus", resumeHost); document.addEventListener("visibilitychange", resumeHost);
  const timer = window.setInterval(() => {if (!document.hidden && !flight.current && bindingReady.current && !initialization.current) void refresh();}, 4000);
  return () => {
   alive.current = false; ++refreshSequence.current; window.clearInterval(timer);
   window.removeEventListener("focus", resumeHost); document.removeEventListener("visibilitychange", resumeHost);
   const old = currentSession();
   if (old && !old.window.minimized && softwareSessionEmbedded(old)) void presentSoftware(old, "independent", crypto.randomUUID()).catch(() => {});
  };
 }, [refresh, currentSession, initializeAndRefresh]);
 useEffect(() => {
  if (!selectedId && data.registrations.length) setSelectedId(data.registrations[0].id);
  if (selectedId && !data.registrations.some(item => item.id === selectedId)) {setSelectedId(""); setSessionId("");}
 }, [data.registrations, selectedId]);
 useEffect(() => {
  if (!sessions.some(item => item.id === sessionId)) {
   setSessionId(sessions.find(softwareSessionEmbedded)?.id ?? sessions.find(softwareSessionActive)?.id ?? sessions[sessions.length - 1]?.id ?? "");
  }
 }, [selectedId, data.sessions, sessionId]);

 async function run<T>(operation: () => Promise<T>, operationId?: string, close = true): Promise<T | null> {
  if (flight.current) return null;
  flight.current = true; ++refreshSequence.current; setBusy(true); setError("");
  try {
   const value = await operation();
   if (isSoftwareSession(value)) {
    if (!alive.current && softwareSessionEmbedded(value)) await presentSoftware(value, "independent", crypto.randomUUID());
    else commitSession(value);
   }
   if (alive.current) {if (close) setDialog(null); await refresh();}
   return value;
  } catch (cause) {
   if (alive.current) {
    setError(softwareError(cause));
    if (operationId && cause instanceof SoftwareApiError && cause.uncertain) setUncertain(operationId);
    await refresh();
   }
   return null;
  } finally {flight.current = false; if (alive.current) setBusy(false);}
 }
 async function present(mode: "embedded" | "independent" | "focus", resize = false, target = currentSession()): Promise<SoftwareSession | null> {
  if (!target || !softwareSessionActive(target)) return null;
  if (flight.current) return null;
  if (mode === "embedded" && !resize) viewport.current?.scrollIntoView({block: "nearest"});
  const bounds = mode === "embedded" ? visibleViewport() : undefined;
  if (mode === "embedded" && !bounds) {setError("显示区域过小；请调整窗口后再嵌入。"); return null;}
  const operationId = crypto.randomUUID();
  const promise = run(async () => {
   if (mode === "embedded" && !resize) await initializeSoftwareHost();
   return presentSoftware(target, mode, operationId, bounds ?? undefined);
  }, operationId, !resize);
  presentation.current = promise;
  try {
   const reply = await promise;
   if (!alive.current) return null;
   // A dialog/visibility change may arrive while the native embed is pending.
   // Recheck after its acknowledgement, even when there is no later DOM mutation.
   if (reply && softwareSessionEmbedded(reply) && (document.hidden || idRef.current !== reply.id || document.querySelector('[role="dialog"], [role="alertdialog"]'))) {
    const latest = dataRef.current.sessions.find(item => item.id === reply.id) ?? reply;
    const restoreId = crypto.randomUUID();
    return await run(() => presentSoftware(latest, "independent", restoreId), restoreId, false);
   }
   return reply;
  } finally {if (presentation.current === promise) presentation.current = null;}
 }
 async function prepareDialog(next: SoftwareDialog) {
  if (flight.current) return;
  if (softwareSessionEmbedded(currentSession()) && !await present("independent")) return;
  if (next === "launch") setApprovedLaunch(registration ? {...registration} : null);
  if (next === "delete") setRemovingRegistration(registration ? {...registration} : null);
  setError(""); setReviewed(false); setDialog(next);
 }
 // All application dialogs sit below a native child HWND; restore it immediately.
 // Our own dialogs are opened only after a confirmed independent presentation.
 useEffect(() => {
  const observer = new MutationObserver(() => {
   const current = currentSession();
   if (document.querySelector('[role="dialog"], [role="alertdialog"]') && !current?.window.minimized && softwareSessionEmbedded(current) && !flight.current) void present("independent");
  });
  observer.observe(document.body, {childList: true, subtree: true});
  const hide = () => {
   const current = currentSession();
   if (document.hidden && !current?.window.minimized && softwareSessionEmbedded(current) && !flight.current) void present("independent");
  };
  document.addEventListener("visibilitychange", hide);
  return () => {observer.disconnect(); document.removeEventListener("visibilitychange", hide);};
 }, [currentSession]);
 useEffect(() => {
  if (!softwareSessionEmbedded(session) || !viewport.current) return;
  let timer = 0, disposed = false, last = "";
  const update = () => {
   window.clearTimeout(timer);
   timer = window.setTimeout(() => {
    const current = currentSession();
    if (disposed || flight.current || !viewport.current || current?.window.minimized || !softwareSessionEmbedded(current)) return;
    const bounds = visibleViewport();
    if (!bounds) {void present("independent"); return;}
    const signature = JSON.stringify(bounds);
    if (!bounds || signature === last) return;
    void present("embedded", true).then(reply => {if (reply) last = signature;});
   }, 120);
  };
  const observer = new ResizeObserver(update); observer.observe(viewport.current);
  window.addEventListener("resize", update); window.addEventListener("scroll", update, true); update();
  return () => {disposed = true; window.clearTimeout(timer); observer.disconnect(); window.removeEventListener("resize", update); window.removeEventListener("scroll", update, true);};
 }, [session?.id, session?.state, session?.window.mode]);

 async function selectRegistration(id: string) {
  if (flight.current || id === selectedId) return;
  if (softwareSessionEmbedded(currentSession()) && !await present("independent")) return;
  setSelectedId(id); setSessionId(""); setError("");
 }
 async function selectSession(id: string) {
  if (flight.current || id === sessionId) return;
  if (softwareSessionEmbedded(currentSession()) && !await present("independent")) return;
  setSessionId(id); setError("");
 }
 async function edit(create = false) {
  if (!create && !registration) return;
  if (softwareSessionEmbedded(currentSession()) && !await present("independent")) return;
  setDraft(create ? emptySoftwareDraft() : softwareRegistrationDraft(registration!));
  setEditingRegistration(create ? undefined : {...registration!});
  setError(""); setDialog(create ? "create" : "edit");
 }
 async function discover() {
  if (!registration) return;
  if (softwareSessionEmbedded(currentSession()) && !await present("independent")) return;
  const reply = await run(() => discoverSoftware(registration.id), undefined, false);
  if (!reply || !alive.current) return;
  if (reply.candidates.length === 1 && !launchBlocked) {
   await connectWindow(registration.id, reply.candidates[0], true);
   return;
  }
  setCandidates(reply.candidates);
  setCandidate("");
  setDialog("discover");
 }

 async function connectWindow(registrationId: string, window: SoftwareCandidate, embed: boolean) {
  const operationId = crypto.randomUUID();
  const next = await run(() => attachSoftware(registrationId, window.identity, operationId), operationId);
  if (!next || !alive.current) return;
  idRef.current = next.id;
  setSessionId(next.id);
  if (embed && next.window.embedding_supported) await present("embedded", false, next);
 }
 async function confirm() {
  if (!registration) return;
  if (dialog === "discover") {
   const chosen = candidates.find(item => item.identity.hwnd === candidate); if (!chosen || launchBlocked) return;
   await connectWindow(registration.id, chosen, false);
  } else if (dialog === "launch") {
   if (launchBlocked || !approvedLaunch) return;
   const operationId = crypto.randomUUID();
   const next = await run(() => launchSoftware(approvedLaunch, operationId), operationId);
   if (next) setSessionId(next.id);
  } else if (dialog === "delete" && removingRegistration) await run(() => deleteSoftware(removingRegistration));
  else if (dialog === "review-launch" && reviewed && launchOperationId) {
   const reply = await run(() => acknowledgeSoftwareLaunch(registration.id, launchOperationId));
   if (reply !== null) setUncertain(null);
  }
  else if (dialog === "release" && session) {
   const operationId = crypto.randomUUID(); await run(() => releaseSoftware(session, operationId), operationId);
  } else if (dialog === "review" && session && reviewed) await run(() => acknowledgeSoftware(session));
 }
 async function readChat() {
  const target = currentSession();
  if (!target || flight.current) return;
  const source = await run(async () => {
   const live = (await listSoftware()).sessions.find(item => item.id === target.id);
   if (live?.window.minimized) {
    const chat = await readWechatConversation(target);
    return {text: [chat.title, ...chat.messages, ...(chat.draft ? [`未发送草稿：${chat.draft}`] : [])].join("\n")};
   }
   return readWechatFrame(target);
  }, undefined, false);
  if (!source || !alive.current || currentSession()?.id !== target.id) return;
  // Capture before restoring the window: restoration can hide or resize the chat.
  if ("png" in source && softwareSessionEmbedded(currentSession()) && !await present("independent")) return;
  if (alive.current) setChatSource(source);
 }
 async function save() {
  const value = validateSoftwareDraft(draft);
  if (!value.value) {setError(value.error ?? "登记无效"); return;}
  const next = await run(() => saveSoftware({...value.value!, workspace_id: value.value!.workspace_id || null}, dialog === "edit" ? editingRegistration : undefined));
  if (next) setSelectedId(next.id);
 }
 return <SoftwareMountShell
  actions={<><Button size="sm" variant="secondary" aria-label="软件列表" aria-controls="software-catalog" aria-expanded={catalogVisible} disabled={!registration} onClick={() => setCatalogOpen(value => !value)}><PanelLeft className="h-4 w-4" /><span className="software-action-label">软件列表</span></Button><Button size="sm" variant="secondary" disabled={busy} onClick={() => void initializeAndRefresh()} aria-label="刷新软件状态"><RefreshCw className="h-4 w-4" /><span className="software-action-label">刷新</span></Button><Button size="sm" aria-label="登记软件" disabled={busy} onClick={() => void edit(true)}><Plus className="h-4 w-4" /><span className="software-action-label">登记软件</span></Button></>}>
  {!loaded ? <div className="software-loading"><Skeleton className="h-40" /></div> : <div className={"software-workspace" + (catalogVisible ? "" : " is-focused")}>
   {catalogVisible && <div id="software-catalog" className="software-catalog">
    <Input label="搜索登记软件" value={query} onChange={event => setQuery(event.target.value)} placeholder="软件名称或路径" />
    <label className="software-field">项目筛选<select value={workspaceFilter} onChange={event => setWorkspaceFilter(event.target.value)}><option value="">全部项目与个人软件</option>{workspaces.map(item => <option key={item.id} value={item.id}>{item.name}</option>)}</select></label>
     <div className="software-list" aria-label="已登记软件">{shown.length ? shown.map(item => <button key={item.id} className={"software-list-item" + (selectedId === item.id ? " is-selected" : "")} onClick={() => void selectRegistration(item.id)} disabled={busy} aria-pressed={selectedId === item.id}><Monitor className="h-4 w-4" /><span><strong>{item.name}</strong><small>{workspaces.find(project => project.id === item.workspace_id)?.name ?? (item.workspace_id ? "关联项目" : "个人软件")}</small></span></button>) : <EmptyState title="没有匹配的软件" description="调整搜索或项目筛选。" />}</div>
   </div>}
   <div className="software-main">
    {loadError && <ErrorState title="软件状态读取失败" description={loadError} action={<Button size="sm" onClick={() => void initializeAndRefresh()}>重试读取</Button>} />}
    {workspaceError && <p role="status" className="software-notice">项目列表读取失败：{workspaceError}。原有关联保留，不自动更换项目。</p>}
     {!data.available && !loadError && <p className="software-notice" role="status">连接、启动和挂载软件需要 Windows 桌面客户端。你仍可以管理登记。</p>}
    {error && !dialog && <ErrorState title="操作未完成" description={error} />}
    {uncertain && <div className="software-unknown" role="status"><p>请求响应不确定，请先刷新核对原窗口与操作记录；不自动重试。</p><p>原操作 ID：{uncertain}</p><Button size="sm" variant="secondary" disabled={busy} onClick={() => void refresh()}>刷新核对记录</Button></div>}
    {registration ? <>
     <SoftwareSessionPanel registration={registration} session={session} sessions={sessions} available={data.available} busy={busy} uncertain={launchBlocked}
      monitor={data.monitors?.find(item => item.registration_id === registration.id && (!session || !softwareSessionActive(session) || !item.session_id || item.session_id === session.id))}
      onReadChat={() => void readChat()}
      onMonitorChange={enabled => void run(() => setSoftwareMonitor(registration.id, enabled, session?.id), undefined, false)}
      onSession={id => void selectSession(id)} onDiscover={() => void discover()} onLaunch={() => {if (!launchBlocked) void prepareDialog("launch");}}
      onEdit={() => void edit()} onHistory={() => void prepareDialog("history")} onDelete={() => void prepareDialog("delete")} onDetails={() => void prepareDialog("details")}
      onMode={mode => void present(mode)} onStop={() => {const target = currentSession(); if (target) void run(() => stopSoftware(target));}}
      onResume={() => {const target = currentSession(); if (target) void run(() => resumeSoftware(target));}}
      onRelease={() => void prepareDialog("release")} onReview={() => void prepareDialog("review")} />
     <div ref={viewport} className="software-viewport" aria-label="软件原生显示区域" data-native-mode={session && softwareSessionActive(session) ? session.window.mode : "none"}>
       {softwareSessionEmbedded(session) ? <p>打开设置或离开页面时，窗口会恢复独立显示。</p> : !session && data.available && <div><Monitor className="h-7 w-7" /><p>连接软件后，窗口会显示在这里。</p></div>}
     </div>
    </> : !loadError && data.registrations.length === 0 && <EmptyState title="尚未登记软件" description="使用“登记软件”添加程序路径。" />}
   </div>
  </div>}
  {chatSource && <WechatExcerptDialog source={chatSource} onClose={() => setChatSource(null)} onUse={text => {setChatSource(null); navigate("/chat", {state: {suggestedText: chatExcerptDraft(text, "text" in chatSource)}});}}/>}
  {(dialog === "create" || dialog === "edit") && <SoftwareRegistrationDialog registration={dialog === "edit" ? editingRegistration : undefined} draft={draft} workspaces={workspaces} active={dialog === "edit" && sessions.some(softwareSessionActive)} busy={busy} error={error} onDraft={setDraft} onSave={() => void save()} onClose={() => setDialog(null)} />}
  {registration && <SoftwareDialogs dialog={dialog} registration={dialog === "launch" && approvedLaunch ? approvedLaunch : dialog === "delete" && removingRegistration ? removingRegistration : registration} session={session} candidates={candidates} candidate={candidate} onCandidate={setCandidate} history={softwareHistory(data.history, registration.id)} evidence={data.capability_evidence?.find(record => record.registration_id === registration.id && record.session_id === (session?.id ?? null) && record.generation === (session?.generation ?? null))} busy={busy} error={error} reviewed={reviewed} onReviewed={setReviewed} launchOperationId={launchOperationId} mutationBlocked={launchBlocked} onReviewLaunch={id => {setLaunchOperationId(id); setReviewed(false); setDialog("review-launch");}} onClose={() => setDialog(null)} onConfirm={() => void confirm()} />}
 </SoftwareMountShell>;
}
