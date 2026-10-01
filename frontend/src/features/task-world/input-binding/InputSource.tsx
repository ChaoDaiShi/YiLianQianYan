import {outputFields,replaceBinding,samePath,type InputBinding} from './model';
import type {TaskNodeProjection} from '../taskGraphProjection';
import type {DependencyOutput} from './useDependencyOutputs';

interface Props {
  path:string[];name:string;bindings:InputBinding[];nodes:TaskNodeProjection[];
  outputs:Record<string,DependencyOutput>;onChange:(bindings:InputBinding[])=>void;
}
export default function InputSource({path,name,bindings,nodes,outputs,onChange}:Props){
  const binding=bindings.find(b=>samePath(b.target_path,path));
  const update=(source:InputBinding['source'])=>onChange(replaceBinding(bindings,path,{target_path:path,source}));
  const state=binding?outputs[binding.source.node_id]:undefined;
  const fields=outputFields(state?.output);
  return <div className="my-2 rounded border border-[var(--border-soft)] p-2 text-xs" data-testid="input-source" data-target-path={JSON.stringify(path)}>
    <label className="flex items-center justify-between gap-2">输入来源
      <select aria-label={`参数来源 ${name}`} value={binding?'upstream':'literal'} onChange={e=>e.target.value==='literal'?onChange(replaceBinding(bindings,path,null)):update({kind:'dependency_output',node_id:'',path:[]})} className="rounded border border-[var(--border-soft)] bg-[var(--surface-solid)] p-1">
        <option value="literal">固定值</option><option value="upstream" disabled={!nodes.length}>上游节点</option>
      </select>
    </label>
    {binding&&<div className="mt-2 space-y-2">
      <p className="text-[var(--text-secondary)]">由上游节点提供 · 覆盖同路径固定值</p>
      <label className="block">上游依赖<select aria-label={`上游节点 ${name}`} value={binding.source.node_id} onChange={e=>update({...binding.source,node_id:e.target.value,path:[]})} className="mt-1 w-full rounded border border-[var(--border-soft)] bg-[var(--surface-solid)] p-1">
        <option value="">请选择直接依赖节点</option>{nodes.map(node=><option key={node.id} value={node.id}>{node.title} · {node.latest_execution?.status||node.status} · {outputs[node.id]?.loading?'正在读取':outputs[node.id]?.error?'读取失败':outputs[node.id]?.output!=null?'有成功输出':'尚无成功输出'}</option>)}
      </select></label>
      {state?.loading?<p role="status">正在读取真实输出字段…</p>:state?.error?<p role="alert">{state.error}</p>:fields.length>0?<label className="block">输出字段<select aria-label={`输出字段 ${name}`} value={JSON.stringify(binding.source.path)} onChange={e=>update({...binding.source,path:JSON.parse(e.target.value) as string[]})} className="mt-1 w-full rounded border border-[var(--border-soft)] bg-[var(--surface-solid)] p-1">
        <option value="[]">请选择字段路径</option>{fields.map(field=><option key={JSON.stringify(field.path)} value={JSON.stringify(field.path)}>{field.path.join('.')} · {field.type}</option>)}
      </select></label>:<p>{binding.source.node_id?'上游节点尚无可浏览输出':'先选择上游节点，再选择字段或输入路径段。'}</p>}
      <details open={fields.length===0}><summary>手动路径段</summary><label className="mt-1 block">每行一个 object 属性名<textarea aria-label={`路径段 ${name}`} value={binding.source.path.join('\n')} onChange={e=>update({...binding.source,path:e.target.value.split(/\r?\n/)})} className="mt-1 min-h-16 w-full rounded border border-[var(--border-soft)] bg-[var(--surface-solid)] p-1 font-mono"/></label></details>
    </div>}
  </div>;
}

export function AdvancedBindings({bindings,nodes,outputs,onChange}:Omit<Props,'path'|'name'>){
  return <details className="studio-detail"><summary>高级输入绑定（{bindings.length}）</summary>
    {bindings.map((binding,index)=><div key={index} className="my-2 rounded border border-[var(--border-soft)] p-2">
      <label className="block text-xs">目标路径段（每行一个）<textarea aria-label={`目标路径段 ${index+1}`} value={binding.target_path.join('\n')} onChange={e=>onChange(bindings.map((b,i)=>i===index?{...b,target_path:e.target.value.split(/\r?\n/)}:b))} className="w-full bg-[var(--surface-solid)] p-1 font-mono"/></label>
      <InputSource path={binding.target_path} name={`绑定 ${index+1}`} bindings={bindings} nodes={nodes} outputs={outputs} onChange={onChange}/>
      <button type="button" className="text-xs underline" onClick={()=>onChange(bindings.filter((_,i)=>i!==index))}>移除绑定</button>
    </div>)}
    <button type="button" disabled={bindings.length>=64||nodes.length===0} className="text-xs underline" onClick={()=>onChange([...bindings,{target_path:[''],source:{kind:'dependency_output',node_id:'',path:[]}}])}>添加输入绑定</button>
  </details>;
}
