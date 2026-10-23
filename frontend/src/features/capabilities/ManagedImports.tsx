import { useCallback, useEffect, useRef, useState } from "react";
import { Link, useSearchParams } from "react-router-dom";
import { Puzzle, RefreshCw, Plus } from "lucide-react";
import { managementRequest } from "../../api/productManagement";
import { Badge, Button, Input, Textarea, EmptyState, ErrorState, Skeleton, Modal, Select } from "../../components/ui";
import "./capabilityNavigation.css";

export interface ManagedImport { id: string; name: string; version: string; kind: string; source: string; permissions: string[]; content_hash: string; revision: number; enabled: boolean; installed: boolean; history_count: number; runtime_ready: boolean; runtime_status: string }
interface ImportPreview { preview_id: string; package: { id: string; name: string; version: string; kind: string; permissions: string[]; content_hash: string; files: Record<string, string>; file_encodings: Record<string, "base64"> }; added: string[]; changed: string[]; removed: string[]; previous_permissions: string[]; unchanged: boolean }
export function filterManagedImports(records: ManagedImport[], filters: { kind?: string; query: string; status: string; source: string }) {
  const query = filters.query.trim().toLowerCase();
  return records.filter(record => (!filters.kind || filters.kind === record.kind) && `${record.name} ${record.id} ${record.source}`.toLowerCase().includes(query) &&
    (filters.source === "all" || record.source === filters.source) &&
    (filters.status === "all" || (filters.status === "removed" ? !record.installed : filters.status === "ready" ? record.installed && record.runtime_ready : record.installed && (filters.status === "enabled" ? record.enabled : !record.enabled))));
}
const recordStatus = (record: ManagedImport) => !record.installed ? "已卸载" : !record.enabled ? "已禁用" : record.runtime_ready ? "已就绪" : "已启用 · 未就绪";

