import assert from 'node:assert/strict';
import {createServer} from 'node:http';
import {writeFile} from 'node:fs/promises';
import {join} from 'node:path';

// LOCAL MCP FIXTURE. Requests reach the real product transport and Gateway.
export async function fixture(name, resourceTools = false) {
  const calls=[], log=[];
  const basic={type:'object',properties:{text:{type:'string',description:'待处理文本'}},required:['text']};
  const tools=[
    {name:'echo',description:'LOCAL MCP FIXTURE echo',inputSchema:basic},
    {name:'transform',description:'LOCAL MCP FIXTURE transform',inputSchema:{type:'object',properties:{text:{type:'string'},count:{type:'integer',minimum:1,maximum:5},enabled:{type:'boolean'},mode:{type:'string',enum:['upper','lower']},factor:{type:'number'},options:{type:'object',properties:{suffix:{type:'string'}}}},required:['text','count','enabled','mode','factor']}},
    {name:'complex',description:'LOCAL MCP FIXTURE complex JSON',inputSchema:{type:'object',properties:{items:{type:'array',items:{type:'string'}}},oneOf:[{required:['items']},{required:['other']}]}},
    {name:'header_bound',description:'LOCAL MCP FIXTURE unsupported secure input',inputSchema:{type:'object',properties:{credential:{type:'string','x-mcp-header':'Authorization'}}}},
  ];
  if (resourceTools) tools.push(...['read', 'publish'].map(name => ({name, description:`LOCAL MCP FIXTURE ${name}`, inputSchema:basic})));
  const server=createServer(async(req,res)=>{
    let body='';for await(const part of req)body+=part;
    const rpc=JSON.parse(body||'{}');log.push({method:rpc.method,at:Date.now()});
    let result={};
    if(rpc.method==='server/discover')result={supportedVersions:['2026-07-28','2025-11-25'],serverInfo:{name,version:'1'},capabilities:{tools:true}};
    if(rpc.method==='tools/list')result={tools};
    if(rpc.method==='tools/call'){
      const {name:tool,arguments:args}=rpc.params;calls.push({tool,args});
      const value=tool==='transform'?{text:(args.mode==='upper'?args.text.toUpperCase():args.text.toLowerCase()).repeat(args.count),enabled:args.enabled,factor:args.factor,options:args.options}:args;
      result={content:[{type:'text',text:JSON.stringify({fixture:name,value})}],isError:false,resultType:'complete'};
    }
    res.writeHead(200,{'Content-Type':'application/json'});res.end(JSON.stringify({jsonrpc:'2.0',id:rpc.id,result}));
  });
  await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
  return {name,calls,log,url:`http://127.0.0.1:${server.address().port}/mcp`,close:()=>new Promise(resolve=>server.close(resolve))};
}

