import {useEffect,useState} from "react";
import {Button,Input,Modal} from "../../../components/ui";
import {connectorRequest} from "../../../api/connectors";
interface Field {key:string;label:string;kind:"text"|"number"|"toggle"|"select";default:string|number|boolean;options:string[]}
interface Data {entry:{plugin_name:string;setting:{title:string;description:string;fields:Field[]}};values:Record<string,string|number|boolean>}
export default function PluginSettingsDialog({nodeId,onClose}:{nodeId:string;onClose:()=>void}) {
 const [,plugin,id]=nodeId.split(":");const path=`/api/plugins/${encodeURIComponent(plugin)}/settings/${encodeURIComponent(id)}`;
 const[data,setData]=useState<Data|null>(null);const[error,setError]=useState("");const[busy,setBusy]=useState(false);const[saved,setSaved]=useState(false);
 useEffect(()=>{let active=true;void connectorRequest<Data>("GET",path).then(result=>{if(active)setData(result);}).catch(cause=>{if(active)setError(String(cause));});return()=>{active=false;};},[path]);
 function change(key:string,value:string|number|boolean){setSaved(false);setData(current=>current?{...current,values:{...current.values,[key]:value}}:current);}
 async function save(){if(!data)return;setBusy(true);setError("");try{setData(await connectorRequest<Data>("PUT",path,{values:data.values}));setSaved(true);}catch(cause){setError(String(cause));}finally{setBusy(false);}}
 return <Modal open onClose={onClose} size="sm" title={data?`${data.entry.plugin_name} · ${data.entry.setting.title}`:"插件设置"} footer={<Button disabled={!data||busy} onClick={()=>void save()}>{busy?"保存中…":saved?"已保存":"保存设置"}</Button>}>
 {error&&<p role="alert" className="text-[var(--danger)]">{error}</p>}{!data&&!error&&<p role="status">正在加载插件设置…</p>}
 {data&&<div className="space-y-4"><p className="text-sm text-[var(--text-muted)]">{data.entry.setting.description}</p>{data.entry.setting.fields.map(field=><label key={field.key} className="flex flex-col gap-2 text-sm">{field.label}{field.kind==="toggle"?<input type="checkbox" checked={data.values[field.key]===true} onChange={e=>change(field.key,e.target.checked)}/>:field.kind==="select"?<select className="rounded-lg border border-[var(--border-soft)] bg-[var(--surface-solid)] p-2" value={String(data.values[field.key])} onChange={e=>change(field.key,e.target.value)}>{field.options.map(option=><option key={option}>{option}</option>)}</select>:<Input type={field.kind==="number"?"number":"text"} value={String(data.values[field.key])} onChange={e=>change(field.key,field.kind==="number"?Number(e.target.value):e.target.value)}/>}</label>)}</div>}
 </Modal>;
}
