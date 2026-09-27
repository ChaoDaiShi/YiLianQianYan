import assert from 'node:assert/strict';
import {createServer} from 'node:http';
import {randomUUID} from 'node:crypto';
import {writeFile} from 'node:fs/promises';
import {join} from 'node:path';

// Only the external provider is a deterministic, explicitly labelled fixture.
// The app backend, its HTTP client, OS SecretStore and SQLite stay real.
export async function protocolFixture() {
  const state={expected:'',reject:false,calls:[]};
  const server=createServer(async(req,res)=>{
    let raw='';for await(const chunk of req)raw+=chunk;
    const body=JSON.parse(raw||'{}');
    const authenticated=req.headers.authorization===`Bearer ${state.expected}`;
    state.calls.push({at:Date.now(),path:req.url,authenticated,stream:!!body.stream,forced_rejection:state.reject});
    if(!authenticated||state.reject){res.writeHead(401,{'Content-Type':'application/json'});res.end(JSON.stringify({error:{message:'Local fixture rejected credential'}}));return;}
    const content='LOCAL_FIXTURE_RESPONSE：本地协议验收，无外部模型调用。';
    if(body.stream){
      res.writeHead(200,{'Content-Type':'text/event-stream'});
      res.write(`data: ${JSON.stringify({id:'fixture',object:'chat.completion.chunk',choices:[{index:0,delta:{role:'assistant',content},finish_reason:null}]})}\n\n`);
      res.write(`data: ${JSON.stringify({id:'fixture',choices:[{index:0,delta:{},finish_reason:'stop'}],usage:{prompt_tokens:1,completion_tokens:1,total_tokens:2}})}\n\n`);
      res.end('data: [DONE]\n\n');
    }else{
      res.writeHead(200,{'Content-Type':'application/json'});
      res.end(JSON.stringify({id:'fixture',object:'chat.completion',choices:[{index:0,message:{role:'assistant',content},finish_reason:'stop'}],usage:{prompt_tokens:1,completion_tokens:1,total_tokens:2}}));
    }
  });
  await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
  return {state,url:`http://127.0.0.1:${server.address().port}/v1`,close:()=>new Promise(resolve=>server.close(resolve))};
}

