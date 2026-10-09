import type {TaskEdge} from '../../../api/taskWorld';

export interface InputBinding {
  target_path: string[];
  source: {kind:'dependency_output';node_id:string;path:string[]};
}
export interface OutputField {path:string[];type:string}
export const samePath=(a:string[],b:string[])=>a.length===b.length&&a.every((key,i)=>key===b[i]);
const prefix=(a:string[],b:string[])=>a.length<=b.length&&a.every((key,i)=>key===b[i]);
export const valueType=(value:unknown)=>value===null?'Null':Array.isArray(value)?'Array':typeof value==='object'?'Object':typeof value==='number'?(Number.isInteger(value)?'Integer':'Number'):typeof value==='boolean'?'Boolean':'String';
const validPath=(path:unknown):path is string[]=>Array.isArray(path)&&path.length>0&&path.length<=16
  &&path.every(s=>typeof s==='string'&&s.trim().length>0&&new TextEncoder().encode(s).length<=128&&!/[\u0000-\u001f\u007f-\u009f*]/.test(s));

export function bindingError(bindings:InputBinding[],directIds?:Set<string>):string|null {
  if(bindings.length>64)return '最多配置 64 个输入绑定';
  for(let i=0;i<bindings.length;i++){
    const b=bindings[i];
    if(!validPath(b.target_path)||!validPath(b.source.path))return '请输入 1–16 个有效路径段，每段最多 128 字节，不含控制字符或 *';
    if(typeof b.source.node_id!=='string'||!/^[a-zA-Z0-9_.-]{1,64}$/.test(b.source.node_id)||directIds&&!directIds.has(b.source.node_id))return '请选择当前节点的直接上游依赖';
    for(const previous of bindings.slice(0,i)){
      if(samePath(previous.target_path,b.target_path))return 'duplicate_binding_target：同一参数不能重复绑定';
      if(prefix(previous.target_path,b.target_path)||prefix(b.target_path,previous.target_path))return 'binding_target_collision：父级与子级参数不能同时绑定';
    }
  }
  return null;
}

export function parseBindings(raw:unknown):InputBinding[] {
  if(raw===undefined)return [];
  if(!Array.isArray(raw))throw new Error('输入绑定必须为数组');
  for(const item of raw){
    if(!item||typeof item!=='object'||Object.keys(item).some(k=>!['target_path','source'].includes(k))
      ||!item.source||item.source.kind!=='dependency_output'||Object.keys(item.source).some(k=>!['kind','node_id','path'].includes(k)))throw new Error('不支持的输入绑定类型');
  }
  const error=bindingError(raw as InputBinding[]);if(error)throw new Error(error);
  return raw as InputBinding[];
}

export function directDependencies<T extends {id:string}>(nodeId:string,edges:TaskEdge[],nodes:T[]):T[] {
  const ids=new Set(edges.filter(e=>e.to===nodeId).map(e=>e.from));
  return nodes.filter(node=>node.id!==nodeId&&ids.has(node.id));
}

/** Only bounded object paths and types. Array indices and actual values stay hidden. */
export function outputFields(value:unknown):OutputField[] {
  const fields:OutputField[]=[];
  const walk=(value:unknown,path:string[])=>{
    if(fields.length>=256||path.length>=16||value===null||typeof value!=='object'||Array.isArray(value))return;
    for(const [key,child] of Object.entries(value)){
      const next=[...path,key];if(!validPath(next)||fields.length>=256)continue;
      fields.push({path:next,type:valueType(child)});walk(child,next);
    }
  };
  walk(value,[]);return fields;
}

export function replaceBinding(bindings:InputBinding[],path:string[],binding:InputBinding|null):InputBinding[] {
  const others=bindings.filter(b=>!samePath(b.target_path,path));return binding?[...others,binding]:others;
}
