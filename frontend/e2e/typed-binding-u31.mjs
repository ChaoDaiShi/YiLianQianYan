import assert from 'node:assert/strict';
import {randomUUID} from 'node:crypto';
import {writeFile} from 'node:fs/promises';
import {join} from 'node:path';
import {fixture} from './mcp-canvas-u3.mjs';

// Real product transport, Task Harness, Gateway, persistent grants and approvals.
// Fixture data originates in structuredContent. No test injects resolved B args.
export async function typedBindingU31({page,backendPort,frontendPort,controlToken,evidenceDir}) {
  const remote=await fixture('U31 LOCAL MCP FIXTURE',false,true);
  const snapshots=[],timeline=[],network=[],geometries=[],grants=new Set();
  const save=(name,data)=>writeFile(join(evidenceDir,name),JSON.stringify(data,null,2));
  const api=async(method,path,body,expected)=>{
    const response=await fetch(`http://127.0.0.1:${backendPort}${path}`,{method,headers:{'Content-Type':'application/json','X-Yilian-Control-Session':controlToken},body:body===undefined?undefined:JSON.stringify(body)});
    const text=await response.text();network.push({method,path,status:response.status});
    if(expected)assert.equal(response.status,expected,text);else assert.ok(response.ok,`${path}: ${response.status} ${text}`);
    try{return JSON.parse(text);}catch{return text;}
  };
  const until=async(read,accept,label)=>{let value;const end=Date.now()+20000;do{value=await read();if(accept(value))return value;await new Promise(r=>setTimeout(r,100));}while(Date.now()<end);throw Error(`${label}: ${JSON.stringify(value)}`);};
  const bCalls=()=>remote.calls.filter(c=>c.tool==='create_layer');
  const binding=(target,path=['structured',target])=>({target_path:[target],source:{kind:'dependency_output',node_id:'a',path}});
  const records=async(path,node,id)=>{
    const result=await api('GET',`${path}/nodes/${node}/executions?include_output=true&include_snapshot=true${id?`&execution_id=${id}`:''}`);
    snapshots.push({path,node,selected:id||'latest',...result});return result;
  };
  const detail=async path=>(await api('GET',`${path}/detail`)).detail;
  const waitExecution=(path,node,status)=>until(async()=>(await detail(path)).nodes.find(n=>n.id===node)?.latest_execution,e=>e?.status===status,`${node}: ${status}`);
  const start=async(path,node)=>api('POST',`${path}/nodes/${node}/executions`,{expected_revision:(await detail(path)).revision});
  const pending=async node=>(await until(async()=>(await api('GET','/api/approvals/pending')).approvals,a=>a.some(p=>p.task_node_id===node),'pending')).find(p=>p.task_node_id===node);
  const approve=async(approval,throughUI=false)=>{
    timeline.push({stage:'before_approve',approval_id:approval.approval_id,args:approval.arguments,remote_calls:remote.calls.length});
    if(throughUI){const p=await page.context().newPage();try{
      await p.goto(`http://127.0.0.1:${frontendPort}/system?card=approvals`,{waitUntil:'domcontentloaded'});
      const card=p.locator(`[data-testid="task-canvas-approval"][data-approval-id="${approval.approval_id}"]`);await card.waitFor();
      await p.screenshot({path:join(evidenceDir,`approval-${approval.task_node_id}-${timeline.length}.png`)});
      await card.getByRole('button',{name:'允许此次工具操作',exact:true}).click();await card.waitFor({state:'detached'});
    }finally{await p.close();}}
    else await api('POST',`/api/approvals/${approval.approval_id}/approve`,{});
    timeline.push({stage:'after_approve',approval_id:approval.approval_id,remote_calls:remote.calls.length});
  };
  const geometry=()=>page.locator('.react-flow').evaluate(el=>({camera:el.querySelector('.react-flow__viewport').style.transform,nodes:[...el.querySelectorAll('.react-flow__node')].map(n=>({id:n.dataset.id,transform:n.style.transform,width:n.offsetWidth,height:n.offsetHeight}))}));
  const capture=async label=>{const record=await geometry();geometries.push({label,...record});await page.screenshot({path:join(evidenceDir,`${label}.png`)});return record;};
  let caps,server,chain,baselineGrants;
  try {
    baselineGrants=(await api('GET','/api/security/grants')).grants;
    server=await api('POST','/api/plugins/mcp',{name:remote.name,transport:'streamable_http',url:remote.url});
    await api('POST','/api/capabilities/refresh');caps=(await api('GET','/api/capabilities?provider=mcp&limit=200')).capabilities;
    const tool=name=>caps.find(c=>c.name===name&&c.metadata.source_name===remote.name);
    assert.ok(tool('inspect_dataset'));assert.ok(tool('create_layer'));
    const create=async(scenario,bindings=[],extra=false)=>{
      const id=`u31-${randomUUID()}`,path=`/api/task-world/graphs/${id}`;
      await api('POST','/api/task-world/graphs',{id,nodes:[
        {id:'a',kind:'work',title:'Inspect dataset',input:{executor_ref:`capability://${tool('inspect_dataset').id}`,capability_input:{key:id,scenario}},retry_policy:{max_attempts:1}},
        {id:'b',kind:'work',title:'Create layer',input:{executor_ref:`capability://${tool('create_layer').id}`,capability_input:{layer_name:'灾情图层'},...(bindings.length?{capability_bindings:bindings}:{})},retry_policy:{max_attempts:1}},
        ...(extra?[{id:'unconnected',kind:'work',title:'Inspect dataset',input:{executor_ref:`capability://${tool('inspect_dataset').id}`,capability_input:{key:'unconnected',scenario:'normal'}},retry_policy:{max_attempts:1}}]:[])],edges:[{from:'a',to:'b'}]});
      return path;
    };
    const runA=async path=>{await start(path,'a');await waitExecution(path,'a','waiting_approval');await approve(await pending('a'));return waitExecution(path,'a','succeeded');};
    chain=await create('normal',[],true);
    await page.goto(`http://127.0.0.1:${frontendPort}/task-world/${chain.split('/').at(-1)}`,{waitUntil:'domcontentloaded'});
    await page.locator('.react-flow__node[data-id="a"]').click();await page.getByRole('button',{name:'运行选中节点',exact:true}).click();
    await waitExecution(chain,'a','waiting_approval');assert.equal(remote.calls.length,0);
    const aApproval=await pending('a');await approve(aApproval,true);await waitExecution(chain,'a','succeeded');
    const firstA=await records(chain,'a');assert.equal(firstA.latest_successful_output.structured.dataset_id,'ds-001');assert.equal(firstA.latest_successful_output.structured.rows,42);
    assert.match(firstA.latest_successful_output.content,/文本不是结构化数据/);
    await save('upstream-output.json',firstA.latest_successful_output);

    // Bind using actual Inspector controls, including a required field with no literal.
    await page.locator('.react-flow__node[data-id="b"]').click();
    if(!await page.getByTestId('task-world-inspector').isVisible())await page.getByRole('button',{name:'节点属性',exact:true}).click();
    await page.getByLabel('layer_name *',{exact:true}).waitFor();
    for(const field of ['dataset_id','rows']){
      await page.getByRole('combobox',{name:`参数来源 ${field}`,exact:true}).selectOption('upstream');
      const source=page.getByRole('combobox',{name:`上游节点 ${field}`,exact:true});
      assert.deepEqual(await source.locator('option').evaluateAll(options=>options.map(o=>o.value).filter(Boolean)),['a'],'UI contains direct incoming nodes only');
      await source.selectOption('a');
      const output=page.getByRole('combobox',{name:`输出字段 ${field}`,exact:true});await output.waitFor();
      await until(()=>output.locator('option').evaluateAll(options=>options.map(o=>o.value)),o=>o.includes(JSON.stringify(['structured',field])),'actual structured output field');
      await output.selectOption(JSON.stringify(['structured',field]));
    }
    await page.getByRole('button',{name:'保存语义',exact:true}).click();
    await until(async()=>(await api('GET',chain)).graph,g=>g.nodes.find(n=>n.id==='b').input.capability_bindings?.length===2,'bindings persisted');
    const expectedBindings=[binding('dataset_id'),binding('rows')];
    await page.reload({waitUntil:'domcontentloaded'});await page.locator('.react-flow__node[data-id="b"]').click();
    if(!await page.getByTestId('task-world-inspector').isVisible())await page.getByRole('button',{name:'节点属性',exact:true}).click();
    await until(()=>page.getByRole('combobox',{name:'参数来源 dataset_id',exact:true}).inputValue(),v=>v==='upstream','binding reload');
    assert.equal(await page.getByRole('combobox',{name:'上游节点 dataset_id',exact:true}).inputValue(),'a');
    assert.equal(await page.getByRole('combobox',{name:'输出字段 dataset_id',exact:true}).inputValue(),JSON.stringify(['structured','dataset_id']));
    const ready=await capture('bindings-reloaded');assert.ok(ready.nodes.every(n=>n.width===240&&n.height===128));
    for(const field of ['dataset_id','rows']){
      const control=page.locator(`[data-testid="input-source"][data-target-path='${JSON.stringify([field])}']`).first();
      await control.scrollIntoViewIfNeeded();await control.screenshot({path:join(evidenceDir,`binding-field-${field}.png`)});
    }
    await page.screenshot({path:join(evidenceDir,'binding-inspector.png')});
    const saved=(await api('GET',chain)).graph;assert.deepEqual(saved.nodes.find(n=>n.id==='b').input.capability_bindings,expectedBindings);
    await save('binding-graph.json',saved);

    // Forged non-dependency update and dependency/node removals are atomic refusals.
    const b=saved.nodes.find(n=>n.id==='b');
    const forged=await api('PUT',`${chain}/nodes/b`,{expected_revision:saved.revision,kind:b.kind,title:b.title,retry_policy:b.retry_policy,input:{...b.input,capability_bindings:[{...binding('dataset_id'),source:{...binding('dataset_id').source,node_id:'unconnected'}}]}},400);
    assert.match(forged.message,/input_binding_source_not_dependency/);
    for(const path of [`${chain}/edges/a/b`,`${chain}/nodes/a`])assert.match((await api('DELETE',path,{expected_revision:saved.revision},400)).message,/input_binding_source_not_dependency/);
    assert.deepEqual((await api('GET',chain)).graph,saved);await save('edge-node-integrity.json',{status:'passed',revision_unchanged:true,bindings_preserved:true,forged_source_rejected:true});

    // Resolve before approval, then rerun upstream while that approval is pending.
    await page.getByRole('button',{name:'运行选中节点',exact:true}).click();await waitExecution(chain,'b','waiting_approval');
    const oldB=await pending('b');const expectedFirst={dataset_id:'ds-001',layer_name:'灾情图层',rows:42};assert.deepEqual(oldB.arguments,expectedFirst);assert.equal(bCalls().length,0);
    const frozenB=(await records(chain,'b')).execution_snapshot;assert.equal(frozenB.context.dependency_outputs.a.structured.dataset_id,'ds-001');
    assert.deepEqual(await capture('b-waiting'),ready);
    await api('POST',`${chain}/rerun`,{node_id:'a',expected_revision:(await detail(chain)).revision});
    assert.equal((await waitExecution(chain,'b','waiting_approval')).execution_id,frozenB.id);
    await runA(chain);const secondA=await records(chain,'a');assert.equal(secondA.latest_successful_output.structured.dataset_id,'ds-002');
    const stillFrozen=(await records(chain,'b')).execution_snapshot;assert.deepEqual(stillFrozen.context,frozenB.context);assert.equal(stillFrozen.approval_ref,oldB.approval_id);
    await approve(oldB,true);await waitExecution(chain,'b','succeeded');assert.deepEqual(bCalls()[0].args,expectedFirst);
    await api('POST',`/api/approvals/${oldB.approval_id}/approve`,{},409);assert.equal(bCalls().length,1);
    assert.deepEqual(await capture('b-old-approval-succeeded'),ready);
    await api('POST',`${chain}/rerun`,{node_id:'b',expected_revision:(await detail(chain)).revision});
    await start(chain,'b');await waitExecution(chain,'b','waiting_approval');const newB=await pending('b');
    const expectedSecond={...expectedFirst,dataset_id:'ds-002'};assert.deepEqual(newB.arguments,expectedSecond);
    await approve(newB);await waitExecution(chain,'b','succeeded');assert.deepEqual(bCalls()[1].args,expectedSecond);
    const oldHistory=(await records(chain,'b',frozenB.id)).execution_snapshot;assert.deepEqual(oldHistory.context,frozenB.context);
    const newHistory=(await records(chain,'b')).execution_snapshot;assert.equal(newHistory.context.dependency_outputs.a.structured.dataset_id,'ds-002');
    await save('resolved-arguments.json',{first_approval:oldB.arguments,second_approval:newB.arguments,first_remote:bCalls()[0].args,second_remote:bCalls()[1].args});

    for(const [scenario,bindings,code] of [
      ['type_mismatch',expectedBindings,'input_binding_type_mismatch'],
      ['normal',[binding('dataset_id',['structured','missing'])],'input_binding_path_missing'],
      ['text_only',[binding('dataset_id')],'input_binding_path_missing']]){
      const path=await create(scenario,bindings);await runA(path);
      if(scenario==='text_only')assert.equal((await records(path,'a')).latest_successful_output.structured,null,'JSON text cannot become structure');
      const before=bCalls().length;await start(path,'b');const failed=await waitExecution(path,'b','failed');assert.equal(failed.failure_code,code);assert.equal(bCalls().length,before);assert.equal(failed.approval_ref,null);
      timeline.push({scenario,code,remote_calls_added:0});
    }
    for(const scenario of ['large','deep']){
      const path=await create(scenario,[]);await start(path,'a');await waitExecution(path,'a','waiting_approval');await approve(await pending('a'));
      const failed=await waitExecution(path,'a','failed');assert.equal(failed.failure_code,'capability_execution_failed');assert.equal((await records(path,'a')).latest_successful_output,null);
      timeline.push({scenario,structured_bounded_fail:true});
    }
    // Binding does not bypass the persistent resource Deny or grant revalidation.
    const grantPath=await create('normal',expectedBindings);await runA(grantPath);await start(grantPath,'b');await waitExecution(grantPath,'b','waiting_approval');
    const approval=await pending('b');const deny=await api('POST','/api/security/grants',{permission_id:'mcp.invoke',effect:'deny',resource:{type:'mcp',server_id:server.id,tool_name:'create_layer'}});grants.add(deny.id);
    const beforeDenied=bCalls().length;await approve(approval);assert.equal((await waitExecution(grantPath,'b','failed')).failure_code,'security_denied');assert.equal(bCalls().length,beforeDenied);
    await api('DELETE',`/api/security/grants/${deny.id}`);grants.delete(deny.id);
    const audit=await api('GET','/api/security/audit?limit=500');assert.ok(!JSON.stringify(audit).includes('dependency_outputs'));
    await save('security-audit.json',audit);
    const publicSurfaces=JSON.stringify({graph:saved,audit,outputs:snapshots});assert.ok(!publicSurfaces.includes(remote.url));assert.ok(!publicSurfaces.includes('env_secret_refs'));
    await save('secret-scan.json',{status:'passed',exposure_count:0,transport_env_absent:true,dependency_snapshot_not_in_audit:true});
    return {status:'passed',backend:'REAL PRODUCT BACKEND',mcp:'LOCAL MCP FIXTURE',structured_preserved:true,text_only_not_parsed:true,structured_bounds:true,direct_dependency_binding:true,ui_direct_filter:true,ui_save_reload:true,type_mismatch_zero:true,missing_path_zero:true,edge_node_integrity:true,rerun_snapshot:true,approval_frozen_arguments:true,approve_exactly_once:true,u3c_resource_deny_zero:true,geometry_stable:true,invalid_binding_remote_calls:0,db_migration:'NO'};
  } finally {
    try{for(const id of grants)await api('DELETE',`/api/security/grants/${id}`);const remaining=(await api('GET','/api/security/grants')).grants;assert.deepEqual(remaining,baselineGrants);await save('grant-cleanup.json',{status:'passed',remaining_disposable_count:0});}
    finally {await save('execution-snapshots.json',snapshots);await save('approval-timeline.json',timeline);await save('fixture-calls.json',remote.calls);await save('fixture-log.json',remote.log);await save('geometry.json',geometries);await save('network.json',network);await remote.close();}
  }
}
