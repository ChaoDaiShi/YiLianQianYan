import { useEffect, useState } from "react";
import { Button } from "../../components/ui";
import { accessPermissions, getAccessRules, putAccessRules, type AccessRule, type AccessMode, type PermissionId } from "../../api/accessRules";
import { AccessScopeDialog } from "./AccessScopeDialog";
const modeLabels={full:"完全权限",approve:"需批准",deny:"禁止"};
export function AccessMatrix({agentId,agentLabel}:{agentId?:string;agentLabel?:string}={}){
 const [rules,setRules]=useState<AccessRule[]>([]);const [loaded,setLoaded]=useState(false);const [busy,setBusy]=useState(false);const [error,setError]=useState("");const [dialog,setDialog]=useState<{rule:AccessRule|null}|null>(null);
 useEffect(()=>{let active=true;void getAccessRules().then(r=>{if(active){if(r.ok){setRules(r.data.rules);setLoaded(true)}else setError(r.error)}});return()=>{active=false}},[]);
 const label=agentLabel??agentId;
 const isTableRule=(rule:AccessRule,permission:PermissionId)=>rule.permission_id===permission&&(agentId?rule.scope==="agent"&&rule.subject_id===agentId:rule.scope==="default");
 const canEdit=(rule:AccessRule)=>!agentId||((rule.scope==="agent"||rule.scope==="resource")&&rule.subject_id===agentId);
 const visibleRules=rules.filter(rule=>agentId?canEdit(rule)||rule.scope==="default"||rule.scope==="capability"||(rule.scope==="resource"&&rule.subject_id===null):rule.scope!=="default");
 async function persist(next:AccessRule[]){setBusy(true);setError("");try{const result=await putAccessRules(next);if(!result.ok)throw new Error(result.error);setRules(result.data.rules)}catch(e){setError(String(e));throw e}finally{setBusy(false)}}
 async function setPermission(permission:PermissionId,mode:AccessMode|""){const next=rules.filter(rule=>!isTableRule(rule,permission));if(mode)next.push({id:agentId?`agent:${agentId}:${permission}`:`default:${permission}`,scope:agentId?"agent":"default",subject_id:agentId??null,permission_id:permission,resource:null,mode});await persist(next)}
 return <div className="grid gap-4">
 {agentId&&<h3 className="font-semibold">{label} 的操作权限</h3>}
 <p className="settings-help">{agentId?"这里保存的操作权限仅作用于当前智能体。未单独设置的操作继承现有策略；全局禁止规则仍然生效。":"完全权限按已保存范围直接执行；需批准等待批准；禁止阻止执行。匹配到禁止规则时优先生效，资源规则比工具、智能体和默认规则更具体。"}</p>
 {error&&<p role="alert" className="text-[var(--danger)]">{error}</p>}
 {!loaded&&!error&&<p role="status">正在读取权限规则…</p>}
 <div className="overflow-x-auto"><table className="w-full text-sm"><thead><tr><th className="p-2 text-left">操作</th><th className="p-2 text-left">{agentId?"该智能体权限":"默认权限"}</th></tr></thead><tbody>{accessPermissions.map(p=><tr key={p.id} className="border-t border-[var(--border-soft)]"><td className="p-2">{p.label}</td><td className="p-2"><select aria-label={agentId?`${label} ${p.label}权限`:`${p.label}默认权限`} className="settings-select" disabled={!loaded||busy} value={rules.find(rule=>isTableRule(rule,p.id))?.mode??""} onChange={e=>void setPermission(p.id,e.target.value as AccessMode|"").catch(()=>{})}><option value="">{agentId?"继承现有策略":"使用现有策略"}</option><option value="full">完全权限</option><option value="approve">需批准</option><option value="deny">禁止</option></select></td></tr>)}</tbody></table></div>
 <div className="flex items-center justify-between gap-3"><h4>{agentId?"该智能体规则与全局约束":"指定范围"}</h4><Button disabled={!loaded||busy} onClick={()=>setDialog({rule:null})}>新增范围规则</Button></div>
 {visibleRules.length===0?<p className="settings-help">还没有指定范围规则。</p>:<div className="grid gap-2">{visibleRules.map(rule=><div key={rule.id} className="flex items-center justify-between gap-3 rounded-lg border border-[var(--border-soft)] p-3"><div><strong>{accessPermissions.find(p=>p.id===rule.permission_id)?.label}</strong><p className="settings-help">{agentId&&!canEdit(rule)?"全局规则":rule.scope==="default"?"默认":rule.scope==="agent"?"智能体":rule.scope==="capability"?"工具":"资源"} · {rule.subject_id===agentId&&agentId?label:rule.subject_id??String(rule.resource?.root??rule.resource?.host??rule.resource?.server_id??"所有操作对象")} · {modeLabels[rule.mode]}</p></div>{canEdit(rule)?<div className="flex gap-2"><Button disabled={busy} onClick={()=>setDialog({rule})}>编辑</Button><Button disabled={busy} onClick={()=>void persist(rules.filter(r=>r.id!==rule.id)).catch(()=>{})}>移除</Button></div>:<span className="text-xs text-[var(--text-muted)]">在全局权限设置中修改</span>}</div>)}</div>}
 {dialog&&<AccessScopeDialog rule={dialog.rule} initialAgentId={agentId} initialAgentLabel={label} onClose={()=>setDialog(null)} onSave={async rule=>{await persist([...rules.filter(r=>r.id!==rule.id),rule]);setDialog(null)}}/>}
 </div>
}
