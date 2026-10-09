import { useCallback,useEffect,useRef,useState } from "react";
import { changeSkillCandidate,confirmSkillCandidate,createSkillCandidate,deactivateSkill,editSkillCandidate,listSkillCandidates,listSkillVersions,rollbackSkillVersion,type EvidenceSource,type SkillCandidate,type SkillVersion } from "../../api/evidence";
import { candidateBelongsToSource,canConfirmCandidate } from "./candidateActions";

export default function MemorySkillCandidatePanel({source,completed=false}:{source?:EvidenceSource;completed?:boolean}){
  const key=JSON.stringify(source);const latest=useRef(key);latest.current=key;
  const[candidates,setCandidates]=useState<SkillCandidate[]>([]);const[selectedId,setSelectedId]=useState("");
  const[lesson,setLesson]=useState("");const[authorized,setAuthorized]=useState(false);const[confirmed,setConfirmed]=useState(false);
  const[name,setName]=useState("");const[versions,setVersions]=useState<SkillVersion[]>([]);const[error,setError]=useState("");const[busy,setBusy]=useState(false);
  const latestName=useRef(name);latestName.current=name;
  const selected=candidates.find(item=>item.id===selectedId)||null;
  const reload=useCallback(async()=>{if(!source)return;try{const values=await listSkillCandidates();if(latest.current===key)setCandidates(values.filter(item=>candidateBelongsToSource(item,source)));}catch(error){if(latest.current===key)setError(error instanceof Error?error.message:"候选不可用");}},[key]);
  useEffect(()=>{setCandidates([]);setSelectedId("");setLesson("");setName("");setAuthorized(false);setConfirmed(false);setVersions([]);setBusy(false);setError("");void reload();},[reload]);
  const reloadVersions=async()=>{const requestedName=name;const values=await listSkillVersions(requestedName);if(latest.current===key&&latestName.current===requestedName)setVersions(values);};
  const run=async(operation:()=>Promise<unknown>)=>{setBusy(true);setError("");try{await operation();if(latest.current===key)await reload();}catch(error){if(latest.current===key)setError(error instanceof Error?error.message:"操作失败");}finally{if(latest.current===key)setBusy(false);}};
  const choose=(id:string)=>{setSelectedId(id);setLesson(candidates.find(item=>item.id===id)?.lesson||"");setConfirmed(false);};
  const accept=(candidate:SkillCandidate)=>{if(latest.current!==key)return;setSelectedId(candidate.id);setLesson(candidate.lesson);setConfirmed(false);};
  if(!source)return <section aria-label="经验候选" className="p-3 text-xs">完成任务后，可选择真实执行记录整理经验候选。</section>;
  return <section aria-label="经验候选与 Skill 版本" className="space-y-2 rounded border border-[var(--border)] p-3 text-xs">
    <h3 className="font-medium">从已完成任务整理经验</h3><p>只读取你授权的当前执行证据；候选不会自动写入长期记忆或成为规则。</p>
    {error?<p role="alert" className="text-[var(--danger)]">{error}</p>:null}
    <label className="block"><input type="checkbox" checked={authorized} onChange={event=>setAuthorized(event.target.checked)}/> 授权使用当前完成任务的证据生成候选</label>
    <textarea aria-label="希望保留的经验或偏好" value={lesson} maxLength={2000} onChange={event=>{setLesson(event.target.value);setConfirmed(false);}} rows={3} className="w-full rounded border border-[var(--border)] bg-[var(--surface)] p-2"/>
    <button type="button" disabled={busy||!completed||!authorized||!lesson.trim()} onClick={()=>void run(async()=>accept(await createSkillCandidate([source],lesson,authorized)))}>新建可审核候选</button>
    <select aria-label="选择候选" value={selectedId} onChange={event=>choose(event.target.value)} className="w-full bg-[var(--surface)]"><option value="">选择已保存候选</option>{candidates.map(item=><option key={item.id} value={item.id}>r{item.revision} · {item.status} · {item.lesson.slice(0,40)}</option>)}</select>
    {selected?<><p>修订 r{selected.revision} · 状态 {selected.status} · 敏感检查 {selected.sensitivity}</p><details><summary>来源记录</summary><pre className="whitespace-pre-wrap break-all">{JSON.stringify(selected.sources,null,2)}</pre></details>
      <div className="flex flex-wrap gap-2"><button type="button" disabled={busy||selected.status==="confirmed"||selected.status==="rejected"} onClick={()=>void run(async()=>accept(await editSkillCandidate(selected,lesson)))}>保存编辑</button>
      <button type="button" disabled={busy||lesson!==selected.lesson||selected.status==="confirmed"||selected.status==="rejected"} onClick={()=>void run(async()=>accept(await changeSkillCandidate(selected,"validate")))}>验证候选</button>
      <button type="button" disabled={busy||selected.status==="confirmed"||selected.status==="rejected"} onClick={()=>void run(async()=>accept(await changeSkillCandidate(selected,"reject")))}>拒绝候选</button></div>
      <input aria-label="托管 Skill 名称" value={name} onChange={event=>{setName(event.target.value);setVersions([]);setConfirmed(false);}} placeholder="例如 reviewed-task" className="w-full border border-[var(--border)] bg-[var(--surface)] p-2"/>
      <label className="block"><input type="checkbox" checked={confirmed} onChange={event=>setConfirmed(event.target.checked)}/> 我已审核当前内容，确认安装为可停用的 Skill</label>
      <button type="button" disabled={busy||!name||!canConfirmCandidate(selected,lesson,confirmed)} onClick={()=>void run(async()=>{await confirmSkillCandidate(selected,name,confirmed);if(latest.current===key){await reloadVersions();setConfirmed(false);}})}>确认安装 Skill 版本</button>
    </>:null}
    <div className="border-t border-[var(--border)] pt-2"><p>Skill 版本与长期 Memory 独立保存。</p><button type="button" disabled={busy||!name} onClick={()=>void run(reloadVersions)}>加载此 Skill 的版本</button>
      <button type="button" disabled={busy||!versions.some(item=>item.active)} onClick={()=>void run(async()=>{await deactivateSkill(versions[0].skill_name);await reloadVersions();})}>停用此 Skill</button>
      <ul>{versions.map(version=><li key={version.version}>{version.skill_name} v{version.version} · {version.active?"已启用":"历史版本"}<button type="button" disabled={busy||version.active} onClick={()=>void run(async()=>{await rollbackSkillVersion(version.skill_name,version.version);await reloadVersions();})}>恢复此版本</button><details><summary>查看已审核内容</summary><pre className="whitespace-pre-wrap">{version.content}</pre></details></li>)}</ul>
    </div>
  </section>;
}
