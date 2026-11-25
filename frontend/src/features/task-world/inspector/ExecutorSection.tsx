import {useEffect,useState} from 'react';
import { Input,Button,ErrorState,Modal } from '../../../components/ui';
import { getHarnesses, type HarnessSnapshot } from '../../../api/harnesses';
import type { ExecutorAvailability } from '../taskGraphProjection';
import CapabilityPicker from '../capability-picker/CapabilityPicker';
import type {useCanvasCapabilities} from '../capability-picker/useCanvasCapabilities';
import type {CapabilityDescriptor} from '../../../api/capabilities';
export default function ExecutorSection({executorRef,onExecutorRefChange,availability,capabilities,onSelect}:{executorRef:string;onExecutorRefChange:(value:string)=>void;availability:ExecutorAvailability;capabilities?:ReturnType<typeof useCanvasCapabilities>;onSelect?:(tool:CapabilityDescriptor)=>void}){
 const [open,setOpen]=useState(false);
 const [workflowPicker,setWorkflowPicker]=useState(false);
 const tool=capabilities?.items.find(c=>`capability://${c.id}`===executorRef);
 const isCapability=executorRef.startsWith('capability://');
 const reference=<Input label="执行引用" value={executorRef} onChange={e=>onExecutorRefChange(e.target.value)} placeholder="例如 workflow://… 或 command://…"/>;
 return <>
  <div className="flex flex-wrap items-center justify-between gap-2">
   <span className="text-sm font-medium">执行能力</span>
   <div className="flex flex-wrap gap-2">
    <Button type="button" variant="secondary" size="sm" onClick={()=>setWorkflowPicker(true)}>选择工作流</Button>
    {capabilities&&<Button type="button" variant="secondary" size="sm" onClick={()=>{setOpen(true);void capabilities.refresh(true);}}>选择能力</Button>}
   </div>
  </div>
  {capabilities&&<CapabilityPicker open={open} onClose={()=>setOpen(false)} items={capabilities.items} onSelect={tool=>{onSelect?.(tool);setOpen(false);}} onRefresh={()=>void capabilities.refresh(true)} loading={capabilities.loading} error={capabilities.error}/>}
  {workflowPicker&&<WorkflowPicker onClose={()=>setWorkflowPicker(false)} onSelect={id=>{onExecutorRefChange(`workflow://${id}`);setWorkflowPicker(false);}}/>}
  {isCapability?<details className="studio-detail"><summary>高级详情</summary>{reference}</details>:reference}
  <div className="rounded border border-[var(--border-soft)] bg-[var(--surface-muted)] p-3 text-xs">
   {tool&&<><strong>{tool.name}</strong><p className="mt-1">来源：{tool.metadata.source_name||'MCP'} · 高风险</p><p className="mt-1">{tool.description}</p></>}
   <p className="mt-1">{availability.label}</p><p className="mt-1 break-words text-[var(--text-faint)]">{availability.reason}</p>
  </div>
 </>;
}

function WorkflowPicker({ onSelect, onClose }: { onSelect: (id: string) => void; onClose: () => void }) {
 const [snapshot, setSnapshot] = useState<HarnessSnapshot | null>(null);
 const [error, setError] = useState("");
 const [attempt, setAttempt] = useState(0);
 const [query, setQuery] = useState("");
 useEffect(() => {
  let active = true;
  void getHarnesses().then(result => {
   if (!active) return;
   if (result.ok) setSnapshot(result.data);
   else setError(result.error);
  });
  return () => { active = false; };
 }, [attempt]);

 const activeProfile = snapshot?.settings.profiles.find(profile => profile.id === snapshot.settings.active_id);
 const workflows = snapshot?.catalog.workflows.filter(workflow => `${workflow.name} ${workflow.description}`.toLowerCase().includes(query.trim().toLowerCase())) ?? [];
 return <Modal open title="选择节点工作流" size="md" onClose={onClose}>
  <div className="min-w-0 space-y-3">
   <Input label="搜索工作流" value={query} onChange={event => setQuery(event.target.value)} />
   {error ? <ErrorState title="无法读取运行工作流" description={error} onRetry={() => { setError(""); setAttempt(value => value + 1); }} /> :
     !snapshot ? <p role="status">正在读取工作流…</p> : !workflows.length ?
       <p className="text-sm text-[var(--text-secondary)]">{query.trim() ? "没有匹配的工作流。" : "请在能力页面创建运行工作流。"}</p> :
       <ul className="divide-y divide-[var(--border-soft)]">
        {workflows.map(workflow => {
          const available = workflow.ready && (!activeProfile || (activeProfile.workflow_ids.includes(workflow.id) &&
            (!workflow.plugin_id || activeProfile.plugin_ids.includes(workflow.plugin_id))));
          return <li key={workflow.id} className="flex flex-wrap items-start justify-between gap-2 py-3">
            <div className="min-w-0 flex-1 break-words">
              <h4 className="font-medium">{workflow.name}</h4>
              {workflow.description && <p className="text-xs text-[var(--text-secondary)]">{workflow.description}</p>}
              {!available && <p className="text-xs text-[var(--warning-fg)]">{workflow.ready ? "不在当前 Harness 范围内" : "工作流定义或来源不可用"}</p>}
            </div>
            <Button type="button" size="sm" variant="secondary" aria-label={`选择 ${workflow.name}`} disabled={!available} onClick={() => onSelect(workflow.id)}>选择</Button>
          </li>;
        })}
       </ul>}
  </div>
 </Modal>;
}