export async function verifyWorkbenchFlows({page,frontendPort,backendPort,controlToken,evidenceDir}) {
  const checks=[];
  const origin=`http://127.0.0.1:${frontendPort}`;
  const api=async(method,path,body)=>{
    const response=await fetch(`http://127.0.0.1:${backendPort}${path}`,{method,headers:{'Content-Type':'application/json','X-Yilian-Control-Session':controlToken},body:body===undefined?undefined:JSON.stringify(body)});
    assert.ok(response.ok,`${method} ${path}: ${response.status}`);
    const text=await response.text();return text?JSON.parse(text):null;
  };
  const button=(name,root=page)=>root.getByRole('button',{name,exact:true});
  const until=async(read,predicate,label)=>{
    const deadline=Date.now()+15000;do{const value=await read();if(predicate(value))return value;await new Promise(resolve=>setTimeout(resolve,100));}while(Date.now()<deadline);throw new Error(label);
  };
  const reach=async locator=>{await locator.scrollIntoViewIfNeeded();assert.ok(await locator.evaluate(el=>{const r=el.getBoundingClientRect();return el.contains(document.elementFromPoint(r.x+r.width/2,r.y+r.height/2));}));};
  const fixture=await protocolFixture();
  const firstKey=`u2-disposable-${randomUUID()}`,secondKey=`u2-disposable-${randomUUID()}`;
  const label=`U2 local ${randomUUID().slice(0,8)}`;
  let profileId,expectedProviderErrors=0,complete=false;
  const responseListener=response=>{if(response.url().endsWith('/api/providers/model/verify')&&response.status()===400)expectedProviderErrors++;};
  page.on('response',responseListener);
  try {
    await page.setViewportSize({width:1280,height:720});
    await page.goto(`${origin}/settings?section=model`,{waitUntil:'domcontentloaded'});
    const basic=page.locator('.u2-model-basic');
    await basic.getByLabel('模型服务').waitFor();
    assert.equal(await basic.getByLabel('API 地址',{exact:true}).isVisible(),false,'advanced closed');
    const settings=await api('GET','/api/settings');
    assert.equal(settings.model.api_key||'','');
    assert.equal(settings.provider_readiness.model.configured,false,'isolated baseline unconfigured');
    await basic.getByLabel('模型服务').selectOption('custom');
    await basic.getByLabel('API 地址',{exact:true}).fill(fixture.url);
    await basic.getByLabel('模型名称',{exact:true}).fill('u2-default-unused');
    await reach(button('保存设置'));
    const basicSaved=page.waitForResponse(r=>r.url().endsWith('/api/settings')&&r.request().method()==='PUT');
    await button('保存设置').click();assert.ok((await basicSaved).ok());
    assert.equal((await api('GET','/api/settings')).model.api_key||'','');
    checks.push({settings:'unconfigured, advanced toggle, custom URL retained, empty default key, reachable save'});

    await page.locator('.u2-profile-management > summary').click();
    const form=page.getByTestId('model-profile-form');
    await form.locator('select').selectOption('custom');
    await form.getByLabel('显示名称',{exact:true}).fill(label);
    await form.getByLabel('模型名称',{exact:true}).fill('u2-local-fixture');
    await form.getByLabel('API 地址',{exact:true}).fill(fixture.url);
    await form.getByLabel('API Key',{exact:true}).fill(firstKey);
    const created=page.waitForResponse(r=>r.url().endsWith('/api/llm/models')&&r.request().method()==='POST');
    await button('添加模型',form).click();
    const response=await created;assert.ok(response.ok());profileId=(await response.json()).id;assert.ok(profileId);
    const card=()=>page.locator(`[data-model-id="${profileId}"]`);
    await card().waitFor();
    const read=async()=>{const all=await api('GET','/api/llm/models');assert.ok(!JSON.stringify(all).includes(firstKey)&&!JSON.stringify(all).includes(secondKey),'GET never returns plaintext');return all.find(m=>m.id===profileId);};
    assert.equal((await read()).api_key_configured,true);
    const verify=async()=>{
      const response=page.waitForResponse(r=>r.url().endsWith(`/api/llm/models/${profileId}/verify`));
      await button('验证',card()).click();assert.ok((await response).ok());
    };
    fixture.state.expected=firstKey;await verify();
    // Omitting a key is a real API write; only this unique disposable profile is touched.
    const current=await read();
    await api('PUT',`/api/llm/models/${profileId}`,{provider:current.provider,label:current.label,model:current.model,base_url:current.base_url,api_format:current.api_format,api_key_env:'',temperature:0,max_tokens:1024,invoke_timeout_ms:3000});
    await verify();
    await button(`编辑${label}`,card()).click();
    await form.getByLabel('温度',{exact:true}).fill('0.1');
    const edited=page.waitForResponse(r=>r.url().endsWith(`/api/llm/models/${profileId}`)&&r.request().method()==='PUT');
    await button('保存模型',form).click();
    const editResponse=await edited;assert.ok(editResponse.ok());
    assert.equal(editResponse.request().postDataJSON().api_key,'');
    await verify();
    await button(`编辑${label}`,card()).click();
    await button('替换密钥',form).click();
    await form.getByLabel('API Key',{exact:true}).fill(secondKey);
    const replaced=page.waitForResponse(r=>r.url().endsWith(`/api/llm/models/${profileId}`)&&r.request().method()==='PUT');
    await button('保存模型',form).click();assert.ok((await replaced).ok());
    fixture.state.expected=secondKey;await verify();
    assert.equal((await read()).api_key_configured,true);
    checks.push({secrets:'unique profile only: GET redacted, omitted KEEP, empty KEEP, nonempty REPLACE',provider_fixture:'all authenticated with expected disposable key'});
    await button('激活',card()).click();
    await until(read,m=>m.active,'profile activation');
    await page.goto(`${origin}/settings?section=model`,{waitUntil:'domcontentloaded'});
    await page.getByText('已保存 · 未测试',{exact:true}).waitFor();
    await button('测试连接').click();await page.getByText('连接正常',{exact:true}).first().waitFor();
    fixture.state.reject=true;
    await button('测试连接').click();await page.getByText('当前不可用',{exact:true}).waitFor();
    fixture.state.reject=false;
    await button('测试连接').click();await page.getByText('连接正常',{exact:true}).first().waitFor();
    await page.screenshot({path:join(evidenceDir,'settings-verified-local-profile.png')});
    checks.push({provider_status:'saved != available; actual provider verify success, rejection, recovery',expected_provider_errors:expectedProviderErrors});

    await page.goto(`${origin}/chat`,{waitUntil:'domcontentloaded'});
    await button('新建任务').click();
    const message='U2 本地验收：请返回协议测试文本，不访问外部服务。';
    await page.locator('.composer-card textarea').fill(message);
    await reach(page.getByTitle('选择、拖入或粘贴文件（最多 8 个，每个 25 MiB）'));
    await reach(button('与小涟语音对话'));
    await button('发送').click();
    await page.locator('.conversation-message-assistant').filter({hasText:'LOCAL_FIXTURE_RESPONSE'}).waitFor({timeout:15000});
    await page.reload({waitUntil:'domcontentloaded'});
    await page.locator('.conversation-row').first().click();
    await page.locator('.conversation-message-assistant').filter({hasText:'LOCAL_FIXTURE_RESPONSE'}).waitFor();
    const chatStyles=await page.evaluate(()=>Object.fromEntries(['.workbench-grid','.composer-card','.conversation-message-assistant'].map(selector=>{
      const style=getComputedStyle(document.querySelector(selector));return [selector,{background:style.backgroundColor,image:style.backgroundImage,blur:style.backdropFilter,shadow:style.boxShadow,border:style.borderTopWidth}];
    })));
    assert.equal(chatStyles['.workbench-grid'].image,'none','conversation uses the quiet work surface');
    assert.equal(chatStyles['.composer-card'].blur,'none');
    assert.equal(chatStyles['.composer-card'].border,'1px');
    assert.equal(chatStyles['.conversation-message-assistant'].background,'rgba(0, 0, 0, 0)');
    checks.push({persisted_conversation_computed_style:chatStyles});
    await button('与小涟语音对话').click();
    await page.locator('[data-testid="voice-pill"]').waitFor();
    const composer=await page.locator('.composer-card').boundingBox(),voice=await page.locator('[data-testid="voice-pill"]').boundingBox();
    assert.ok(composer&&voice&&composer.y+composer.height<=voice.y,'voice safe area protects composer');
    await page.screenshot({path:join(evidenceDir,'chat-persisted-local-message-and-voice.png')});
    checks.push({chat:'created, sent via real SSE to labelled loopback fixture, reopened persisted reply',file_voice_send_reachable:true,voice_no_collision:true});

    await page.goto(`${origin}/capabilities`,{waitUntil:'domcontentloaded'});
    await page.locator('.capability-list-row').first().click();
    await page.getByRole('heading',{name:'来源与版本'}).waitFor();
    await page.locator('.u2-managed-imports > summary').click();
    const imports=page.getByLabel('托管能力导入');
    await imports.getByLabel('稳定 ID',{exact:true}).fill('u2-local-note');
    await imports.getByLabel('名称',{exact:true}).fill('U2 本地说明');
    await imports.getByLabel('Skill Markdown（仅作为数据预览）',{exact:true}).fill('# U2 本地说明\nOnly describes this local fixture. No shell commands or external sources.');
    await button('检查并生成预览',imports).click();
    await button('确认安装或更新',imports).click();
    const item=imports.locator('article').filter({hasText:'U2 本地说明'});
    await item.waitFor();
    await button('启用声明',item).click();await button('禁用',item).waitFor();
    await button('刷新能力').click();
    await page.getByLabel('搜索能力').fill('U2 本地说明');
    await page.locator('.capability-list-row').filter({hasText:'U2 本地说明'}).click();
    await page.screenshot({path:join(evidenceDir,'capability-real-local-skill.png')});
    await button('禁用',item).click();await button('启用声明',item).waitFor();
    checks.push({capability:'real builtin card/detail plus local Markdown import preview, confirm, enable, discovery and disable',remote_code_installed:false});

    await page.goto(`${origin}/settings?section=model`,{waitUntil:'domcontentloaded'});
    await page.locator('.u2-profile-management > summary').click();await card().waitFor();
    page.once('dialog',dialog=>dialog.accept());
    await button('清除密钥',card()).click();
    await until(read,m=>!m.api_key_configured,'explicit clear deletes unique profile secret');
    checks.push({explicit_clear:'DELETE verified on disposable profile only'});
    assert.ok(fixture.state.calls.every(c=>c.authenticated),'every provider call uses the expected key');
    complete=true;
    return {checks,expectedProviderErrors};
  } finally {
    page.off('response',responseListener);
    try {
      if(profileId){
        await api('DELETE',`/api/llm/models/${profileId}`);
        assert.ok(!(await api('GET','/api/llm/models')).some(model=>model.id===profileId));
        checks.push({disposable_profile_removed:true});
      }
      await writeFile(join(evidenceDir,'flow-checks.json'),JSON.stringify({status:complete?'passed':'failed',checks,provider_calls:fixture.state.calls,provider_kind:'loopback protocol fixture, not a real cloud model',first_token_timing:'immediate provider response; no history wait or artificial delay'},null,2));
    }
    finally { await fixture.close(); }
  }
}
