import {useEffect,useRef,useState} from 'react';
import {Link} from 'react-router-dom';
import {approveAction,rejectAction} from '../../../api/approvals';
import {getCapability} from '../../../api/capabilities';
import {getTaskGraph} from '../../../api/taskWorld';
import type {PendingApproval} from '../../../types/approval';
import ApprovalCard from '../../../components/approval/ApprovalCard';
export default function TaskCanvasApproval({approval,onResolved}:{approval:PendingApproval;onResolved:()=>Promise<void>}){
 const [busy,setBusy]=useState(false),[error,setError]=useState(''),[label,setLabel]=useState(approval.task_node_id||''),[capability,setCapability]=useState('MCP 工具');
 const inFlight=useRef(false);
 useEffect(()=>{
  let active=true;
  void getTaskGraph(approval.task_graph_id!).then(result=>{if(active&&result.ok)setLabel(result.data.nodes.find(n=>n.id===approval.task_node_id)?.title||approval.task_node_id!);});
  void getCapability(approval.tool_name.replace(/^mcp_/,'mcp.')).then(result=>{if(active&&result.ok)setCapability(`${result.data.name} · ${result.data.metadata.source_name||'MCP'}`);});
  return()=>{active=false;};
 },[approval.task_graph_id,approval.task_node_id,approval.tool_name]);
 const decide=async(approve:boolean)=>{
  if(inFlight.current)return;inFlight.current=true;setBusy(true);setError('');
  try{let failure='';await (approve?approveAction:rejectAction)(approval.approval_id,null,event=>{if(event.type==='error')failure=event.error||"审批执行失败";});if(failure)throw new Error(failure);await onResolved();}
  catch(e){setError(e instanceof Error?e.message:'审批失败');}
  finally{inFlight.current=false;setBusy(false);}
 };
 return <section className="mb-4" data-testid="task-canvas-approval" data-approval-id={approval.approval_id}>
  <p className="mb-2 text-sm">来源：任务画布 · {label}</p><p className="mb-2 text-xs">执行能力：{capability}</p>
  <Link className="mb-2 block break-all text-xs underline" to={`/task-world/${encodeURIComponent(approval.task_graph_id!)}`}>查看任务图 {approval.task_graph_id}</Link>
  <ApprovalCard approval={approval} resolving={busy} onApprove={()=>void decide(true)} onReject={()=>void decide(false)}/>
  {error&&<p role="alert" className="mt-2 text-xs text-[var(--danger-fg)]">{error}</p>}
 </section>;
}
