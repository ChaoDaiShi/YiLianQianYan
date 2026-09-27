import { useState } from 'react';
import { HEADER_UNSUPPORTED, parseArguments, schemaMode, type Schema } from './schema';

type Props={schema:Schema|null|undefined; value:string; onChange:(value:string)=>void; disabled?:boolean};
const object=(value:unknown):Record<string,unknown>=>value&&typeof value==='object'&&!Array.isArray(value)?value as Record<string,unknown>:{};
function setPath(value:Record<string,unknown>,path:string[],next:unknown):Record<string,unknown>{
  const [key,...rest]=path;
  return Object.fromEntries([...Object.entries(value).filter(([name])=>name!==key),...(next===undefined&&!rest.length?[]:[[key,rest.length?setPath(object(value[key]),rest,next):next]])]);
}
export default function SchemaForm({schema,value,onChange,disabled}:Props){
  const [advanced,setAdvanced]=useState(false);
  const mode=schemaMode(schema);
  let data:Record<string,unknown>={},error='';
  try{data=parseArguments(value);}catch(e){error=e instanceof Error?e.message:'JSON 参数无效';}
  if(mode==='unsupported')return <p role="alert" className="text-xs text-[var(--warning-fg)]">{HEADER_UNSUPPORTED}</p>;
  const change=(path:string[],next:unknown)=>onChange(JSON.stringify(setPath(data,path,next),null,2));
  const fields=(s:Schema,path:string[]=[])=>Object.entries(object(s.properties)).map(([name,raw])=>{
    const field=object(raw),keys=[...path,name],id=`mcp-arg-${keys.join('.')}`;
    const parent=path.reduce((current,key)=>object(current[key]),data),current=parent[name];
    const required=Array.isArray(s.required)&&s.required.includes(name);
    const label=`${String(field.title||name)}${required?' *':''}`;
    if(field.type==='object')return <fieldset key={id} className="rounded border border-[var(--border-soft)] p-2"><legend className="text-xs">{label}</legend>{fields(field,keys)}</fieldset>;
    const options=Array.isArray(field.enum)?field.enum:field.type==='boolean'?[true,false]:null;
    return <div key={id} className="my-3"><label htmlFor={id} className="mb-1 block text-xs font-medium">{label}</label>
      {options?<select id={id} required={required} value={current===undefined?'':JSON.stringify(current)} onChange={e=>change(keys,e.target.value===''?undefined:JSON.parse(e.target.value))} className="w-full rounded border border-[var(--border-soft)] bg-[var(--surface-solid)] p-2 text-sm"><option value="">请选择</option>{options.map((v,i)=><option key={i} value={JSON.stringify(v)}>{typeof v==='boolean'?(v?'是':'否'):String(v)}</option>)}</select>
      :<input id={id} required={required} type={field.type==='number'||field.type==='integer'?'number':'text'} step={field.type==='integer'?1:'any'} min={typeof field.minimum==='number'?field.minimum:undefined} max={typeof field.maximum==='number'?field.maximum:undefined} minLength={typeof field.minLength==='number'?field.minLength:undefined} maxLength={typeof field.maxLength==='number'?field.maxLength:undefined} value={typeof current==='string'||typeof current==='number'?current:''} onChange={e=>change(keys,e.target.value===''?undefined:field.type==='string'?e.target.value:Number(e.target.value))} className="w-full rounded border border-[var(--border-soft)] bg-[var(--surface-solid)] p-2 text-sm"/>}
      {typeof field.description==='string'&&<p className="mt-1 text-xs text-[var(--text-faint)]">{field.description}</p>}
    </div>;
  });
  return <fieldset disabled={disabled} className="min-w-0" aria-label="MCP 参数">
    <div className="flex items-center justify-between"><span className="text-sm font-medium">参数</span>{mode==='form'&&<button type="button" className="text-xs underline" onClick={()=>setAdvanced(!advanced)}>{advanced?'使用表单':'JSON 参数'}</button>}</div>
    {mode==='json'||advanced||error?<label className="mt-2 block text-xs">JSON 参数<textarea aria-label="JSON 参数" className="mt-1 min-h-40 w-full rounded border border-[var(--border-soft)] bg-[var(--surface-solid)] p-2 font-mono text-xs" value={value} onChange={e=>onChange(e.target.value)}/></label>:fields(schema!)}
    {error&&<p role="alert" className="text-xs text-[var(--danger-fg)]">{error}</p>}
    {mode==='json'&&<p className="mt-1 text-xs text-[var(--text-faint)]">此参数结构使用完整 JSON 编辑；所有字段都会保留。</p>}
  </fieldset>;
}
