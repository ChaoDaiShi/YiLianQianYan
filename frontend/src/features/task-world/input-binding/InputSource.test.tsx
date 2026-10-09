import {renderToStaticMarkup} from 'react-dom/server';
import {describe,it,expect} from 'vitest';
import InputSource from './InputSource';
import type {TaskNodeProjection} from '../taskGraphProjection';

describe('binding editor presentation',()=>{
  it('renders persisted binding identity/path and actual type tree on reload',()=>{
    const binding={target_path:['id'],source:{kind:'dependency_output' as const,node_id:'a',path:['structured','id']}};
    const html=renderToStaticMarkup(<InputSource path={['id']} name="id" bindings={[binding]}
      nodes={[{id:'a',title:'上游数据',status:'succeeded'} as TaskNodeProjection]}
      outputs={{a:{loading:false,output:{structured:{id:'PRIVATE_VALUE',rows:42}}}}} onChange={()=>{}}/>);
    expect(html).toContain('由上游节点提供');expect(html).toContain('上游数据');
    expect(html).toContain('structured.id');expect(html).toContain('String');expect(html).not.toContain('PRIVATE_VALUE');
  });
  it('states that no browsable output exists instead of inventing fields',()=>{
    const html=renderToStaticMarkup(<InputSource path={['id']} name="id"
      bindings={[{target_path:['id'],source:{kind:'dependency_output',node_id:'a',path:['structured','id']}}]}
      nodes={[{id:'a',title:'上游',status:'pending'} as TaskNodeProjection]} outputs={{a:{loading:false,output:null}}} onChange={()=>{}}/>);
    expect(html).toContain('上游节点尚无可浏览输出');expect(html).toContain('路径段 id');
  });
});
