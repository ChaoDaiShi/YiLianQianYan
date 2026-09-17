import { API_BASE } from "./transport";
import { controlSessionHeaders } from "./controlSession";

export type EvidenceSource = { kind: "task"; task_id: string; execution_id: string } | { kind: "node"; graph_id: string; node_id: string; execution_id: string; workspace_id?: string };
export interface ArtifactRecord { id:string; name:string; task_id:string; task_execution_id:string; mime_type?:string; size?:number; created_at:number; updated_at:number }
export interface ArtifactPreview { artifact:ArtifactRecord; provenance:{source:EvidenceSource;version:number;sha256:string}|null; text:string|null; truncated:boolean }
export interface SkillCandidate { id:string;revision:number;status:"draft"|"validated"|"rejected"|"confirmed";sources:EvidenceSource[];lesson:string;sensitivity:string;skill_name:string|null;skill_version:number|null;created_at:number;updated_at:number }
export interface SkillVersion { skill_name:string;version:number;candidate_id:string;content:string;content_hash:string;active:boolean;created_at:number }

async function request<T>(path:string,method="GET",body?:unknown):Promise<T>{
  const response=await fetch(`${API_BASE}${path}`,{method,headers:{"Content-Type":"application/json",...controlSessionHeaders()},...(body===undefined?{}:{body:JSON.stringify(body)})});
  const value=await response.json().catch(()=>null) as {message?:string}|null;
  if(!response.ok)throw new Error(value?.message||`请求失败 (${response.status})`);
  return value as T;
}
export async function listArtifactResults(source:EvidenceSource):Promise<ArtifactRecord[]>{const query=new URLSearchParams({task_id:source.kind==="task"?source.task_id:source.graph_id,task_execution_id:source.execution_id});return (await request<{artifacts:ArtifactRecord[]}>(`/api/artifacts?${query}`)).artifacts;}
export async function materializeArtifact(source:EvidenceSource,name:string):Promise<void>{await request("/api/artifacts/from-execution","POST",{source,name});}
export const previewArtifact=(id:string)=>request<ArtifactPreview>(`/api/artifacts/${encodeURIComponent(id)}/preview`);
export async function downloadArtifact(id:string):Promise<Blob>{const response=await fetch(`${API_BASE}/api/artifacts/${encodeURIComponent(id)}/download`,{headers:controlSessionHeaders()});if(!response.ok){const error=await response.json().catch(()=>null) as {message?:string}|null;throw new Error(error?.message||"产物下载失败，历史记录仍保留");}return response.blob();}
export async function listSkillCandidates():Promise<SkillCandidate[]>{return (await request<{candidates:SkillCandidate[]}>("/api/skill-candidates")).candidates;}
export async function createSkillCandidate(sources:EvidenceSource[],lesson:string,authorized:boolean):Promise<SkillCandidate>{return (await request<{candidate:SkillCandidate}>("/api/skill-candidates","POST",{sources,lesson,authorized})).candidate;}
export async function editSkillCandidate(candidate:SkillCandidate,lesson:string):Promise<SkillCandidate>{return (await request<{candidate:SkillCandidate}>(`/api/skill-candidates/${encodeURIComponent(candidate.id)}`,"PUT",{expected_revision:candidate.revision,lesson})).candidate;}
export async function changeSkillCandidate(candidate:SkillCandidate,action:"validate"|"reject"):Promise<SkillCandidate>{return (await request<{candidate:SkillCandidate}>(`/api/skill-candidates/${encodeURIComponent(candidate.id)}/${action}`,"POST",{expected_revision:candidate.revision})).candidate;}
export async function confirmSkillCandidate(candidate:SkillCandidate,name:string,confirmed:boolean):Promise<SkillVersion>{return (await request<{version:SkillVersion}>(`/api/skill-candidates/${encodeURIComponent(candidate.id)}/confirm`,"POST",{expected_revision:candidate.revision,name,confirmed})).version;}
export async function listSkillVersions(name:string):Promise<SkillVersion[]>{return (await request<{versions:SkillVersion[]}>(`/api/managed-skill-versions/${encodeURIComponent(name)}`)).versions;}
export async function rollbackSkillVersion(name:string,version:number):Promise<void>{await request(`/api/managed-skill-versions/${encodeURIComponent(name)}/rollback`,"POST",{version,confirmed:true});}
export async function deactivateSkill(name:string):Promise<void>{await request(`/api/managed-skill-versions/${encodeURIComponent(name)}/deactivate`,"POST",{confirmed:true});}
