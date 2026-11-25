import {useCallback,useEffect,useMemo,useRef,useState} from "react";
import {ChevronDown} from "lucide-react";
import {Link} from "react-router-dom";
import PageShell from "../../../components/layout/PageShell";
import {ErrorState,Skeleton} from "../../../components/ui";
import {bindConnectorResource,getConnectorView,listConnectorCanvases,listConnectorResources,saveConnectorDisplay,saveConnectorLayout,unbindConnectorResource,type ConnectorCanvas as CanvasRecord,type ConnectorNode,type ConnectorResource,type ConnectorView,type ConnectorViewport} from "../../../api/connectors";
import {getSettingsCanvas,putSettingsCanvas,type SettingsCanvasData,type SettingsCanvasState} from "../../../api/settingsCanvas";
import ConnectorCanvas from "../../connectors/ConnectorCanvas";
import ConnectorNodeEditor from "../../connectors/ConnectorNodeEditor";
import {bindingForResource} from "../../connectors/connectorActions";
import SettingsEditorDialog from "../editor/SettingsEditorDialog";
import SettingsNodeManager from "./SettingsNodeManager";
import PluginSettingsDialog from "./PluginSettingsDialog";
import {useSettingsSection} from "../model/useSettingsSection";
import {composeSettingsGraph,mergeSettingsGroupLayout,releaseAutomaticBranchPositions,settingsGroups,settingsFocusNeighborhood,SETTING_PREFIX,SETTING_GROUP_PREFIX} from "./settingsGraph";
import "./settingsCanvas.css";
const menuItemClass="settings-canvas-menu-item focus-ring-token";
export default function SettingsCanvasPage(){
 const[focusRequest,setFocusRequest]=useState<{revision:number;nodeIds:string[]}>();const focusSequence=useRef(0);
 const[focusChoice,setFocusChoice]=useState<string|null>(null);
 const[expandedGroups,setExpandedGroups]=useState<string[]>([]);
 const moreActions=useRef<HTMLDetailsElement>(null);
 const moreTrigger=useRef<HTMLElement>(null);
 const{section:editing,setSection}=useSettingsSection();const[scope,setScope]=useState("");const[canvases,setCanvases]=useState<CanvasRecord[]>([]);const[resources,setResources]=useState<ConnectorResource[]>([]);const[data,setData]=useState<SettingsCanvasData|null>(null);const[view,setView]=useState<ConnectorView|null>(null);const[managerOpen,setManagerOpen]=useState(false);const[resourceEditing,setResourceEditing]=useState<ConnectorNode|null>(null);const[loading,setLoading]=useState(true);const[busy,setBusy]=useState(false);const[error,setError]=useState("");
 const scopeRef=useRef(scope);scopeRef.current=scope;const dataRef=useRef(data);dataRef.current=data;const serial=useRef(Promise.resolve());const requestId=useRef(0);
 const load=useCallback(async(id:string)=>{if(scopeRef.current!==id)return;const request=++requestId.current;setLoading(true);setError("");try{await serial.current;const[state,resourceView]=await Promise.all([getSettingsCanvas(id),id?getConnectorView(id):Promise.resolve(null)]);if(request===requestId.current&&scopeRef.current===id){setData({...state,state:releaseAutomaticBranchPositions(state.state,state.presets,resourceView)});setView(resourceView);}}catch(cause){if(request===requestId.current&&scopeRef.current===id)setError(String(cause));}finally{if(request===requestId.current&&scopeRef.current===id)setLoading(false);}},[]);
 useEffect(()=>{let current=true;void Promise.all([listConnectorCanvases(),listConnectorResources()]).then(([list,catalog])=>{if(current){setCanvases(list.canvases);setResources(catalog.resources);}}).catch(cause=>setError(String(cause)));return()=>{current=false;};},[]);
 useEffect(()=>{void load(scope);},[scope,load]);
 const graph=useMemo(()=>data?composeSettingsGraph(view,{...data.state,expanded_group_ids:expandedGroups},data.presets):null,[view,data,expandedGroups]);
 const groups=useMemo(()=>settingsGroups(data?.presets??[]),[data?.presets]);
 const openBranches=expandedGroups.flatMap(id=>{
  const group=groups.find(item=>item.id===id);
  return group&&graph?.nodes.some(node=>node.id===SETTING_GROUP_PREFIX+id)?[group]:[];
 });
 const activeBranch=openBranches.find(group=>group.id===focusChoice)?.id??"";
 const focusedNodeIds=graph&&activeBranch?settingsFocusNeighborhood(graph,activeBranch):undefined;
 const persist=(next:SettingsCanvasState)=>{const id=scope;if(scopeRef.current!==id)return;setData(current=>current?{...current,state:next}:current);serial.current=serial.current.then(async()=>{try{await putSettingsCanvas(id,next);}catch(cause){if(scopeRef.current===id)setError(`设置画布未保存：${String(cause)}`);}});};
 const refresh=async()=>{await load(scope);const catalog=await listConnectorResources();setResources(catalog.resources);};
 function switchScope(id:string){
  scopeRef.current=id;
  setScope(id);
  setLoading(true);
  setData(null);
  setView(null);
  setExpandedGroups([]);
  setFocusChoice(null);
  setFocusRequest(undefined);
  setManagerOpen(false);
  setResourceEditing(null);
 }
 function openSetting(id:string){setResourceEditing(null);setSection(id);}
 async function refreshResources(){try{const catalog=await listConnectorResources();setResources(catalog.resources);if(scopeRef.current){const current=await getConnectorView(scopeRef.current);if(current.canvas_id===scopeRef.current)setView(current);}}catch(cause){setError(String(cause));}}
 function closeSetting(){setSection(null);void refreshResources();}
 function closeMoreActions(){moreTrigger.current?.focus();if(moreActions.current)moreActions.current.open=false;}
 function showAllSettings(){if(data)persist({...data.state,connected_node_ids:[...new Set([...data.state.connected_node_ids,...data.presets.filter(preset=>preset.group!=="plugins").map(preset=>preset.id)])]});}
 function fitSettings(nextGraph:ConnectorView,branch=""){setFocusRequest({revision:++focusSequence.current,nodeIds:branch?settingsFocusNeighborhood(nextGraph,branch):nextGraph.nodes.map(node=>node.id)});}
 function returnToOverview(){setExpandedGroups([]);setFocusChoice(null);setManagerOpen(false);if(data)fitSettings(composeSettingsGraph(view,{...data.state,expanded_group_ids:[]},data.presets));}
 function arrangeLayout(){const latest=dataRef.current;if(!latest)return;const next={...latest.state,nodes:[]};persist(next);setFocusChoice(null);fitSettings(composeSettingsGraph(view,{...next,expanded_group_ids:expandedGroups},latest.presets));}
 async function bind(resource:ConnectorResource){if(!scope||!view)return;setBusy(true);setError("");try{await bindConnectorResource(scope,bindingForResource(view.agent_id,resource));if(view.display.hidden_node_ids.includes(resource.id))await saveConnectorDisplay(scope,{...view.display,hidden_node_ids:view.display.hidden_node_ids.filter(id=>id!==resource.id)});await refresh();}catch(cause){setError(String(cause));}finally{setBusy(false);}}
 async function unbind(resource:ConnectorResource){if(!scope||!view)return;setBusy(true);setError("");try{for(const binding of view.bindings.filter(b=>b.source_id===resource.id||b.target_id===resource.id))await unbindConnectorResource(scope,binding.id);await refresh();}catch(cause){setError(String(cause));}finally{setBusy(false);}}
 function editResource(node:ConnectorNode){
  if(node.id==="agent:settings-global"){setManagerOpen(true);return;}
  if(node.id.startsWith(SETTING_GROUP_PREFIX)&&data){
   const id=node.id.slice(SETTING_GROUP_PREFIX.length);
   const next=expandedGroups.includes(id)?expandedGroups.filter(key=>key!==id):[...expandedGroups,id];
   setExpandedGroups(next);
   setFocusChoice(null);
   fitSettings(composeSettingsGraph(view,{...data.state,expanded_group_ids:next},data.presets));
   return;
  }
  if(node.id.startsWith(SETTING_PREFIX)){openSetting(node.entity_id);return;}
  setResourceEditing(node);
 }
 function save(positions:Array<{id:string;x:number;y:number}>,_viewport:ConnectorViewport){const latest=dataRef.current;if(!latest||!graph||!positions.length)return;const moved=new Map(positions.map(point=>[point.id,point]));persist(mergeSettingsGroupLayout(latest.state,graph,positions,latest.state.viewport));if(scope&&view&&view.nodes.some(node=>moved.has(node.id))){const id=scope;const points=view.nodes.map(node=>({id:node.id,x:moved.get(node.id)?.x??node.x,y:moved.get(node.id)?.y??node.y}));setView(current=>current?.canvas_id===id?{...current,nodes:current.nodes.map(node=>({...node,...moved.get(node.id)}))}:current);serial.current=serial.current.then(async()=>{try{await saveConnectorLayout(id,points,view.viewport);}catch(cause){if(scopeRef.current===id)setError(String(cause));}});}}
 function hide(ids:string[]){if(!data)return;const settings=[...groups.filter(group=>ids.includes(SETTING_GROUP_PREFIX+group.id)).flatMap(group=>group.presets.map(preset=>preset.id)),...ids.filter(id=>id.startsWith(SETTING_PREFIX)).map(id=>id.slice(SETTING_PREFIX.length))];if(settings.length)persist({...data.state,connected_node_ids:data.state.connected_node_ids.filter(id=>!settings.includes(id))});const actual=ids.filter(id=>!id.startsWith(SETTING_PREFIX)&&!id.startsWith(SETTING_GROUP_PREFIX));if(actual.length&&scope&&view){const display={...view.display,hidden_node_ids:[...new Set([...view.display.hidden_node_ids,...actual])]};const id=scope;void saveConnectorDisplay(id,display).then(saved=>setView(current=>current?.canvas_id===id?{...current,display:saved}:current)).catch(cause=>setError(String(cause)));}}

 return <PageShell title="设置" description="从画布打开的系统设置作用于全局。" variant="workspace" className="settings-canvas-page page-canvas" contentClassName="p-0 min-h-0 overflow-hidden" actions={<>
  {canvases.length>0
   ? <label className="flex min-w-0 items-center gap-2 text-xs text-[var(--text-secondary)]">
     <span className="shrink-0">画布视图</span>
     <select className="settings-scope-label focus-ring-token" value={scope} disabled={Boolean(editing||resourceEditing)} onChange={event=>switchScope(event.target.value)}><option value="">全局画布</option>{canvases.map(canvas=><option key={canvas.id} value={canvas.id}>{canvas.name}</option>)}</select>
    </label>
   : <span className="settings-scope-label">全局画布</span>}
  <details className="settings-canvas-actions" ref={moreActions} onBlur={event=>{
   if(!event.currentTarget.contains(event.relatedTarget))event.currentTarget.open=false;
  }} onKeyDown={event=>{
   if(event.key==="Escape"&&event.currentTarget.open){event.preventDefault();closeMoreActions();}
  }}>
   <summary ref={moreTrigger} className="focus-ring-token">更多操作<ChevronDown size={14} aria-hidden="true"/></summary>
   <div className="settings-canvas-action-list">
    <Link className={menuItemClass} to="/settings" onClick={closeMoreActions}>返回全局设置</Link>
    <button type="button" className={menuItemClass} onClick={()=>{closeMoreActions();openSetting("model-pools");}}>高级模型池</button>
    <button type="button" className={menuItemClass} disabled={!graph||loading} onClick={()=>{closeMoreActions();returnToOverview();}}>返回收拢视图</button>
    <button type="button" className={menuItemClass} disabled={!data||loading} onClick={()=>{closeMoreActions();showAllSettings();}}>显示全部系统设置</button>
    <button type="button" className={menuItemClass} disabled={!data||loading||busy} title="重排节点位置，保留连接和展开状态" onClick={()=>{closeMoreActions();arrangeLayout();}}>整理布局</button>
    <button type="button" className={menuItemClass} onClick={()=>{closeMoreActions();void refresh();}}>刷新</button>
   </div>
  </details>
 </>}>
  {error&&<div className="settings-canvas-error"><ErrorState title="设置操作未完成" description={error}/></div>}
  <div className="settings-canvas-workspace"><div className="settings-canvas-stage">{loading?<Skeleton className="h-full w-full"/>:graph&&<ConnectorCanvas settingsLayout key={scope||"global"} layoutKey={`${managerOpen?"expanded":"collapsed"}:${data?.state.connected_node_ids.join(",")}:${expandedGroups.join(",")}`} view={graph} focusRequest={focusRequest} focusedNodeIds={focusedNodeIds} onEdit={editResource} onSave={save} onHide={hide} onUnbind={ids=>{if(view)for(const id of ids){const binding=view.bindings.find(b=>b.id===id);const resource=resources.find(r=>r.kind!=="agent"&&(r.id===binding?.source_id||r.id===binding?.target_id));if(resource&&resource.kind!=="agent")void unbind(resource);}}} onConnect={connection=>{const resource=resources.find(item=>item.id===connection.source&&item.kind==="conversation"||item.id===connection.target&&item.kind!=="agent");if(resource)void bind(resource);}}/>}
   {graph&&openBranches.length>0&&<div className="settings-branch-focus"><label>聚焦分支<select className="focus-ring-token" aria-label="查看已展开分支" value={activeBranch} onChange={event=>{const branch=event.target.value;setFocusChoice(branch);fitSettings(graph,branch);}}><option value="">全图</option>{openBranches.map(group=><option key={group.id} value={group.id}>{group.label}</option>)}</select></label><button type="button" className="focus-ring-token" onClick={returnToOverview}>收起分支</button></div>}
  </div>
   <SettingsNodeManager open={managerOpen} onToggle={()=>setManagerOpen(value=>!value)} presets={data?.presets??[]} connected={data?.state.connected_node_ids??[]} busy={busy||loading} onSettingConnect={id=>{if(data)persist({...data.state,connected_node_ids:[...new Set([...data.state.connected_node_ids,id])]});}} onSettingDisconnect={id=>{if(data)persist({...data.state,connected_node_ids:data.state.connected_node_ids.filter(key=>key!==id)});}} onSettingEdit={openSetting}/>
  </div>
  {editing?.startsWith("plugin:")?<PluginSettingsDialog key={editing} nodeId={editing} onClose={closeSetting}/>:editing&&<SettingsEditorDialog key={editing} nodeKey={editing} onClose={closeSetting} onSaved={()=>void refreshResources()}/>}
  {resourceEditing&&<ConnectorNodeEditor node={resourceEditing} resources={resources} onClose={()=>setResourceEditing(null)} onSaved={()=>void refresh()} onBind={bind} bindingActions={null}/>}
 </PageShell>;
}
