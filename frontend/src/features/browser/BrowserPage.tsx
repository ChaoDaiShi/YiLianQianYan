import { useCallback, useEffect, useRef, useState } from "react";
import { useSearchParams } from "react-router-dom";
import { Button, ErrorState, EmptyState, Modal } from "../../components/ui";
import { browserExecute, browserTabs, nativeBrowserAvailable, openedBrowserTab, listBrowserUnknown, listProjectSites, browserUiState, browserCommand, listenForBrowserShortcut, type BrowserUiState, type BrowserFindResult, type BrowserShortcut, type UnknownBrowserOperation, type BrowserTab, type ProjectSite } from "../../api/browser";
import { listWorkspaces, type Workspace } from "../../api/workspaces";
import { openInSystemBrowser } from "../../components/chat/externalLink";
import { browserOrigin, normalizeBrowserAddress, resolveBrowserInput } from "./browserPolicy";
import BrowserChrome from "./BrowserChrome";
import BrowserStartPage from "./BrowserStartPage";
import UnknownBrowserReview from "./UnknownBrowserReview";
import NativeBrowserNotice from "./NativeBrowserNotice";
import SiteManager from "./SiteManager";
import PendingNativeActions from "./PendingNativeActions";
import BrowserFindBar from "./BrowserFindBar";
import BrowserTools from "./BrowserTools";
import BrowserCredentialVault from "./BrowserCredentialVault";
import "./browser.css";
export default function BrowserPage({ detached = false }: {detached?: boolean}) {
 const [query, setQuery] = useSearchParams(), [tabs,setTabs] = useState<BrowserTab[]>([]), [selected,setSelected] = useState(query.get("tab") || ""), [address,setAddress] = useState(query.get("url") || ""), [project,setProject] = useState(query.get("workspace") || ""), [workspaces,setWorkspaces] = useState<Workspace[]>([]), [error,setError] = useState(""), [pendingCount,setPendingCount] = useState(0), [dialog,setDialog] = useState<"sites"|"history"|"editor"|"tools"|"review"|"requests"|"close"|"vault"|null>(null), [closeTarget,setCloseTarget] = useState<BrowserTab|null>(null), [dialogs,setDialogs] = useState(false), [unknown,setUnknown]=useState<UnknownBrowserOperation[]>([]), [dismissedNotice,setDismissedNotice]=useState("");
 const [temporary,setTemporary]=useState(false),[ui,setUi]=useState<BrowserUiState>({requests:[],zoom:{}}),[findOpen,setFindOpen]=useState(false),[findQuery,setFindQuery]=useState(""),[findResult,setFindResult]=useState<BrowserFindResult|null>(null),[findError,setFindError]=useState("");
 const addressRef=useRef<HTMLInputElement>(null),shortcutRef=useRef<(shortcut:BrowserShortcut)=>void>(()=>{}),busy=pendingCount>0,sitesOpen=dialog==="sites"||dialog==="history"||dialog==="editor";
 const canvas = useRef<HTMLDivElement>(null), current = tabs.find(t=>t.tab_id===selected), currentRef = useRef<BrowserTab | undefined>(current), tabsRef = useRef(tabs), mounted=useRef(true), opening = useRef(false), newSession = useRef(false), refreshFlight=useRef<Promise<void>|null>(null), refreshEpoch=useRef(0); currentRef.current=current; tabsRef.current=tabs;
 const available=nativeBrowserAvailable();
 const [sites, setSites] = useState<ProjectSite[]>([]), [sitesLoading, setSitesLoading] = useState(true), [sitesError, setSitesError] = useState("");
 useEffect(() => {
  if (sitesOpen) return;
  let active = true;
  setSitesLoading(true); setSitesError(""); setSites([]);
  void listProjectSites(project || undefined).then(result => {
   if (active) setSites(result.sites.filter(site => site.status === "active" && site.bookmarked));
  }).catch(cause => { if (active) setSitesError(cause instanceof Error ? cause.message : String(cause)); })
   .finally(() => { if (active) setSitesLoading(false); });
  return () => { active = false; };
 }, [project, sitesOpen]);
 const refresh = useCallback(()=>{
  if(!available)return Promise.resolve();
  if(refreshFlight.current)return refreshFlight.current;
  const epoch=refreshEpoch.current;
  const active=()=>mounted.current&&epoch===refreshEpoch.current;
  const work=(async()=>{try{
   const value=await browserTabs();if(!active())return;setTabs(value);
   setSelected(old=>newSession.current?"":value.some(t=>t.tab_id===old)?old:detached?query.get("tab")||"":value[0]?.tab_id||"");
   const items=await listBrowserUnknown();if(active())setUnknown(items);
   const state=await browserUiState();if(active())setUi(state);
  }catch(e){if(active())setError(e instanceof Error?e.message:String(e));}})();
  refreshFlight.current=work;void work.finally(()=>{if(refreshFlight.current===work)refreshFlight.current=null;});return work;
 },[available,detached,query]);
 useEffect(()=>{mounted.current=true;const epoch=++refreshEpoch.current;void listWorkspaces().then(r=>{if(mounted.current&&refreshEpoch.current===epoch){if(r.ok)setWorkspaces(r.data.filter(w=>w.status==="active"));else setError(r.error);}});void refresh();const timer=window.setInterval(()=>void refresh(),1000);return()=>{mounted.current=false;++refreshEpoch.current;clearInterval(timer);};},[refresh]);
 useEffect(()=>{const tabId=query.get("tab");if(tabId){newSession.current=false;setSelected(tabId);}},[query.get("tab")]);
 useEffect(()=>{
  if(detached||!query.get("new"))return;
  startSession();
 },[query.get("new"),detached]);
 useEffect(()=>{if(current){setAddress(current.url);setProject(current.workspace_id || "");}},[current?.url,current?.workspace_id]);
 async function action(name: string,payload:Record<string,unknown>={},tab=currentRef.current) {setPendingCount(n=>n+1);setError("");try {const reply=await browserExecute(name,payload,name === "open" ? undefined : tab); await refresh();return reply;}catch(e){if(mounted.current)setError(e instanceof Error?e.message:String(e));return null;}finally{if(mounted.current)setPendingCount(n=>n-1);}}
 async function open(url: string, site?: ProjectSite) {
  try {
   const normalized = normalizeBrowserAddress(url);
   if (detached) {
    if (!currentRef.current) throw new Error("此网页已关闭，请回到主窗口打开站点");
    const reply = await action("navigate", {url: normalized, allowed_origins: [browserOrigin(normalized)]});
    if (reply) setDialog(null);
    return;
   }
   const reply = await action("open", {
    url: normalized, workspace_id: site?.workspace_id || project || null,
    site_id: site?.id || null, ephemeral: temporary, allowed_origins: [browserOrigin(normalized)],
   }, undefined);
   if (reply) {
    const tab = openedBrowserTab(reply);
    newSession.current = false;
    setSelected(tab.tab_id);
    setDialog(null);
   }
  } catch (cause) { setError(cause instanceof Error ? cause.message : String(cause)); }
 }
 useEffect(()=>{const url=query.get("url");if(!url||!available||opening.current)return;opening.current=true;void open(url).then(()=>{setQuery(old=>{const next=new URLSearchParams(old);next.delete("url");return next;},{replace:true});opening.current=false;});},[query.get("url"),available]);
 // Native child views sit above DOM overlays. Hide the same view for ANY app dialog.
 useEffect(()=>{let previous=false;const check=()=>{const next=!!document.querySelector('[role="dialog"], [role="alertdialog"]');if(previous!==next){previous=next;setDialogs(next);}};const observer=new MutationObserver(check);observer.observe(document.body,{childList:true,subtree:true});check();return()=>observer.disconnect();},[]);
 // Releasing this selected tab is distinct from updating its presentation.
 // Dialog, navigation and geometry updates must not enqueue an ownership-release hide.
 useEffect(() => {
  if (!available || !selected) return;
  return () => {
   const tab = tabsRef.current.find(item => item.tab_id === selected);
   if (tab && ((detached && tab.mode !== "embedded") || (!detached && tab.mode !== "detached"))) {
    void browserExecute("present", {mode: "hidden"}, tab).catch(() => {});
   }
  };
 }, [selected, available, detached]);
 useEffect(()=>{if(!available)return;let cancelled=false,frame=0,last="";
 const present=()=>{cancelAnimationFrame(frame);frame=requestAnimationFrame(()=>{const tab=currentRef.current,element=canvas.current;if(cancelled||!tab||!element||((!detached&&tab.mode==="detached")||(detached&&tab.mode==="embedded")))return;const rect=element.getBoundingClientRect();const payload=dialogs?{mode:"hidden"}:{mode:detached?"detached":"embedded",x:rect.x,y:rect.y,width:Math.max(1,rect.width),height:Math.max(1,rect.height)};const signature=JSON.stringify(payload);if(signature===last)return;last=signature;void browserExecute("present",payload,tab).catch(e=>{if(!cancelled)setError(e instanceof Error?e.message:String(e));});});};
 const observer=new ResizeObserver(present);if(canvas.current)observer.observe(canvas.current);window.addEventListener("resize",present);present();return()=>{cancelled=true;cancelAnimationFrame(frame);observer.disconnect();window.removeEventListener("resize",present);};},[selected,available,dialogs,detached,!!current,current?.mode,current?.navigation_id]);
 async function navigate(input = address){try{const url=resolveBrowserInput(input);if(current)await action("navigate",{url,allowed_origins:[browserOrigin(url)]});else await open(url);}catch(e){setError(e instanceof Error?e.message:String(e));}}
 function selectTab(tab: BrowserTab) {
  newSession.current = false;
  setSelected(tab.tab_id);
  setAddress(tab.url);
  setFindOpen(false);setFindResult(null);
 }
 function startSession() {
  newSession.current = true;
  setSelected("");
  setAddress("");
  setError("");
  setFindOpen(false);setFindResult(null);
  setQuery(old=>{const next=new URLSearchParams(old);next.delete("tab");next.delete("url");next.delete("new");return next;},{replace:true});
 }
 function openExternal() {
  try { void openInSystemBrowser(resolveBrowserInput(address)).catch(cause => setError(String(cause))); }
  catch (cause) { setError(cause instanceof Error ? cause.message : String(cause)); }
 }
 async function closeTab(tab:BrowserTab) {
  const index=tabsRef.current.findIndex(item=>item.tab_id===tab.tab_id);
  const wasSelected=currentRef.current?.tab_id===tab.tab_id;
  const neighbor=tabsRef.current[index+1]||tabsRef.current[index-1];
  const reply=await action("close",{},tab);
  if(reply){if(wasSelected){newSession.current=!neighbor;setSelected(neighbor?.tab_id||"");setAddress(neighbor?.url||"");setFindOpen(false);}setDialog(null);setCloseTarget(null);}
 }
 function requestClose(tab:BrowserTab) {
  if(busy)return;
  if(tab.ephemeral){setCloseTarget(tab);setDialog("close");}else void closeTab(tab);
 }
 async function zoom(factor:number) {
  const tab=currentRef.current;if(!tab)return;
  setPendingCount(n=>n+1);setError("");
  try{const reply=await browserCommand<{factor:number}>(tab,"zoom",{factor});setUi(old=>({...old,zoom:{...old.zoom,[tab.tab_id]:reply.factor}}));}
  catch(cause){setError(cause instanceof Error?cause.message:String(cause));}
  finally{setPendingCount(n=>n-1);}
 }
 async function find(backwards:boolean,query=findQuery) {
  const tab=currentRef.current;if(!tab)return;
  setPendingCount(n=>n+1);setFindError("");
  try{const result=await browserCommand<BrowserFindResult>(tab,"find",{query,backwards});if(currentRef.current?.tab_id===tab.tab_id)setFindResult(result);}
  catch(cause){setFindError(cause instanceof Error?cause.message:String(cause));}
  finally{setPendingCount(n=>n-1);}
 }
 async function clearData() {
  const tab=currentRef.current;if(!tab)return;
  setPendingCount(n=>n+1);setError("");
  try{await browserCommand(tab,"clear_data");await refresh();setDialog(null);}
  catch(cause){setError(cause instanceof Error?cause.message:String(cause));}
  finally{setPendingCount(n=>n-1);}
 }
 async function newTabInMain() {
  const tab=currentRef.current;if(!tab)return;
  setPendingCount(n=>n+1);setError("");
  try{await browserCommand(tab,"new_tab");}
  catch(cause){setError(cause instanceof Error?cause.message:String(cause));}
  finally{setPendingCount(n=>n-1);}
 }
 shortcutRef.current=shortcut=>{
  if(document.querySelector('[role="dialog"], [role="alertdialog"]'))return;
  if(shortcut.tab_id&&shortcut.tab_id!==currentRef.current?.tab_id)return;
  if(shortcut.action==="focus-address"){addressRef.current?.focus();addressRef.current?.select();}
  else if(shortcut.action==="new-tab"){if(detached)void newTabInMain();else startSession();}
  else if(shortcut.action==="close-tab"&&currentRef.current)requestClose(currentRef.current);
  else if(shortcut.action==="next-tab"||shortcut.action==="previous-tab"){
   const owned=tabsRef.current.filter(tab=>!detached||tab.tab_id===query.get("tab"));
   const index=owned.findIndex(tab=>tab.tab_id===currentRef.current?.tab_id);
   const next=owned[(index+(shortcut.action==="next-tab"?1:-1)+owned.length)%owned.length];if(next)selectTab(next);
  }
 };
 useEffect(()=>{
  const keydown=(event:KeyboardEvent)=>{
   if(document.querySelector('[role="dialog"], [role="alertdialog"]')||!event.ctrlKey||event.altKey)return;
   const key=event.key.toLowerCase();
   const action=key==="l"?"focus-address":key==="t"?"new-tab":key==="w"?"close-tab":key==="tab"?(event.shiftKey?"previous-tab":"next-tab"):null;
   if(action){event.preventDefault();shortcutRef.current({action,tab_id:""});}
   else if(key==="f"&&currentRef.current){event.preventDefault();setFindOpen(true);}
  };
  window.addEventListener("keydown",keydown);
  return()=>window.removeEventListener("keydown",keydown);
 },[]);
 // AppRoot owns browser:opened routing; shortcuts keep one subscription and read the current handler.
 useEffect(()=>{
  if(!available)return;let cancelled=false;const cleanups:Array<()=>void>=[];
  const add=(promise:Promise<()=>void>)=>void promise.then(cleanup=>{if(cancelled)cleanup();else cleanups.push(cleanup);}).catch(cause=>{if(!cancelled)setError(cause instanceof Error?cause.message:String(cause));});
  add(listenForBrowserShortcut(shortcut=>shortcutRef.current(shortcut)));
  return()=>{cancelled=true;cleanups.forEach(cleanup=>cleanup());};
 },[available]);
 const noticeKey=current?.error?`${current.tab_id}:${current.navigation_id}:${current.error}`:"";
 const projectName=workspaces.find(w=>w.id===(current?.workspace_id||project))?.name;
 const requests=ui.requests.filter(request=>!detached||request.tab_id===query.get("tab"));
 const starting = available && !current && !detached;
 return <section className={`browser-page ${detached?"browser-page-detached":""}`} aria-label="内置浏览器">
 <BrowserChrome detached={detached} tabs={tabs.filter(tab => !detached || tab.tab_id === query.get("tab"))}
  selected={selected} current={current} address={address} project={project} workspaces={workspaces}
  sites={sites} busy={busy} available={available} starting={starting} unknownCount={unknown.length} projectName={projectName}
  onSelect={selectTab} onStart={startSession} onAddress={setAddress} onProject={setProject}
  onNavigate={() => void navigate()} onAction={name => void action(name)}
  onManageSites={() => setDialog("sites")} onReview={() => setDialog("review")}
  onOpenSite={site => void open(site.url, site)} onExternal={openExternal}
  onMove={() => void action("present", {mode: detached || current?.mode === "detached" ? "embedded" : "detached"})}
  onCloseTab={requestClose} onHistory={()=>setDialog("history")} onTools={()=>setDialog("tools")} onCredentialVault={()=>setDialog("vault")} addressRef={addressRef}/>
 {findOpen&&current&&<BrowserFindBar query={findQuery} result={findResult} error={findError} busy={busy} onQuery={value=>{setFindQuery(value);setFindResult(null);}} onFind={backwards=>void find(backwards)} onClose={()=>{setFindOpen(false);void find(false,"");}}/>}
 {requests.length>0&&<div className="browser-request-banner" role="status"><span>网页有 {requests.length} 项请求等待处理</span><Button size="sm" variant="secondary" onClick={()=>setDialog("requests")}>查看网页请求（{requests.length}）</Button></div>}
 {error&&<div className="browser-error"><ErrorState title="浏览器操作未完成" description={error} action={<Button variant="secondary" size="sm" onClick={()=>{setError("");void refresh();}}>重新检查</Button>}/></div>}
 {current?.error&&dismissedNotice!==noticeKey&&<NativeBrowserNotice message={current.error} onDismiss={()=>setDismissedNotice(noticeKey)}/>}
 <div ref={canvas} className="browser-canvas" data-testid="native-browser-viewport">
 {!available ? <EmptyState title="请在桌面应用中打开浏览器" description="桌面应用提供原生网页显示。你仍可以管理项目站点。" />
   : starting ? <BrowserStartPage busy={busy} sites={sites} sitesLoading={sitesLoading} sitesError={sitesError} address={address} addressRef={addressRef} onAddress={setAddress} temporary={temporary} onTemporary={setTemporary} onNavigate={() => void navigate()} onOpenSite={site => void open(site.url, site)} onManageSites={() => setDialog("sites")} />
   : !current ? <EmptyState title="网页已关闭" description="请回到主窗口继续浏览。" />
   : detached && current.mode === "embedded" ? <EmptyState title="网页已回到主窗口" description="页面及会话保持不变，可在主窗口继续浏览。" />
   : current.mode === "detached" && !detached ? <EmptyState title="网页正在独立窗口显示" description="使用工具栏的“回到主窗口”可移动当前页面并保留会话。" />
   : <p className="browser-native-placeholder">原生网页区域</p>}
 </div>
 {dialog==="review"&&<UnknownBrowserReview operations={detached?unknown.filter(op=>!op.target||op.target.tab_id===current?.tab_id):unknown} tabs={tabs} onClose={()=>setDialog(null)} onReviewed={()=>void refresh()} onInspect={tabId=>{newSession.current=false;setSelected(tabId);setDialog(null);}}/>}
 {sitesOpen&&<SiteManager workspaces={workspaces} workspaceId={project} initialView={dialog==="history"?"history":dialog==="editor"?"editor":"sites"} initialUrl={current?.url||address} initialTitle={current?.title} onClose={()=>setDialog(null)} onOpen={site=>void open(site.url,site)}/>}
 {dialog==="requests"&&<PendingNativeActions requests={requests} onClose={()=>setDialog(null)} onResolved={refresh}/>}
 {dialog==="vault"&&<BrowserCredentialVault origin={current?.url ?? ""} onClose={()=>setDialog(null)}/>}
 {dialog==="tools"&&<BrowserTools key={`${current?.session_id}:${current?.tab_id}`} tab={current} zoom={current?ui.zoom[current.tab_id]||1:1} busy={busy} error={error} onZoom={factor=>void zoom(factor)} onFind={()=>{setDialog(null);setFindOpen(true);}} onBookmark={()=>setDialog("editor")} onClear={clearData} onClose={()=>setDialog(null)}/>}
 {dialog==="close"&&closeTarget&&<Modal open title="结束临时会话" onClose={()=>{setDialog(null);setCloseTarget(null);}}><p className="mb-4 text-sm">关闭“{closeTarget.title||closeTarget.url}”将丢弃此临时会话的登录状态和未提交表单。切换显示模式会保留页面。</p><Button variant="danger" disabled={busy} onClick={()=>void closeTab(closeTarget)}>关闭并丢弃会话</Button></Modal>}
 </section>;
}
