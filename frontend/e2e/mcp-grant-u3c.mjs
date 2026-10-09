import assert from 'node:assert/strict';
import {randomUUID} from 'node:crypto';
import {writeFile} from 'node:fs/promises';
import {join} from 'node:path';
import {fixture} from './mcp-canvas-u3.mjs';

// All grants and servers belong to core-paths' disposable real backend DB.
// Every execution enters the real Canvas Task Harness, never a mocked gateway.
export async function mcpGrantU3c({page,backendPort,frontendPort,controlToken,evidenceDir}) {
  const A=await fixture('U3C LOCAL MCP FIXTURE A',true), B=await fixture('U3C LOCAL MCP FIXTURE B',true);
  const created=new Set(),network=[],scenarios=[];
  const save=(name,value)=>writeFile(join(evidenceDir,name),JSON.stringify(value,null,2));
  const api=async(method,path,body,expected)=>{
    const response=await fetch(`http://127.0.0.1:${backendPort}${path}`,{method,headers:{'Content-Type':'application/json','X-Yilian-Control-Session':controlToken},body:body===undefined?undefined:JSON.stringify(body)});
    const text=await response.text();network.push({method,path,status:response.status});
    if(expected)assert.equal(response.status,expected,text);else assert.ok(response.ok,`${path}: ${response.status} ${text}`);
    try{return JSON.parse(text);}catch{return text;}
  };
  const until=async(read,accept,label)=>{let value;const end=Date.now()+20000;do{value=await read();if(accept(value))return value;await new Promise(r=>setTimeout(r,100));}while(Date.now()<end);throw Error(`${label}: ${JSON.stringify(value)}`);};
  const path=`/api/task-world/graphs/u3c-${randomUUID()}`;
  const detail=async()=>(await api('GET',`${path}/detail`)).detail;
  const latest=async id=>(await detail()).nodes.find(n=>n.id===id)?.latest_execution;
  const waitExecution=(id,status)=>until(()=>latest(id),e=>e?.status===status,`${id}: ${status}`);
  const grants=async()=>(await api('GET','/api/security/grants')).grants;
  const addGrant=async(server,tool,effect)=>{
    const grant=await api('POST','/api/security/grants',{permission_id:'mcp.invoke',effect,resource:{type:'mcp',server_id:server,tool_name:tool}});
    created.add(grant.id);return grant;
  };
  const deleteGrant=async grant=>{await api('DELETE',`/api/security/grants/${grant.id}`);created.delete(grant.id);};
  const addNode=async(id,capability)=>api('POST',`${path}/nodes`,{expected_revision:(await detail()).revision,node:{id,kind:'work',title:id,input:{executor_ref:`capability://${capability.id}`,capability_input:{text:id}},retry_policy:{max_attempts:1}}});
  const start=async id=>api('POST',`${path}/nodes/${id}/executions`,{expected_revision:(await detail()).revision});
  const pending=async id=>(await until(async()=>(await api('GET','/api/approvals/pending')).approvals,a=>a.some(p=>p.task_node_id===id),`pending ${id}`)).find(p=>p.task_node_id===id);
  const approve=async approval=>api('POST',`/api/approvals/${approval.approval_id}/approve`,{});
  const counts=()=>({A:A.calls.length,B:B.calls.length});
  const checkAudit=(events,executionId,grant,decision)=>assert.ok(events.some(event=>event.event_type==='grant_evaluated'
    && event.correlation_id===executionId && event.decision_status===decision
    && event.details?.result?.matched_grant_id===grant.id
    && event.resources.some(r=>r.kind==='mcp'&&r.server_id===grant.resource.server_id)),`matching audit ${decision}/${grant.id}`);
  let editorPage;
  let baselineGrants;
  try {
    baselineGrants=await grants();
    assert.ok(baselineGrants.every(g=>g.source!=='user'),'isolated DB must start without user grants');
    const servers=[];
    for(const f of [A,B])servers.push(await api('POST','/api/plugins/mcp',{name:f.name,transport:'streamable_http',url:f.url}));
    await api('POST','/api/capabilities/refresh');
    const discovery=await api('GET','/api/capabilities?provider=mcp&limit=200');
    const tool=(name,source=A)=>discovery.capabilities.find(c=>c.name===name&&c.metadata.source_name===source.name);
    assert.ok(tool('publish'));assert.notEqual(tool('echo',A).id,tool('echo',B).id);
    await api('POST','/api/task-world/graphs',{id:path.split('/').at(-1),nodes:[],edges:[]});

    // Create an actual Deny through the existing Grant Editor, not a fake form.
    editorPage=await page.context().newPage();
    await editorPage.goto(`http://127.0.0.1:${frontendPort}/settings?section=permissions`,{waitUntil:'domcontentloaded'});
    const editor=editorPage.getByTestId('grant-editor');await editor.waitFor();
    await editor.getByLabel(/^资源类型/).selectOption('mcp');
    await editor.getByLabel(/^效果/).selectOption('deny');
    await editor.getByLabel('Server ID',{exact:true}).fill(servers[0].id);
    await editor.getByLabel('工具名（可选）',{exact:true}).fill('publish');
    const responsePromise=editorPage.waitForResponse(r=>r.url().endsWith('/api/security/grants')&&r.request().method()==='POST');
    await editor.getByRole('button',{name:'创建授权',exact:true}).click();
    const response=await responsePromise;assert.ok(response.ok());const denyPublish=await response.json();created.add(denyPublish.id);
    assert.deepEqual(denyPublish.resource,{type:'mcp',server_id:servers[0].id,tool_name:'publish'});
    await editor.getByText(`MCP：${servers[0].id} / publish`,{exact:true}).waitFor();
    await editorPage.screenshot({path:join(evidenceDir,'grant-editor-deny.png'),fullPage:true});
    const editorText=await editor.innerText();assert.ok(!editorText.includes(A.url)&&!editorText.includes('env_secret_refs'));
    const allowServer=await addGrant(servers[0].id,null,'allow');
    await addNode('publish-denied',tool('publish'));
    await page.goto(`http://127.0.0.1:${frontendPort}/task-world/${path.split('/').at(-1)}`,{waitUntil:'domcontentloaded'});
    await page.locator('.react-flow__node[data-id="publish-denied"]').click();
    await page.getByRole('button',{name:'运行选中节点',exact:true}).click();
    const denied=await waitExecution('publish-denied','failed');
    assert.equal(denied.failure_code,'security_denied');assert.deepEqual(counts(),{A:0,B:0});
    await page.locator('.react-flow__node[data-id="publish-denied"]').getByText('失败',{exact:true}).waitFor();
    await page.getByRole('button',{name:'节点属性',exact:true}).click();
    await page.locator('summary').filter({hasText:'状态与执行记录'}).click();
    await page.getByTestId('task-execution-state').getByText(/当前安全策略或授权拒绝此操作/).waitFor();
    await page.screenshot({path:join(evidenceDir,'canvas-publish-denied.png')});
    await page.getByRole('button',{name:'节点属性',exact:true}).click();
    scenarios.push({scenario:'UI deny + server-wide allow',execution:denied,remote_calls:0});

    await addNode('read-allowed',tool('read'));await start('read-allowed');
    await waitExecution('read-allowed','waiting_approval');assert.equal(A.calls.length,0);
    await approve(await pending('read-allowed'));const read=await waitExecution('read-allowed','succeeded');
    assert.deepEqual(A.calls.map(c=>c.tool),['read']);
    scenarios.push({scenario:'server-wide read still requires High approval',execution:read,remote_calls:1});

    const echoDeny=await addGrant(servers[0].id,'echo','deny');
    await addNode('echo-A-denied',tool('echo',A));await start('echo-A-denied');
    assert.equal((await waitExecution('echo-A-denied','failed')).failure_code,'security_denied');
    await addNode('echo-B-allowed',tool('echo',B));await start('echo-B-allowed');
    await waitExecution('echo-B-allowed','waiting_approval');await approve(await pending('echo-B-allowed'));
    await waitExecution('echo-B-allowed','succeeded');assert.equal(B.calls.length,1);assert.equal(A.calls.filter(c=>c.tool==='echo').length,0);
    scenarios.push({scenario:'A/echo deny does not affect B/echo',calls:counts()});
    await deleteGrant(echoDeny);

    // No allow at pending creation; add an exact deny while that approval exists.
    await deleteGrant(allowServer);
    await addNode('old-approval-new-deny',tool('echo'));await start('old-approval-new-deny');
    await waitExecution('old-approval-new-deny','waiting_approval');const oldApproval=await pending('old-approval-new-deny');
    const newDeny=await addGrant(servers[0].id,'echo','deny');const beforeDeny=counts();
    await approve(oldApproval);const oldDenied=await waitExecution('old-approval-new-deny','failed');
    assert.equal(oldDenied.failure_code,'security_denied');assert.deepEqual(counts(),beforeDeny);
    await api('POST',`/api/approvals/${oldApproval.approval_id}/approve`,{},409);assert.deepEqual(counts(),beforeDeny);
    scenarios.push({scenario:'old approval + new exact deny',execution:oldDenied,before:beforeDeny,after:counts(),remote_calls:0});
    await deleteGrant(newDeny);

    await addNode('old-approval-new-allow',tool('echo'));await start('old-approval-new-allow');
    await waitExecution('old-approval-new-allow','waiting_approval');const allowApproval=await pending('old-approval-new-allow');
    const exactAllow=await addGrant(servers[0].id,'echo','allow');const beforeAllow=counts();
    await new Promise(resolve=>setTimeout(resolve,300));assert.equal((await latest('old-approval-new-allow')).status,'waiting_approval');
    assert.equal((await pending('old-approval-new-allow')).approval_id,allowApproval.approval_id);assert.deepEqual(counts(),beforeAllow);
    await approve(allowApproval);const allowed=await waitExecution('old-approval-new-allow','succeeded');
    await api('POST',`/api/approvals/${allowApproval.approval_id}/approve`,{},409);
    assert.equal(A.calls.length,beforeAllow.A+1);assert.equal(B.calls.length,beforeAllow.B);
    scenarios.push({scenario:'new allow never auto-consumes pending approval',execution:allowed,before:beforeAllow,after:counts(),remote_calls:1});
    await deleteGrant(exactAllow);

    // Remove precisely the UI-created grant through UI; publish can recover.
    editorPage.once('dialog',dialog=>dialog.accept());
    const row=editor.getByText(`MCP：${servers[0].id} / publish`,{exact:true}).locator('..').locator('..');
    await row.getByRole('button',{name:'删除',exact:true}).click();
    await until(grants,g=>!g.some(x=>x.id===denyPublish.id),'UI grant delete');created.delete(denyPublish.id);
    await addNode('publish-recovered',tool('publish'));await start('publish-recovered');
    await waitExecution('publish-recovered','waiting_approval');await approve(await pending('publish-recovered'));
    const recovered=await waitExecution('publish-recovered','succeeded');assert.equal(A.calls.filter(c=>c.tool==='publish').length,1);
    await page.reload({waitUntil:'domcontentloaded'});await page.locator('.react-flow__node[data-id="publish-recovered"]').waitFor();
    await page.screenshot({path:join(evidenceDir,'canvas-publish-recovered.png')});
    scenarios.push({scenario:'UI delete restores publish approval flow',execution:recovered,remote_calls:1});
    assert.deepEqual(await grants(),baselineGrants,'one-shot approvals must not persist grants');

    const audit=await api('GET','/api/security/audit?limit=500');
    const events=audit.events;
    checkAudit(events,denied.execution_id,denyPublish,'deny');
    checkAudit(events,read.execution_id,allowServer,'allow');
    checkAudit(events,oldDenied.execution_id,newDeny,'deny');
    checkAudit(events,allowed.execution_id,exactAllow,'allow');
    const surfaces={discovery,detail:await detail(),audit};
    const serialized=JSON.stringify(surfaces);
    for(const secret of [A.url,B.url,'env_secret_refs','headers_from_env'])assert.ok(!serialized.includes(secret));
    await save('security-audit.json',audit);await save('execution-detail.json',surfaces.detail);
    await save('secret-scan.json',{status:'passed',exposure_count:0,surfaces:['capabilities','grant editor','execution detail','audit'],transport_url_absent:true,env_headers_absent:true});
    return {status:'passed',backend:'REAL PRODUCT BACKEND',mcp:'LOCAL MCP FIXTURE',exact_allow:true,exact_deny:true,server_wide_allow:true,deny_precedence:true,cross_server_isolation:true,old_approval_new_deny:true,new_allow_explicit_approval_once:true,ui_create_delete_recovery:true,ui_denial_explanation:true,matched_grant_audit:true,denied_remote_call_count:0,secret_exposure:0,persistent_grants_after_approval:0};
  } finally {
    try {
      for(const id of created)await api('DELETE',`/api/security/grants/${id}`);
      const remaining=await grants();
      assert.deepEqual(remaining,baselineGrants,'preserve seeded grants and remove every disposable grant');
      await save('grant-cleanup.json',{status:'passed',remaining_disposable_count:0,preserved_baseline_count:remaining.length});
    } finally {
      await save('scenarios.json',scenarios);await save('tool-call-counts.json',{A:A.calls,B:B.calls});await save('network.json',network);
      if(editorPage)await editorPage.screenshot({path:join(evidenceDir,'grant-editor-final.png'),fullPage:true}).catch(()=>{});
      await editorPage?.close();await Promise.all([A.close(),B.close()]);
    }
  }
}
