import { describe, expect, it } from 'vitest';
import { schemaMode, parseArguments } from './schema';
const schema = {type:'object',required:['text'],properties:{text:{type:'string'},count:{type:'integer'},active:{type:'boolean'},mode:{type:'string',enum:['upper','lower']},nested:{type:'object',properties:{ratio:{type:'number'}}}}};
describe('MCP canvas argument schema',()=>{
 it('renders the supported bounded object types',()=>expect(schemaMode(schema)).toBe('form'));
 it('preserves complex schemas through JSON fallback',()=>{
  for(const extra of [{oneOf:[]},{anyOf:[]},{'x-vendor':true},{properties:{list:{type:'array',items:{type:'object'}}}}])expect(schemaMode({...schema,...extra})).toBe('json');
 });
 it('blocks header binding even inside definitions or JSON fallback',()=>expect(schemaMode({...schema,$defs:{x:{properties:{key:{'x-mcp-header':'Authorization'}}}}})).toBe('unsupported'));
 it('requires an object and bounds size and depth',()=>{
  for(const raw of ['no','null','[]','"text"',JSON.stringify({x:'a'.repeat(17000)})])expect(()=>parseArguments(raw)).toThrow();
  let deep:unknown={};for(let i=0;i<18;i++)deep={x:deep};expect(()=>parseArguments(JSON.stringify(deep))).toThrow();
 });
 it('keeps complete JSON values with no automatic dependency substitution',()=>{
  const value={text:'{{node.a.output.x}}',count:2,active:false,nested:{ratio:1.5},extra:[1,2]};expect(parseArguments(JSON.stringify(value))).toEqual(value);
 });
});
