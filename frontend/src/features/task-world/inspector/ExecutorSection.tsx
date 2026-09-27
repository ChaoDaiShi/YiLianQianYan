import {useState} from 'react';
import { Input,Button } from '../../../components/ui';
import type { ExecutorAvailability } from '../taskGraphProjection';
import CapabilityPicker from '../capability-picker/CapabilityPicker';
import type {useCanvasCapabilities} from '../capability-picker/useCanvasCapabilities';
import type {CapabilityDescriptor} from '../../../api/capabilities';
export default function ExecutorSection({executorRef,onExecutorRefChange,availability,capabilities,onSelect}:{executorRef:string;onExecutorRefChange:(value:string)=>void;availability:ExecutorAvailability;capabilities?:ReturnType<typeof useCanvasCapabilities>;onSelect?:(tool:CapabilityDescriptor)=>void}){
 const [open,setOpen]=useState(false),tool=capabilities?.items.find(c=>`capability://${c.id}`===executorRef),isCapability=executorRef.startsWith('capability://');
 const reference=<Input label="执行引用" value={executorRef} onChange={e=>onExecutorRefChange(e.target.value)} placeholder="例如 workflow://… 或 command://…"/>;
 return <>
  {capabilities&&<><div className="flex items-center justify-between"><span className="text-sm font-medium">执行能力</span><Button type="button" variant="secondary" size="sm" onClick={()=>{setOpen(true);void capabilities.refresh(true);}}>选择能力</Button></div><CapabilityPicker open={open} onClose={()=>setOpen(false)} items={capabilities.items} onSelect={tool=>{onSelect?.(tool);setOpen(false);}} onRefresh={()=>void capabilities.refresh(true)} loading={capabilities.loading} error={capabilities.error}/></>}
  {isCapability?<details className="studio-detail"><summary>高级详情</summary>{reference}</details>:reference}
  <div className="rounded border border-[var(--border-soft)] bg-[var(--surface-muted)] p-3 text-xs">
   {tool&&<><strong>{tool.name}</strong><p className="mt-1">来源：{tool.metadata.source_name||'MCP'} · 高风险</p><p className="mt-1">{tool.description}</p></>}
   <p className="mt-1">{availability.label}</p><p className="mt-1 break-words text-[var(--text-faint)]">{availability.reason}</p>
  </div>
 </>;
}