export default function ManagedImports({ kind, initialId, initialDirectory, initialName }: { kind?: "skill" | "plugin"; initialId?: string; initialDirectory?: string; initialName?: string } = {}) {
  const [,setParams] = useSearchParams();
  const [format,setFormat] = useState(kind === "plugin" ? "zip" : "markdown");
  const [formOpen,setFormOpen] = useState(false);
  const [id,setId] = useState(""); const [name,setName] = useState(""); const [version,setVersion] = useState("1.0.0");
  const [content,setContent] = useState(""); const [zip,setZip] = useState(""); const [directory,setDirectory] = useState("");
  const [repository,setRepository] = useState(""); const [commit,setCommit] = useState(""); const [path,setPath] = useState(kind === "plugin" ? "plugin.zip" : "SKILL.md");
  const [preview,setPreview] = useState<ImportPreview|null>(null);
  const [records,setRecords] = useState<ManagedImport[]|null>(null);
  const [selected,setSelected] = useState<string|null>(initialId ?? null);
  const [query,setQuery] = useState(""); const [status,setStatus] = useState("all"); const [source,setSource] = useState("all");
  const [error,setError] = useState(""); const [busy,setBusy] = useState(false); const inFlight=useRef(false);
  const reload=useCallback(async()=>{const data=await managementRequest<{imports:ManagedImport[]}>("GET","/api/capabilities/imports");setRecords(data.imports);},[]);
  useEffect(()=>{void reload().catch(cause=>setError(cause instanceof Error?cause.message:String(cause)));},[reload]);
  useEffect(()=>{if(initialId&&initialId!==selected){setSelected(initialId);setQuery("");setStatus("all");setSource("all");}},[initialId]);
  useEffect(()=>{if(initialDirectory){setFormat("directory");setDirectory(initialDirectory);setName(initialName??"");setPreview(null);setFormOpen(true);}},[initialDirectory,initialName]);
  const run=async(action:()=>Promise<void>)=>{if(inFlight.current)return;inFlight.current=true;setBusy(true);setError("");try{await action();}catch(cause){setError(cause instanceof Error?cause.message:String(cause));}finally{inFlight.current=false;setBusy(false);}};
  const inspect=()=>run(async()=>{
    setPreview(null);
    const body=format==="directory"?{format,id,name,version,directory}:format==="zip"?{format,data_base64:zip}:format==="github"?{format,id,name,version,repository,commit,path}:{format,id,name,version,content};
    const result=await managementRequest<ImportPreview>("POST","/api/capabilities/imports/inspect",body);
    if(kind&&result.package.kind!==kind)throw new Error(`此包为 ${result.package.kind}，请在对应类别导入。`);
    setPreview(result);
  });
  const change=(record:ManagedImport,action:string)=>run(async()=>{
    if(["uninstall","rollback"].includes(action)&&!window.confirm(`${action==="uninstall"?"卸载":"回滚"} ${record.name}？`))return;
    await managementRequest(action==="uninstall"?"DELETE":"PUT",`/api/capabilities/imports/${encodeURIComponent(record.id)}`,action==="uninstall"?{revision:record.revision}:{revision:record.revision,action});
    await reload();window.dispatchEvent(new Event("plugin-features-changed"));
  });
  const filtered=filterManagedImports(records??[],{kind,query,status,source});
  const record=records?.find(item=>item.id===selected&&(!kind||item.kind===kind));
  const sources=[...new Set((records??[]).filter(item=>!kind||item.kind===kind).map(item=>item.source))];
  const select=(record:ManagedImport)=>{setSelected(record.id);setParams(current=>{const next=new URLSearchParams(current);next.set(kind==="plugin"?"plugin":"package",record.id);return next;},{replace:true});};
  return <section id="managed-imports" className="managed-imports-workspace" aria-label={kind==="plugin"?"插件管理":"托管能力导入"}>
    <div className="capability-workbench-toolbar"><Input aria-label={kind==="plugin"?"搜索插件":"搜索托管能力"} placeholder={kind==="plugin"?"搜索插件名称或来源…":"搜索已导入的能力…"} value={query} onChange={event=>setQuery(event.target.value)}/><Select aria-label="导入项状态筛选" value={status} onChange={event=>setStatus(event.target.value)} options={[{value:"all",label:"全部状态"},{value:"ready",label:"已就绪"},{value:"enabled",label:"已启用"},{value:"disabled",label:"已禁用"},{value:"removed",label:"已卸载"}]}/><Select aria-label="导入项来源筛选" value={source} onChange={event=>setSource(event.target.value)} options={[{value:"all",label:"全部来源"},...sources.map(value=>({value,label:value}))]}/><Button variant="secondary" size="sm" disabled={busy} onClick={()=>void run(reload)} aria-label="刷新导入清单"><RefreshCw size={14}/></Button><Button size="sm" onClick={()=>{setError("");setPreview(null);setFormOpen(true);}}><Plus size={14}/>{kind==="plugin"?"导入插件":"导入 Skill 或插件"}</Button></div>
    {error&&!formOpen&&<ErrorState title="能力来源操作未完成" description={error}/>}
    <div className="capability-resource-split"><div className="capability-resource-list" aria-label={kind==="plugin"?"插件列表":"托管导入清单"}><p className="capability-list-caption">{filtered.length} 项</p>{records===null?(error?null:<Skeleton className="h-20"/>):filtered.length?filtered.map(item=><button key={item.id} type="button" className="capability-resource-row" aria-current={selected===item.id?"true":undefined} onClick={()=>select(item)}><span className="capability-resource-name"><Puzzle size={16}/><strong>{item.name}</strong></span><span>{item.kind==="plugin"?"插件":"Skill"} · v{item.version}</span><small>{recordStatus(item)}</small></button>):<EmptyState title={query||source!=="all"||status!=="all"?"没有匹配的导入项":kind==="plugin"?"暂无插件":"暂无托管导入"} description="导入后可查看状态、启用或管理版本。"/>}</div>
      <div className="capability-resource-detail" aria-label={kind==="plugin"?"插件详情":"导入项详情"}>{record?<><h3>{record.name}</h3><div className="flex gap-2"><Badge tone={record.runtime_ready&&record.installed?"success":"default"}>{recordStatus(record)}</Badge><Badge>v{record.version}</Badge></div><p>{record.installed?(record.runtime_ready?"能力已进入当前运行时。":record.enabled?`运行状态：${record.runtime_status}`:"启用后可在相应能力入口中使用。" ):"安装记录已保留，可以恢复为禁用状态。"}</p><div className="flex flex-wrap gap-2">{record.installed?<><Button size="sm" disabled={busy} onClick={()=>void change(record,record.enabled?"disable":"enable")}>{record.enabled?"禁用":"启用声明"}</Button><Button size="sm" variant="secondary" disabled={busy||record.history_count===0} onClick={()=>void change(record,"rollback")}>回滚上一版本</Button><Button size="sm" variant="ghost" disabled={busy} onClick={()=>void change(record,"uninstall")}>卸载</Button></>:<Button size="sm" disabled={busy} onClick={()=>void change(record,"restore")}>恢复（禁用状态）</Button>}</div><details><summary>来源、权限与版本详情</summary><p>{record.source}</p><p>标识：{record.id} · 修订 {record.revision}</p><p>权限：{record.permissions.join("、")||"无"}</p><p>SHA-256：{record.content_hash}</p></details>{record.kind==="plugin"&&record.installed&&<p><Link to="/more" className="text-[var(--accent-primary)] underline">查看插件提供的功能入口</Link></p>}</>:<EmptyState title={selected?"未找到请求的导入项":"选择一个导入项"} description="查看可用状态、版本与管理操作。"/>}</div>
    </div>
    <Modal open={formOpen} onClose={()=>{if(!busy)setFormOpen(false);}} title={kind==="plugin"?"导入插件":"导入 Skill 或插件"} size="lg" footer={<Button variant="secondary" disabled={busy} onClick={()=>setFormOpen(false)}>关闭</Button>}>
      <div className="space-y-4">{error&&<ErrorState title="导入未完成" description={error}/>}<p className="text-sm text-[var(--text-secondary)]">选择来源，检查变更后确认安装。</p><fieldset disabled={busy} onChange={()=>setPreview(null)} className="space-y-3">
        <Select label="导入来源" value={format} onChange={event=>{setFormat(event.target.value);setZip("");}} options={[...(kind!=="plugin"?[{value:"directory",label:"本地 Skill 目录"},{value:"markdown",label:"Markdown Skill"}]:[]),{value:"zip",label:kind==="plugin"?"ZIP 插件声明包":"ZIP 声明包"},{value:"github",label:"公开 GitHub"}]}/>
        {format!=="zip"&&<div className="grid gap-3 sm:grid-cols-3"><Input label="稳定 ID" value={id} onChange={event=>setId(event.target.value)} placeholder={kind==="plugin"?"my.plugin":"my.skill"}/><Input label="名称" value={name} onChange={event=>setName(event.target.value)}/><Input label="版本" value={version} onChange={event=>setVersion(event.target.value)}/></div>}
        {format==="directory"?<Input label="Skill 目录绝对路径" value={directory} onChange={event=>setDirectory(event.target.value)}/>:format==="github"?<><Input label="仓库地址" value={repository} onChange={event=>setRepository(event.target.value)}/><Input label="Commit（40 位十六进制）" value={commit} onChange={event=>setCommit(event.target.value)}/><Input label="仓库内路径（.md 或 .zip）" value={path} onChange={event=>setPath(event.target.value)}/></>:<label className="block text-sm">选择文件<input type="file" className="mt-2 block w-full" accept={format==="zip"?".zip":".md"} onChange={event=>{const file=event.target.files?.[0];if(!file)return;void run(async()=>{if(file.size>(format==="zip"?2*1024*1024:512*1024))throw new Error("文件超出导入上限。");if(format==="markdown")setContent(await file.text());else{const bytes=new Uint8Array(await file.arrayBuffer());let binary="";for(let offset=0;offset<bytes.length;offset+=8192)binary+=String.fromCharCode(...bytes.subarray(offset,offset+8192));setZip(btoa(binary));}});}}/></label>}
        {format==="markdown"&&<Textarea label="Skill Markdown" value={content} onChange={event=>setContent(event.target.value)} rows={6}/>}
        <Button disabled={busy||(format==="zip"?!zip:!id||!name)} onClick={()=>void inspect()}>检查并生成预览</Button>
      </fieldset>
      {preview&&<div className="rounded-xl border border-[var(--border-soft)] p-4 space-y-3" aria-label="导入变更预览"><h3>{preview.package.name} · v{preview.package.version}</h3><p className="text-sm">新增 {preview.added.join("、")||"无"}；修改 {preview.changed.join("、")||"无"}；移除 {preview.removed.join("、")||"无"}</p><p className="text-sm">权限：{preview.previous_permissions.join("、")||"无"} → {preview.package.permissions.join("、")||"无"}</p><details><summary>完整声明与文件详情</summary><p className="text-xs break-all">SHA-256 {preview.package.content_hash}</p>{Object.entries(preview.package.files).map(([file,text])=><div key={file}><h4>{file}</h4><pre className="max-h-52 overflow-auto whitespace-pre-wrap text-xs">{preview.package.file_encodings[file]==="base64"?`二进制资源 · ${Math.floor(text.length*3/4)-(text.endsWith("==")?2:text.endsWith("=")?1:0)} 字节`:text}</pre></div>)}</details><p className="text-xs text-[var(--text-secondary)]">{preview.unchanged?"内容一致，不会重复安装。":"安装或更新后为禁用状态，可在清单中启用。"}</p><Button disabled={busy} onClick={()=>void run(async()=>{await managementRequest("POST","/api/capabilities/imports/confirm",{preview_id:preview.preview_id});await reload();setSelected(preview.package.id);setQuery("");setStatus("all");setSource("all");setPreview(null);setFormOpen(false);window.dispatchEvent(new Event("plugin-features-changed"));})}>确认安装或更新</Button></div>}
      </div>
    </Modal>
  </section>;
}