export async function mcpCanvasU3({page,backendPort,frontendPort,controlToken,evidenceDir}) {
  const A=await fixture('LOCAL MCP FIXTURE A'),B=await fixture('LOCAL MCP FIXTURE B');
  const timeline=[],geometries=[],network=[];
  const save=(name,value)=>writeFile(join(evidenceDir,name),JSON.stringify(value,null,2));
  const api=async(method,path,body,expected)=>{
    const r=await fetch(`http://127.0.0.1:${backendPort}${path}`,{method,headers:{'Content-Type':'application/json','X-Yilian-Control-Session':controlToken},body:body===undefined?undefined:JSON.stringify(body)});
    const text=await r.text();network.push({method,path,status:r.status});
    if(expected)assert.equal(r.status,expected,text);else assert.ok(r.ok,`${path}: ${r.status} ${text}`);
    try{return JSON.parse(text);}catch{return text;}
  };
  const until=async(read,accept,label)=>{let last;const end=Date.now()+20000;do{last=await read();if(accept(last))return last;await new Promise(r=>setTimeout(r,100));}while(Date.now()<end);throw new Error(`${label}: ${JSON.stringify(last)}`);};
  const button=name=>page.getByRole('button',{name,exact:true});
  const graphId='u3-canvas',path=`/api/task-world/graphs/${graphId}`;
  const graph=async()=>(await api('GET',path)).graph;
  const detail=async()=>(await api('GET',`${path}/detail`)).detail;
  const geometry=()=>page.locator('.react-flow').evaluate(el=>({camera:el.querySelector('.react-flow__viewport').style.transform,nodes:[...el.querySelectorAll('.react-flow__node')].map(n=>({id:n.dataset.id,transform:n.style.transform,width:n.offsetWidth,height:n.offsetHeight}))}));
  const snapshot=async(label)=>{const g=await geometry();geometries.push({label,...g});await page.screenshot({path:join(evidenceDir,`${label}.png`)});return g;};
  const execution=async(nodeId,status)=>until(async()=>(await detail()).nodes.find(n=>n.id===nodeId)?.latest_execution,e=>e?.status===status,`execution ${status}`);
  const pending=async(nodeId)=>until(async()=>(await api('GET','/api/approvals/pending')).approvals,a=>a.some(p=>p.task_node_id===nodeId),'approval').then(a=>a.find(p=>p.task_node_id===nodeId));
  const decide=async(approval,allow)=>{
    const approvalPage=await page.context().newPage();
    try {
      await approvalPage.goto(`http://127.0.0.1:${frontendPort}/system?card=approvals`,{waitUntil:'domcontentloaded'});
      const card=approvalPage.locator(`[data-approval-id="${approval.approval_id}"][data-testid="task-canvas-approval"]`);
      await card.waitFor();assert.match(await card.innerText(),/来源：任务画布/);
      await approvalPage.screenshot({path:join(evidenceDir,allow?'approval-allow.png':'approval-reject.png')});
      await card.getByRole('button',{name:allow?'允许此次工具操作':'拒绝此次工具操作'}).click();
      await card.waitFor({state:'detached'});
      timeline.push({decision:allow?'approve':'reject',approval_id:approval.approval_id,calls:A.calls.length+B.calls.length});
    } finally {await approvalPage.close();}
  };
  try {
    const servers=[];
    for(const f of [A,B])servers.push(await api('POST','/api/plugins/mcp',{name:f.name,transport:'streamable_http',url:f.url}));
    await api('POST','/api/capabilities/refresh');
    const discovery=await api('GET','/api/capabilities?provider=mcp&limit=200');await save('capability-discovery.json',discovery);
    const tools=discovery.capabilities;
    const tool=(name,source='B')=>tools.find(t=>t.name===name&&t.metadata.source_name===`LOCAL MCP FIXTURE ${source}`);
    assert.ok(tool('transform'));assert.notEqual(tool('echo','A').id,tool('echo','B').id);
    await page.goto(`http://127.0.0.1:${frontendPort}/capabilities`,{waitUntil:'domcontentloaded'});
    await page.getByLabel('Provider',{exact:true}).selectOption('mcp');
    await page.locator('.capability-list-row').filter({hasText:'transform'}).first().click();
    await page.locator('.capability-detail-panel').getByText(/LOCAL MCP FIXTURE [AB]/).first().waitFor();
    await page.screenshot({path:join(evidenceDir,'capability-page.png')});
    await api('POST','/api/task-world/graphs',{id:graphId,nodes:[{id:'anchor',kind:'work',title:'普通任务',input:{instruction:'保持位置'},retry_policy:{max_attempts:1}}],edges:[]});
    await page.goto(`http://127.0.0.1:${frontendPort}/task-world/${graphId}`,{waitUntil:'domcontentloaded'});
    await page.locator('.react-flow__node[data-id="anchor"]').waitFor();await snapshot('before-mcp');
    await button('从能力添加').click();
    const header=page.locator(`[data-capability-id="${tool('header_bound').id}"]`);
    assert.equal(await header.getByRole('button').isDisabled(),true);assert.match(await header.innerText(),/安全请求头/);
    await page.locator(`[data-capability-id="${tool('transform').id}"]`).getByRole('button').click();
    await page.getByLabel('text *',{exact:true}).fill('hello');
    await page.getByLabel('count *',{exact:true}).fill('2');
    await page.getByLabel('enabled *',{exact:true}).selectOption('true');
    await page.getByLabel('mode *',{exact:true}).selectOption('"upper"');
    await page.getByLabel('factor *',{exact:true}).fill('1.5');
    await page.getByLabel('suffix',{exact:true}).fill('!');
    await button('保存语义').click();
    const saved=await until(graph,g=>g.nodes.some(n=>n.input?.capability_input?.text==='hello'),'persisted args');
    const node=saved.nodes.find(n=>n.input?.capability_input?.text==='hello');
    const expected={text:'hello',count:2,enabled:true,mode:'upper',factor:1.5,options:{suffix:'!'}};
    assert.deepEqual(node.input.capability_input,expected);assert.equal(A.calls.length+B.calls.length,0);
    await page.reload({waitUntil:'domcontentloaded'});await page.locator(`[data-id="${node.id}"].react-flow__node`).click();await button('节点属性').click();
    await until(()=>page.getByLabel('text *',{exact:true}).inputValue(),v=>v==='hello','reloaded args');
    assert.equal(await page.getByLabel('count *',{exact:true}).inputValue(),'2');
    const ready=await snapshot('ready');
    await button('运行选中节点').click();const waiting=await execution(node.id,'waiting_approval');
    const reject=await pending(node.id);assert.equal(A.calls.length+B.calls.length,0);
    timeline.push({state:'waiting_approval',execution:waiting,calls:0});assert.deepEqual(await snapshot('waiting-approval'),ready);
    await decide(reject,false);await execution(node.id,'failed');assert.equal(A.calls.length+B.calls.length,0);assert.deepEqual(await snapshot('rejected'),ready);
    await page.locator('summary').filter({hasText:'状态与执行记录'}).click();
    await button('从此节点重跑').click();
    await until(detail,d=>d.nodes.find(n=>n.id===node.id)?.latest_execution?.status==='stale','ready for rerun');
    await button('运行选中节点').click();await execution(node.id,'waiting_approval');
    const approval=await pending(node.id);await decide(approval,true);
    const done=await execution(node.id,'succeeded');assert.equal(B.calls.length,1);assert.deepEqual(B.calls[0].args,expected);assert.equal(A.calls.length,0);
    assert.equal(done.validation.status,'accepted');assert.match(done.result_summary,/HELLOHELLO/);
    assert.deepEqual(await snapshot('succeeded'),ready);
    await api('POST',`/api/approvals/${approval.approval_id}/approve`,{},409);assert.equal(B.calls.length,1);
    await save('task-execution.json',{detail:await detail(),graph:await graph()});
    // Changing the picker draft inside Inspector must not submit its form.
    const draftRevision=(await graph()).revision;
    await button('选择能力').click();
    await page.locator(`[data-capability-id="${tool('complex').id}"]`).getByRole('button').click();
    await page.waitForTimeout(300);
    assert.equal((await graph()).nodes.find(n=>n.id===node.id).input.executor_ref,`capability://${tool('transform').id}`,'picker selection is a draft, never an implicit save');
    assert.equal((await graph()).revision,draftRevision,'opening/selecting a capability cannot save the graph');
    assert.equal(B.calls.length,1);
    await page.reload({waitUntil:'domcontentloaded'});await page.locator('.react-flow__node').first().waitFor();

    // Independent real attempts isolate cancel and same-name routing counts.
    const add=async(id,cap)=>{const current=await detail();await api('POST',`${path}/nodes`,{expected_revision:current.revision,node:{id,kind:'work',title:id,input:{executor_ref:`capability://${cap.id}`,capability_input:{text:id}},retry_policy:{max_attempts:1}}});};
    const start=async(id)=>{const current=await detail();await api('POST',`${path}/nodes/${id}/executions`,{expected_revision:current.revision});return execution(id,'waiting_approval');};
    await add('cancel-test',tool('echo','A'));const cancelled=await start('cancel-test');const late=await pending('cancel-test');
    await api('POST',`${path}/executions/${cancelled.execution_id}/cancel`,{expected_revision:(await detail()).revision});
    await api('POST',`/api/approvals/${late.approval_id}/approve`,{},409);assert.equal(A.calls.length,0);timeline.push({state:'cancel_then_approve',execution:await execution('cancel-test','cancelled'),remote_calls:0});
    const sameNameBefore={A:A.calls.length,B:B.calls.length};
    await add('same-name-B',tool('echo','B'));await start('same-name-B');const echoApproval=await pending('same-name-B');
    const responses=await Promise.all([0,1].map(async()=>{const r=await fetch(`http://127.0.0.1:${backendPort}/api/approvals/${echoApproval.approval_id}/approve`,{method:'POST',headers:{'Content-Type':'application/json','X-Yilian-Control-Session':controlToken},body:'{}'});await r.text();return r.status;}));
    assert.deepEqual(responses.sort(),[200,409]);await execution('same-name-B','succeeded');
    assert.equal(A.calls.filter(c=>c.tool==='echo').length,0);assert.equal(B.calls.filter(c=>c.tool==='echo').length,1);
    await save('same-name-routing.json',{capability_A:tool('echo','A').id,capability_B:tool('echo','B').id,before:sameNameBefore,after:{A:A.calls.length,B:B.calls.length},scenario_calls:{A:A.calls.length-sameNameBefore.A,B:B.calls.length-sameNameBefore.B},duplicate_http_statuses:responses});

    // Complex schemas preserve unrecognized fields using explicit JSON mode.
    await button('从能力添加').click();await page.locator(`[data-capability-id="${tool('complex').id}"]`).getByRole('button').click();
    const jsonEditor=page.getByRole('textbox',{name:'JSON 参数',exact:true});await jsonEditor.fill('{broken');await button('保存语义').click();await page.getByRole('alert').filter({hasText:/JSON|Unexpected|Expected/}).first().waitFor();
    await jsonEditor.fill('{"items":["a"],"unrecognized":{"keep":true}}');await button('保存语义').click();
    await until(graph,g=>g.nodes.some(n=>n.input?.capability_input?.unrecognized?.keep===true),'complex fields preserved');
    // Backend independently rejects a forged header-bound save, before storage.
    const denied=await api('POST',`${path}/nodes`,{expected_revision:(await detail()).revision,node:{id:'unsafe',kind:'work',title:'unsafe',input:{executor_ref:`capability://${tool('header_bound').id}`,capability_input:{credential:'U3_HEADER_SENTINEL_NOT_A_SECRET'}},retry_policy:{max_attempts:1}}},400);
    assert.ok(denied);
    await api('PUT',`/api/plugins/mcp/${servers[1].id}`,{enabled:false});await api('POST','/api/capabilities/refresh');
    const r=await fetch(`http://127.0.0.1:${backendPort}${path}/nodes/${node.id}/executions`,{method:'POST',headers:{'Content-Type':'application/json','X-Yilian-Control-Session':controlToken},body:JSON.stringify({expected_revision:(await detail()).revision})});
    const unavailableBody=await r.text();assert.ok(!r.ok);assert.match(unavailableBody,/ProviderUnavailable/);assert.equal(B.calls.length,2);
    await save('unavailable.json',{http_status:r.status,response:unavailableBody,node_id:node.id,binding:node.input.executor_ref,calls_before:2,calls_after:B.calls.length});
    await page.reload({waitUntil:'domcontentloaded'});await page.locator(`[data-id="${node.id}"].react-flow__node`).waitFor();
    assert.match(await page.locator(`[data-id="${node.id}"].react-flow__node`).innerText(),/能力当前不可用/);await snapshot('unavailable-preserved');
    const audit=await api('GET','/api/security/audit?limit=500');await save('security-audit.json',audit);
    const auditText=JSON.stringify(audit);for(const type of ['policy_decided','approval_requested','approval_resolved','execution_finished','verification_finished'])assert.ok(auditText.includes(type),type);
    assert.ok(auditText.includes('mcp.invoke'));assert.ok(auditText.includes(servers[1].id));
    const responsesToScan={discovery,graph:await graph(),detail:await detail(),audit};
    const scan=JSON.stringify(responsesToScan);assert.ok(!scan.includes('U3_HEADER_SENTINEL_NOT_A_SECRET'));assert.ok(!scan.includes('env_secret_refs'));assert.ok(!scan.includes(A.url));assert.ok(!scan.includes(B.url));
    await save('secret-scan.json',{status:'passed',surfaces:['capabilities','graph','detail','audit'],header_sentinel_absent:true,transport_and_env_absent:true});
    const matrix=[];
    for(const [width,height] of [[1280,720],[1366,768],[1920,1080],[2560,1440]]){await page.setViewportSize({width,height});const b=await button('从能力添加').boundingBox();assert.ok(b&&b.x>=0&&b.y+b.height<=height);matrix.push({width,height,control:b});await page.screenshot({path:join(evidenceDir,`responsive-${width}.png`)});}
    await save('responsive.json',matrix);
    return {status:'passed',backend:'REAL PRODUCT BACKEND',mcp:'LOCAL MCP FIXTURE',discovery:true,schema_form:true,persist_reload:true,approval_required:true,reject_zero:true,approve_once:true,duplicate_once:true,cancel_then_approve_zero:true,same_name_exact_routing:true,unavailable_preserved:true,geometry_stable:true,header_fail_closed:true,secret_scan:true};
  } finally {
    await save('tool-call-counts.json',{A:A.calls,B:B.calls});await save('fixture-log.json',{A:A.log,B:B.log});await save('approval-timeline.json',timeline);await save('geometry.json',geometries);await save('u3-network.json',network);
    await Promise.all([A.close(),B.close()]);
  }
}
