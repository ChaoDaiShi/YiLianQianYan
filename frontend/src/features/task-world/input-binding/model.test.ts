import {describe,it,expect} from 'vitest';
import {bindingError,directDependencies,outputFields,parseBindings,replaceBinding,valueType,type InputBinding} from './model';
const binding:InputBinding={target_path:['dataset_id'],source:{kind:'dependency_output',node_id:'a',path:['structured','dataset_id']}};

describe('typed dependency binding model',()=>{
  it('filters incoming direct dependencies by ID, not title or graph membership',()=>{
    const nodes=[{id:'a',title:'同名'},{id:'b',title:'同名'},{id:'c',title:'同名'},{id:'other',title:'其它'}];
    expect(directDependencies('b',[{from:'a',to:'b'},{from:'b',to:'c'},{from:'other',to:'a'}],nodes)).toEqual([nodes[0]]);
  });
  it('preserves vector paths, source identities and empty legacy binding sets',()=>{
    expect(parseBindings(undefined)).toEqual([]);
    expect(parseBindings(JSON.parse(JSON.stringify([binding])))).toEqual([binding]);
    expect(()=>parseBindings([{...binding,source:{...binding.source,kind:'secret'}}])).toThrow();
    expect(()=>parseBindings([{...binding,source:{...binding.source,node_id:123}}])).toThrow();
  });
  it('shows real object paths/types without values or array indices',()=>{
    const fields=outputFields({structured:{dataset_id:'DO_NOT_RENDER_VALUE',rows:42,valid:true,meta:{region:'test'},items:[{id:1}],empty:null}});
    expect(fields).toContainEqual({path:['structured','rows'],type:'Integer'});
    expect(fields).toContainEqual({path:['structured','meta','region'],type:'String'});
    expect(fields).toContainEqual({path:['structured','items'],type:'Array'});
    expect(fields.some(f=>f.path.includes('0'))).toBe(false);
    expect(JSON.stringify(fields)).not.toContain('DO_NOT_RENDER_VALUE');
    expect(outputFields(null)).toEqual([]);expect(valueType(null)).toBe('Null');
  });
  it('bounds paths, fields and bindings and rejects collisions and non-dependencies',()=>{
    expect(outputFields(Object.fromEntries(Array.from({length:500},(_,i)=>[`k${i}`,i])))).toHaveLength(256);
    expect(bindingError([binding],new Set(['different']))).toContain('直接上游');
    expect(bindingError([binding,binding])).toContain('duplicate_binding_target');
    expect(bindingError([binding,{...binding,target_path:['dataset_id','child']}])).toContain('binding_target_collision');
    expect(bindingError([{...binding,source:{...binding.source,path:['*']}}])).toContain('路径段');
    expect(bindingError(Array(65).fill(binding))).toContain('64');
  });
  it('switching one field back to literal preserves other bindings',()=>{
    const rows={...binding,target_path:['rows']};
    expect(replaceBinding([binding,rows],['dataset_id'],null)).toEqual([rows]);
  });
});
