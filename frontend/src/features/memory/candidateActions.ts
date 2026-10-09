import type { EvidenceSource, SkillCandidate } from "../../api/evidence";
export function candidateBelongsToSource(candidate:SkillCandidate,source:EvidenceSource):boolean{
  return candidate.sources.some(item=>item.kind===source.kind&&item.execution_id===source.execution_id&&(item.kind==="task"&&source.kind==="task"?item.task_id===source.task_id:item.kind==="node"&&source.kind==="node"&&item.graph_id===source.graph_id&&item.node_id===source.node_id));
}
export function canConfirmCandidate(candidate:SkillCandidate|null,lesson:string,confirmed:boolean):boolean{return Boolean(candidate&&candidate.status==="validated"&&candidate.lesson===lesson&&confirmed);}
