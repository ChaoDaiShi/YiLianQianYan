import {useState} from 'react';
import {Link} from 'react-router-dom';
import type {CapabilityDescriptor} from '../../../api/capabilities';
import {Button,Modal} from '../../../components/ui';
import {capabilityAvailability} from './binding';
export default function CapabilityPicker({open,onClose,items,onSelect,onRefresh,loading,error}:{open:boolean;onClose:()=>void;items:CapabilityDescriptor[];onSelect:(tool:CapabilityDescriptor)=>void;onRefresh:()=>void;loading:boolean;error:string}){
 const [query,setQuery]=useState(''),[readyOnly,setReadyOnly]=useState(true);
 const shown=items.filter(item=>(!readyOnly||item.status==='ready')&&`${item.name} ${item.description} ${item.metadata.source_name||''}`.toLowerCase().includes(query.toLowerCase()));
 return <Modal open={open} onClose={onClose} title="从能力添加"><div className="flex gap-2"><input aria-label="搜索能力" placeholder="搜索工具或来源" value={query} onChange={e=>setQuery(e.target.value)} className="min-w-0 flex-1 rounded border border-[var(--border-soft)] bg-[var(--surface-solid)] p-2"/><Button variant="secondary" size="sm" disabled={loading} onClick={onRefresh}>刷新能力</Button></div>
 <label className="my-3 flex gap-2 text-xs"><input type="checkbox" checked={readyOnly} onChange={e=>setReadyOnly(e.target.checked)}/>仅显示已就绪能力</label>
 {error&&<p role="alert">{error}</p>}{loading&&<p role="status">正在读取能力…</p>}
 {!loading&&!shown.length&&<p className="text-sm">没有可用 MCP 工具。<Link className="ml-2 underline" to="/capabilities">前往能力来源管理</Link></p>}
 <div className="space-y-2">{shown.map(tool=>{const availability=capabilityAvailability(tool);return <article key={tool.id} className="rounded border border-[var(--border-soft)] p-3" data-capability-id={tool.id}>
 <div className="flex items-center justify-between gap-2"><strong className="text-sm">{tool.name}</strong><Button size="sm" disabled={availability.kind!=='configured'} onClick={()=>onSelect(tool)} aria-label={`选择 ${tool.name} · ${tool.metadata.source_name||tool.id}`}>选择</Button></div>
 <p className="mt-1 text-xs">来源：{tool.metadata.source_name||'MCP'} · {tool.status} · {tool.risk==='high'?'高风险':tool.risk}</p><p className="mt-1 text-xs text-[var(--text-secondary)]">{tool.description}</p>
 <p className="mt-1 text-xs">权限：{tool.permissions.map(p=>p.permission).join('、')||'McpInvoke（运行时校验）'}</p>
 {availability.kind==='unavailable'&&<p className="mt-1 text-xs text-[var(--warning-fg)]">{availability.reason}</p>}
 </article>;})}</div></Modal>;
}
