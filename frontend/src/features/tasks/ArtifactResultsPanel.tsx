import {useCallback,useEffect,useRef,useState} from "react";
import {listWorkspaces,type Workspace} from "../../api/client";
import {downloadArtifact,listArtifactResults,materializeArtifact,previewArtifact,type ArtifactPreview,type ArtifactRecord,type EvidenceSource} from "../../api/evidence";

export function ArtifactResultCard({artifact}:{artifact:ArtifactRecord}){
  const[preview,setPreview]=useState<ArtifactPreview|null>(null);const[error,setError]=useState("");const[busy,setBusy]=useState(false);
  const inspect=async()=>{setError("");setBusy(true);try{setPreview(await previewArtifact(artifact.id));}catch(error){setError(error instanceof Error?error.message:"无法预览，历史记录仍保留");}finally{setBusy(false);}};
  const download=async()=>{setError("");setBusy(true);try{const bytes=await downloadArtifact(artifact.id);const url=URL.createObjectURL(bytes);const link=document.createElement("a");link.href=url;link.download=artifact.name.includes(".")?artifact.name:`${artifact.name}.md`;link.click();window.setTimeout(()=>URL.revokeObjectURL(url),1000);}catch(error){setError(error instanceof Error?error.message:"无法下载，历史记录仍保留");}finally{setBusy(false);}};
  return <article className="space-y-1 rounded border border-[var(--border)] p-2 text-xs"><strong>{artifact.name}</strong><p>输出产物 · {artifact.mime_type||"MIME 未记录"} · {artifact.size===undefined?"大小未记录":`${artifact.size} 字节`} · {new Date(artifact.created_at).toLocaleString()}</p>
    <p>来源执行：{artifact.task_execution_id}</p><div className="flex gap-2"><button type="button" disabled={busy} onClick={()=>void inspect()}>预览与来源</button><button type="button" disabled={busy} onClick={()=>void download()}>下载真实文件</button></div>
    {error?<p role="alert" className="text-[var(--danger)]">{error}</p>:null}{preview?<><p>{preview.provenance?`版本 v${preview.provenance.version}`:"历史版本号未记录"}</p><details><summary>溯源信息</summary><pre className="whitespace-pre-wrap break-all">{JSON.stringify(preview.provenance?.source||{task_id:artifact.task_id,execution_id:artifact.task_execution_id},null,2)}</pre></details>{preview.text!==null?<pre className="max-h-64 overflow-auto whitespace-pre-wrap">{preview.text}</pre>:<p>此类型不提供文本预览，可下载原文件。</p>}{preview.truncated?<p>预览已截断，文件仍完整保存。</p>:null}</>:null}
  </article>;
}

export default function ArtifactResultsPanel({source,completed=false}:{source?:EvidenceSource;completed?:boolean}){
  const key=JSON.stringify(source);const latest=useRef(key);latest.current=key;
  const[artifacts,setArtifacts]=useState<ArtifactRecord[]>([]);const[workspaces,setWorkspaces]=useState<Workspace[]>([]);const[workspaceId,setWorkspaceId]=useState("");const[error,setError]=useState("");const[busy,setBusy]=useState(false);
  const reload=useCallback(async()=>{if(!source)return;try{const values=await listArtifactResults(source);if(latest.current===key)setArtifacts(values);}catch(error){if(latest.current===key)setError(error instanceof Error?error.message:"产物列表不可用");}},[key]);
  useEffect(()=>{setArtifacts([]);setError("");setBusy(false);setWorkspaceId(source?.kind==="node"?source.workspace_id||"":"");void reload();void listWorkspaces().then(result=>{if(latest.current===key&&result.ok)setWorkspaces(result.data);});},[reload]);
  if(!source)return <section className="p-3 text-xs">选择执行记录后可查看输出产物。</section>;
  const generate=async()=>{setBusy(true);setError("");try{await materializeArtifact(source.kind==="node"?{...source,workspace_id:workspaceId}:source,"执行结果");if(latest.current===key)await reload();}catch(error){if(latest.current===key)setError(error instanceof Error?error.message:"产物保存失败");}finally{if(latest.current===key)setBusy(false);}};
  return <section aria-label="真实输出产物" className="space-y-2 rounded border border-[var(--border)] p-3 text-xs"><h3 className="font-medium">输出产物</h3><p>仅从完成且验证通过的执行记录生成真实文件；输入附件与产物分别保存。</p>
    {source.kind==="node"?<select aria-label="产物所属工作空间" value={workspaceId} onChange={event=>setWorkspaceId(event.target.value)} className="w-full bg-[var(--surface)]"><option value="">选择实际工作空间</option>{workspaces.map(workspace=><option value={workspace.id} key={workspace.id}>{workspace.name}</option>)}</select>:null}
    <button type="button" disabled={!completed||busy||(source.kind==="node"&&!workspaceId)} onClick={()=>void generate()}>生成并保存 Markdown 产物</button>
    {error?<p role="alert" className="text-[var(--danger)]">{error}</p>:null}{!artifacts.length?<p>尚无已保存产物</p>:artifacts.map(artifact=><ArtifactResultCard key={artifact.id} artifact={artifact}/>)}
  </section>;
}
