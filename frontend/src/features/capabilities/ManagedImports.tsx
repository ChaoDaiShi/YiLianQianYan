import { useCallback, useEffect, useRef, useState } from "react";
import { Link, useSearchParams } from "react-router-dom";
import { Puzzle, RefreshCw, Plus } from "lucide-react";
import { managementRequest } from "../../api/productManagement";
import { Badge, Button, ConfirmDialog, Input, EmptyState, ErrorState, Skeleton, Select } from "../../components/ui";
import PluginEntries from "../plugins/PluginEntries";
import PluginTemplateDialog from "../plugins/PluginTemplateDialog";
import PackageImportDialog from "./PackageImportDialog";
import "./capabilityNavigation.css";

export interface ManagedImport { id: string; name: string; version: string; kind: string; source: string; permissions: string[]; content_hash: string; revision: number; enabled: boolean; installed: boolean; history_count: number; runtime_ready: boolean; runtime_status: string; credential_cleanup_warning?: string }
export function filterManagedImports(records: ManagedImport[], filters: { kind?: string; query: string; status: string; source: string }) {
  const query = filters.query.trim().toLowerCase();
  return records.filter(record => (!filters.kind || filters.kind === record.kind) && `${record.name} ${record.id} ${record.source}`.toLowerCase().includes(query) &&
    (filters.source === "all" || record.source === filters.source) &&
    (filters.status === "all" || (filters.status === "removed" ? !record.installed : filters.status === "ready" ? record.installed && record.runtime_ready : record.installed && (filters.status === "enabled" ? record.enabled : !record.enabled))));
}
const recordStatus = (record: ManagedImport) => !record.installed ? "已卸载" : !record.enabled ? "已禁用" : record.runtime_ready ? "已就绪" : "已启用 · 未就绪";
const sourceLabel = (source: string) => source === "plugin_directory" ? "本地插件目录" : source === "zip" ? "本地 ZIP" : source;

