import { useEffect, useState } from "react";
import { Button, Modal } from "../../components/ui";
import { accessPermissions, type AccessRule, type AccessMode, type PermissionId } from "../../api/accessRules";
import { requestResult } from "../../api/transport";
interface Resource {kind:string;entity_id:string;label:string}
export function AccessScopeDialog({rule,onClose,onSave,initialAgentId,initialAgentLabel}:{rule:AccessRule|null;onClose:()=>void;onSave:(rule:AccessRule)=>Promise<void>;initialAgentId?:string;initialAgentLabel?:string}){
 const [draft,setDraft]=useState<AccessRule>(()=>rule??{id:crypto.randomUUID(),scope:"agent",subject_id:initialAgentId??null,permission_id:"filesystem.read",resource:null,mode:"approve"});
 const [resources,setResources]=useState<Resource[]>([]);const [path,setPath]=useState(String(rule?.resource?.root??""));const [host,setHost]=useState(String(rule?.resource?.host??""));const [serverId,setServerId]=useState(String(rule?.resource?.server_id??""));const [tool,setTool]=useState(String(rule?.resource?.tool_name??""));const [busy,setBusy]=useState(false);const [error,setError]=useState("");
 useEffect(()=>{let active=true;void requestResult<{resources:Resource[]}>("GET","/api/connectors/resources").then(r=>{if(active){if(r.ok)setResources(r.data.resources);else setError(r.error)}});return()=>{active=false}},[]);
 const change=(update:Partial<AccessRule>)=>setDraft(v=>({...v,...update}));
 async function save(){
  let resource:Record<string,unknown>|null=null;
  if(draft.scope==="resource"){
   if(draft.permission_id.startsWith("filesystem.")){if(!path.trim()){setError("请选择文件或目录范围");return}resource={type:"filesystem",root:path.trim(),recursive:true}}
   else if(draft.permission_id==="network.request"){if(!host.trim()){setError("请输入主机名");return}resource={type:"network",scheme:"https",host:host.trim(),methods:["GET","HEAD","OPTIONS","POST","PUT","PATCH","DELETE"],zone:"public"}}
   else if(draft.permission_id==="mcp.invoke"){if(!serverId){setError("请选择 MCP 服务");return}resource={type:"mcp",server_id:serverId,tool_name:tool.trim()||null}}
   else if(draft.permission_id==="shell.execute")resource={type:"shell",host_escape_acknowledged:false};
   else if(draft.permission_id==="process.control")resource={type:"process",scope:{kind:"managed_children"}};
   else {setError("该操作支持默认、智能体或工具范围");return}
  }
  if((draft.scope==="agent"||draft.scope==="capability")&&!draft.subject_id){setError("请选择关联对象");return}
  setBusy(true);setError("");try{await onSave({...draft,resource,subject_id:initialAgentId??(draft.scope==="resource"?null:draft.subject_id)})}catch(e){setError(String(e))}finally{setBusy(false)}
 }
 return <Modal open title={initialAgentId?`${initialAgentLabel??initialAgentId} 的权限范围`:"权限作用范围"} onClose={onClose} size="md" footer={<><Button onClick={onClose} disabled={busy}>取消</Button><Button variant="primary" onClick={()=>void save()} loading={busy}>保存规则</Button></>}>
 <div className="grid gap-4">
 <label>操作<select className="settings-select" value={draft.permission_id} onChange={e=>change({permission_id:e.target.value as PermissionId})}>{accessPermissions.map(p=><option key={p.id} value={p.id}>{p.label}</option>)}</select></label>
 <label>权限<select className="settings-select" value={draft.mode} onChange={e=>change({mode:e.target.value as AccessMode})}><option value="full">完全权限</option><option value="approve">需批准</option><option value="deny">禁止</option></select></label>
 <label>作用范围<select className="settings-select" value={draft.scope} onChange={e=>change({scope:e.target.value as AccessRule["scope"],subject_id:initialAgentId??null})}><option value="agent">指定智能体</option>{!initialAgentId&&<option value="capability">指定工具</option>}<option value="resource">指定资源</option></select></label>
 {initialAgentId&&<p className="settings-help">作用于智能体：{initialAgentLabel??initialAgentId}</p>}
 {!initialAgentId&&(draft.scope==="agent"||draft.scope==="capability")&&<label>关联对象<select className="settings-select" value={draft.subject_id??""} onChange={e=>change({subject_id:e.target.value||null})}><option value="">请选择</option>{resources.filter(r=>r.kind===(draft.scope==="agent"?"agent":"tool")).map(r=><option key={r.entity_id} value={r.entity_id}>{r.label}</option>)}</select></label>}
 {draft.scope==="resource"&&draft.permission_id.startsWith("filesystem.")&&<label>文件或目录<input className="settings-text-input" value={path} onChange={e=>setPath(e.target.value)} placeholder="F:\\项目资料"/><span className="settings-help">对该目录及其子目录生效。</span></label>}
 {draft.scope==="resource"&&draft.permission_id==="network.request"&&<label>HTTPS 主机<input className="settings-text-input" value={host} onChange={e=>setHost(e.target.value)} placeholder="api.example.com"/></label>}
 {draft.scope==="resource"&&draft.permission_id==="mcp.invoke"&&<><label>MCP 服务<select className="settings-select" value={serverId} onChange={e=>setServerId(e.target.value)}><option value="">请选择</option>{resources.filter(r=>r.kind==="mcp").map(r=><option key={r.entity_id} value={r.entity_id}>{r.label}</option>)}</select></label><label>工具名称<input className="settings-text-input" value={tool} onChange={e=>setTool(e.target.value)} placeholder="留空应用到该服务全部工具"/></label></>}
 {draft.scope==="resource"&&draft.permission_id==="shell.execute"&&<p>命令仅在现有工作区约束内运行。</p>}
 {draft.scope==="resource"&&draft.permission_id==="process.control"&&<p>仅控制由本应用启动并托管的子进程。</p>}
 {error&&<p role="alert" className="text-[var(--danger)]">{error}</p>}
 </div></Modal>
}
