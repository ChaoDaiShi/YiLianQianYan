export type Schema = Record<string, unknown>;
export const HEADER_UNSUPPORTED = '此工具需要安全请求头参数，当前画布尚未支持安全凭据绑定。';
export function hasHeaderBinding(value: unknown): boolean {
  if (!value || typeof value !== 'object') return false;
  if (!Array.isArray(value) && Object.prototype.hasOwnProperty.call(value, 'x-mcp-header')) return true;
  return Object.values(value).some(hasHeaderBinding);
}
const allowed = new Set(['type','title','description','default','enum','properties','required','additionalProperties','minimum','maximum','minLength','maxLength']);
export function schemaMode(schema: unknown): 'form'|'json'|'unsupported' {
  if (hasHeaderBinding(schema)) return 'unsupported';
  let fields=0;
  const simple=(value:unknown,depth:number):boolean=>{
    if (!value || typeof value!=='object' || Array.isArray(value) || depth>3) return false;
    const s=value as Schema;
    if(Object.keys(s).some(key=>!allowed.has(key)))return false;
    if(!['object','string','number','integer','boolean'].includes(String(s.type)))return false;
    if(s.enum!==undefined && (!Array.isArray(s.enum)||s.enum.length===0||s.enum.length>32||s.enum.some(v=>v===null||typeof v==='object')))return false;
    if(s.type==='object'){
      if(s.additionalProperties!==undefined&&typeof s.additionalProperties!=='boolean')return false;
      if(!s.properties||typeof s.properties!=='object'||Array.isArray(s.properties))return false;
      const props=s.properties as Record<string,unknown>;fields+=Object.keys(props).length;
      if(fields>32)return false;
      if(s.required!==undefined&&(!Array.isArray(s.required)||s.required.some(k=>typeof k!=='string'||!Object.prototype.hasOwnProperty.call(props,k))))return false;
      return Object.values(props).every(v=>simple(v,depth+1));
    }
    return true;
  };
  return (schema as Schema)?.type==='object'&&simple(schema,0)?'form':'json';
}
export function parseArguments(raw: string): Record<string, unknown> {
  if(new TextEncoder().encode(raw).length>16384)throw new Error('参数不能超过 16 KiB');
  const value:unknown=JSON.parse(raw);
  if(!value||typeof value!=='object'||Array.isArray(value))throw new Error('参数必须是 JSON 对象');
  const bounded=(v:unknown,depth:number):boolean=>depth<=16&&(!v||typeof v!=='object'||Object.values(v).every(child=>bounded(child,depth+1)));
  if(!bounded(value,0))throw new Error('参数嵌套不能超过 16 层');
  return value as Record<string, unknown>;
}