export default function ManagedImports({ kind, initialId, initialDirectory, initialName }: { kind?: "skill" | "plugin"; initialId?: string; initialDirectory?: string; initialName?: string } = {}) {
  const [,setParams] = useSearchParams();
  const [formOpen,setFormOpen] = useState(false);
  const [templateOpen,setTemplateOpen] = useState(false);
  const [records,setRecords] = useState<ManagedImport[]|null>(null);
  const [selected,setSelected] = useState<string|null>(initialId ?? null);
  const [confirmation, setConfirmation] = useState<{ record: ManagedImport; action: "uninstall" | "rollback" } | null>(null);
  const [query,setQuery] = useState(""); const [status,setStatus] = useState("all"); const [source,setSource] = useState("all");
  const [error,setError] = useState(""); const [busy,setBusy] = useState(false); const inFlight=useRef(false);
  const [cleanupWarning, setCleanupWarning] = useState("");
  const reload=useCallback(async()=>{const data=await managementRequest<{imports:ManagedImport[]}>("GET","/api/capabilities/imports");setRecords(data.imports);},[]);
  useEffect(()=>{void reload().catch(cause=>setError(cause instanceof Error?cause.message:String(cause)));},[reload]);
  useEffect(()=>{if(initialId&&initialId!==selected){setSelected(initialId);setQuery("");setStatus("all");setSource("all");}},[initialId]);
  useEffect(()=>{if(initialDirectory)setFormOpen(true);},[initialDirectory]);
  const run=async(action:()=>Promise<void>)=>{if(inFlight.current)return;inFlight.current=true;setBusy(true);setError("");try{await action();}catch(cause){setError(cause instanceof Error?cause.message:String(cause));}finally{inFlight.current=false;setBusy(false);}};
  const applyChange=(record:ManagedImport,action:string)=>run(async()=>{
    const result = await managementRequest<ManagedImport>(action==="uninstall"?"DELETE":"PUT",`/api/capabilities/imports/${encodeURIComponent(record.id)}`,action==="uninstall"?{revision:record.revision}:{revision:record.revision,action});
    setCleanupWarning(result?.credential_cleanup_warning ?? "");
    setConfirmation(null);
    window.dispatchEvent(new Event("plugin-features-changed"));
    await reload();
  });
  function change(record: ManagedImport, action: string) {
    if (action === "uninstall" || action === "rollback") setConfirmation({ record, action });
    else void applyChange(record, action);
  }
  const filtered=filterManagedImports(records??[],{kind,query,status,source});
  const record=selected ? records?.find(item=>item.id===selected&&(!kind||item.kind===kind)) : kind === "plugin" ? filtered.find(item => item.installed) ?? filtered[0] : undefined;
  const sources=[...new Set((records??[]).filter(item=>!kind||item.kind===kind).map(item=>item.source))];
  const select=(record:ManagedImport)=>{setSelected(record.id);setParams(current=>{const next=new URLSearchParams(current);next.set(kind==="plugin"?"plugin":"package",record.id);return next;},{replace:true});};
  return <section id="managed-imports" className="managed-imports-workspace" aria-label={kind==="plugin"?"插件管理":"托管能力导入"}>
    <div className="capability-workbench-toolbar"><Input aria-label={kind==="plugin"?"搜索插件":"搜索托管能力"} placeholder={kind==="plugin"?"搜索插件名称或来源…":"搜索已导入的能力…"} value={query} onChange={event=>setQuery(event.target.value)}/><Select aria-label="导入项状态筛选" value={status} onChange={event=>setStatus(event.target.value)} options={[{value:"all",label:"全部状态"},{value:"ready",label:"已就绪"},{value:"enabled",label:"已启用"},{value:"disabled",label:"已禁用"},{value:"removed",label:"已卸载"}]}/><Select aria-label="导入项来源筛选" value={source} onChange={event=>setSource(event.target.value)} options={[{value:"all",label:"全部来源"},...sources.map(value=>({value,label:sourceLabel(value)}))]}/><Button variant="secondary" size="sm" disabled={busy} onClick={()=>void run(reload)} aria-label="刷新导入清单"><RefreshCw size={14}/></Button>{kind === "plugin" ? (
            <><Button size="sm" variant="secondary" onClick={()=>setTemplateOpen(true)}>开发插件</Button><Button size="sm" onClick={()=>setFormOpen(true)}>导入插件</Button></>
    ) : (
      <Button size="sm" onClick={()=>{setFormOpen(true);}}><Plus size={14}/>导入 Skill 或插件</Button>
    )}</div>
    {error&&<ErrorState title="能力来源操作未完成" description={error}/>}
    {cleanupWarning && <p role="status" className="text-sm">{cleanupWarning}</p>}
    <div className="capability-resource-split"><div className="capability-resource-list" aria-label={kind==="plugin"?"插件列表":"托管导入清单"}><p className="capability-list-caption">{filtered.length} 项</p>{records===null?(error?null:<Skeleton className="h-20"/>):filtered.length?filtered.map(item=><button key={item.id} type="button" className="capability-resource-row" aria-current={record?.id===item.id?"true":undefined} onClick={()=>select(item)}><span className="capability-resource-name"><Puzzle size={16}/><strong>{item.name}</strong></span><span>{item.kind==="plugin"?"插件":"Skill"} · v{item.version}</span><small>{recordStatus(item)}</small></button>):<EmptyState title={query||source!=="all"||status!=="all"?"没有匹配的导入项":kind==="plugin"?"暂无插件":"暂无托管导入"} description="导入后可查看状态、启用或管理版本。"/>}</div>
      <div className="capability-resource-detail" aria-label={kind === "plugin" ? "插件详情" : "导入项详情"}>
        {record ? <>
          <h3>{record.name}</h3>
          <div className="flex flex-wrap gap-2"><Badge tone={record.runtime_ready && record.installed ? "success" : "default"}>{recordStatus(record)}</Badge><Badge>v{record.version}</Badge></div>
          <p>{record.installed ? record.runtime_ready ? "能力已进入当前运行时。" : record.enabled ? `运行状态：${record.runtime_status}` : "启用后可在相应能力入口中使用。" : "可将此记录恢复为禁用状态。"}</p>
          <div className="flex flex-wrap gap-2">
            {record.kind === "plugin" && kind !== "plugin" ? <Link className="capability-owner-link" to={`/capabilities?category=plugin&plugin=${encodeURIComponent(record.id)}`}>管理插件</Link>
              : record.installed ? <>
                <Button size="sm" disabled={busy} onClick={() => change(record, record.enabled ? "disable" : "enable")}>{record.enabled ? "禁用" : "启用声明"}</Button>
                {kind !== "plugin" && <>
                  <Button size="sm" variant="secondary" disabled={busy || record.history_count === 0} onClick={() => change(record, "rollback")}>回滚上一版本</Button>
                  <Button size="sm" variant="ghost" disabled={busy} onClick={() => change(record, "uninstall")}>卸载</Button>
                </>}
              </> : <Button size="sm" disabled={busy} onClick={() => change(record, "restore")}>恢复（禁用状态）</Button>}
          </div>
          {kind === "plugin" && record.installed && record.enabled && <PluginEntries pluginId={record.id} revision={record.revision} />}
          <details>
            <summary>{kind === "plugin" ? "来源、权限与版本维护" : "来源、权限与版本详情"}</summary>
            <p>{sourceLabel(record.source)}</p><p>标识：{record.id} · 修订 {record.revision}</p><p>权限：{record.permissions.join("、") || "无"}</p><p>SHA-256：{record.content_hash}</p>
            {kind === "plugin" && record.installed && <div className="flex flex-wrap gap-2 pt-1">
              <Button size="sm" variant="secondary" disabled={busy || record.history_count === 0} onClick={() => change(record, "rollback")}>回滚上一版本</Button>
              <Button size="sm" variant="danger" disabled={busy} onClick={() => change(record, "uninstall")}>卸载</Button>
            </div>}
          </details>
        </> : <EmptyState title={selected ? "未找到请求的导入项" : "选择一个导入项"} description="查看可用状态、版本与管理操作。" />}
      </div>
    </div>
    <ConfirmDialog open={confirmation !== null} title={confirmation?.action === "uninstall" ? "卸载导入项" : "回滚版本"}
      description={confirmation?.action === "uninstall" ? confirmation.record.kind === "plugin"
        ? `卸载「${confirmation.record.name}」会停用插件并撤销其专属 MCP 授权和服务令牌。运行设置和外部文件会保留。如使用服务令牌，恢复安装后需重新配置。`
        : `卸载「${confirmation.record.name}」会停用它提供的功能。安装记录仍可恢复。`
        : `将「${confirmation?.record.name ?? ""}」恢复到上一版本。`}
      confirmLabel={confirmation?.action === "uninstall" ? "确认卸载" : "确认回滚"} tone="danger" loading={busy}
      onCancel={() => { if (!busy) setConfirmation(null); }} onConfirm={() => confirmation ? applyChange(confirmation.record, confirmation.action) : undefined}>
      {error && <p className="text-sm text-[var(--danger-fg)]" role="alert">{error}</p>}
    </ConfirmDialog>
    {formOpen && <PackageImportDialog pluginOnly={kind==="plugin"} initialDirectory={initialDirectory} initialName={initialName}
      onClose={()=>setFormOpen(false)} onImported={item=>{
        setRecords(current=>[item,...(current??[]).filter(record=>record.id!==item.id)]);
        select(item);
        setQuery("");setStatus("all");setSource("all");setFormOpen(false);
        window.dispatchEvent(new Event("plugin-features-changed"));
      }} />}
    {templateOpen && <PluginTemplateDialog onClose={()=>setTemplateOpen(false)} />}
  </section>;
}
