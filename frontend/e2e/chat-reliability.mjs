import assert from 'node:assert/strict';
import {randomUUID} from 'node:crypto';
import {writeFile} from 'node:fs/promises';
import {join} from 'node:path';
import {protocolFixture} from './workbench-flows.mjs';

export async function chatReliability({page,frontendPort,backendPort,controlToken,evidenceDir}){
 const timeline=[],cases=[],releases=[],snapshots=[];
 const note=(type,data={})=>timeline.push({at:Date.now(),type,...data});
 const api=async(method,path,body)=>{
  const response=await fetch(`http://127.0.0.1:${backendPort}${path}`,{method,headers:{'Content-Type':'application/json','X-Yilian-Control-Session':controlToken},body:body===undefined?undefined:JSON.stringify(body)});
  assert.ok(response.ok,`${method} ${path}: ${response.status}`);const text=await response.text();return text?JSON.parse(text):null;
 };
 const fixture=await protocolFixture();fixture.state.expected=`r1-disposable-${randomUUID()}`;
 let profile,forced=false,success=false;
 const seen=new Set();
 await page.addInitScript(()=>{
  window.__r1={timeline:[],visible:()=>{
   const element=document.querySelector('.conversation-message-list');
   let fiber=element?.[Object.keys(element||{}).find(k=>k.startsWith('__reactFiber$'))];
   while(fiber?.return)fiber=fiber.return;
   // DOM fibers may reference an alternate tree; read only the committed root.
   const find=node=>{
    if(!node)return null;
    if(Array.isArray(node.memoizedProps?.messages)&&node.memoizedProps.scrollContainerRef)return node.memoizedProps.messages;
    return find(node.child)||find(node.sibling);
   };
   return {messages:find(fiber?.stateNode?.current)?.map(m=>({id:m.id,role:m.role,content:m.content})),user_count:document.querySelectorAll('.conversation-message-user').length,assistant_count:document.querySelectorAll('.conversation-message-assistant:not(.conversation-streaming-message)').length};
  }};
  const original=window.fetch.bind(window);
  window.fetch=async(...args)=>{
   const url=String(args[0]),chat=url.endsWith('/api/chat'),history=/\/api\/conversations\/[^/?]+$/.test(url);
   if(chat)window.__r1.timeline.push({at:Date.now(),type:'send'});
   if(history)window.__r1.timeline.push({at:Date.now(),type:'history_get_start',url});
   const response=await original(...args);
   if(history)response.clone().json().then(data=>window.__r1.timeline.push({at:Date.now(),type:'history_get_end',conversation_id:data.id,message_ids:data.messages?.map(m=>m.id)}));
   if(chat){const copy=response.clone();void(async()=>{
    const reader=copy.body.getReader(),decoder=new TextDecoder();let buffer='',kind='';
    while(true){const {value,done}=await reader.read();if(done)break;buffer+=decoder.decode(value,{stream:true});let newline;
     while((newline=buffer.indexOf('\n'))>=0){const line=buffer.slice(0,newline).trim();buffer=buffer.slice(newline+1);
      if(line.startsWith('event:'))kind=line.slice(6).trim();
      if(line.startsWith('data:')){try{const data=JSON.parse(line.slice(5));window.__r1.timeline.push({at:Date.now(),type:kind||data.type,...data});}catch{}}
     }
    }
    window.__r1.timeline.push({at:Date.now(),type:'stream_end'});
   })().catch(e=>window.__r1.timeline.push({type:'trace_error',error:String(e)}));}
   return response;
  };
 });
 await page.route(/\/api\/conversations\/[^/?]+$/,async route=>{
  const id=new URL(route.request().url()).pathname.split('/').pop();
  if(route.request().method()!=='GET'||!forced||seen.has(id))return route.continue();seen.add(id);
  note('held_history_start',{conversation_id:id});
  const response=await route.fetch(),body=await response.json();
  // Explicit test-only older projection: immediate persistence can precede GET.
  // Retain real persisted user rows but remove the new assistant from this snapshot.
  const originalIds=body.messages.map(m=>m.id);body.messages=body.messages.filter(m=>m.role!=='assistant');body.execution_history=[];
  let release;const gate=new Promise(resolve=>{release=resolve;});releases.push({id,release});
  await gate;note('held_history_release',{conversation_id:id,original_ids:originalIds,released_ids:body.messages.map(m=>m.id),test_only_stale_projection:true});
  await route.fulfill({response,json:body});
 });
 const visible=()=>page.evaluate(()=>window.__r1.visible());
 try{
  profile=await api('POST','/api/llm/models',{provider:'custom',label:'R1 instant local fixture',model:'r1-loopback',base_url:fixture.url,api_format:'openai',api_key:fixture.state.expected,api_key_env:'',temperature:0,max_tokens:1024,invoke_timeout_ms:10000});
  await api('POST',`/api/llm/models/${profile.id}/verify`);
  await api('POST',`/api/llm/models/${profile.id}/activate`);
  for(let index=1;index<=10;index++){
   forced=index>5;
   await page.goto(`http://127.0.0.1:${frontendPort}/chat`,{waitUntil:'domcontentloaded'});
   await page.getByRole('button',{name:'新建任务',exact:true}).click();
   const prompt=`R1 instant ${index}: local protocol only`;
   await page.locator('.composer-card textarea').fill(prompt);
   note('send',{index,forced});await page.getByRole('button',{name:'发送',exact:true}).click();
   await page.locator('.conversation-message-assistant:not(.conversation-streaming-message)').filter({hasText:'LOCAL_FIXTURE_RESPONSE'}).waitFor({timeout:15000});
   await page.waitForFunction(()=>window.__r1.timeline.some(e=>e.type==='stream_end')&&window.__r1.visible().messages?.some(m=>m.role==='assistant'));
   const stream=await page.evaluate(()=>window.__r1.timeline),connected=stream.find(e=>e.type==='connected'),done=stream.find(e=>e.type==='done');
   assert.ok(connected?.conversation_id&&done?.message_id);const id=connected.conversation_id;
   const live=await visible();assert.equal(live.user_count,1);assert.equal(live.assistant_count,1);assert.ok(live.messages,'actual MessageList state readable');
   snapshots.push({index,stage:'live',conversation_id:id,...live});
   if(forced){
    assert.equal(live.messages.find(m=>m.role==='assistant').id,done.message_id);
    const held=releases.find(r=>r.id===id);assert.ok(held,'forced history request intercepted');
    const ended=page.waitForResponse(r=>r.url().endsWith(`/api/conversations/${id}`));held.release();await ended;
    await page.waitForFunction(id=>window.__r1.timeline.some(e=>e.type==='history_get_end'&&e.conversation_id===id),id);
    const after=await visible();assert.equal(after.assistant_count,1);assert.equal(after.user_count,1);assert.equal(after.messages.find(m=>m.role==='assistant').id,done.message_id);snapshots.push({index,stage:'after_stale_history',...after});
   }
   const persisted=await api('GET',`/api/conversations/${id}`);
   assert.equal(persisted.messages.filter(m=>m.role==='user').length,1);assert.equal(persisted.messages.filter(m=>m.role==='assistant').length,1);
   const ids={optimistic_user:live.messages.find(m=>m.role==='user').id,persisted_user:persisted.messages.find(m=>m.role==='user').id,live_assistant:done.message_id,persisted_assistant:persisted.messages.find(m=>m.role==='assistant').id};
   assert.notEqual(ids.optimistic_user,ids.persisted_user);assert.notEqual(ids.live_assistant,ids.persisted_assistant);
   cases.push({index,conversation_id:id,forced_history:forced,ids,live_visible:true});
   note('browser_timeline',{index,events:await page.evaluate(()=>window.__r1.timeline)});
   await page.reload({waitUntil:'domcontentloaded'});await page.locator('.conversation-row').filter({hasText:prompt}).click();
   await page.locator('.conversation-message-assistant:not(.conversation-streaming-message)').filter({hasText:'LOCAL_FIXTURE_RESPONSE'}).waitFor();
   const reloaded=await visible();assert.equal(reloaded.user_count,1);assert.equal(reloaded.assistant_count,1);assert.equal(reloaded.messages.find(m=>m.role==='assistant').id,ids.persisted_assistant);
   cases.at(-1).reload_persisted=true;snapshots.push({index,stage:'reloaded',...reloaded});
  }
  assert.equal(fixture.state.calls.filter(c=>c.stream).length,10);assert.ok(fixture.state.calls.every(c=>c.authenticated));
  await page.screenshot({path:join(evidenceDir,'instant-reply-reloaded.png')});success=true;
  return {status:'passed',real_backend:true,instant_replies:10,forced_old_history:5,reload_checks:10,no_duplicate_assistant:true,provider:'immediate loopback fixture; no history wait or delay',cases};
 }finally{
  for(const held of releases)held.release();
  note('final_browser_timeline',{events:await page.evaluate(()=>window.__r1?.timeline||[]).catch(()=>[])});
  snapshots.push({stage:success?'final':'failure',...await visible().catch(()=>({unavailable:true}))});
  await writeFile(join(evidenceDir,'request-timeline.json'),JSON.stringify(timeline,null,2));
  await writeFile(join(evidenceDir,'message-state-snapshots.json'),JSON.stringify(snapshots,null,2));
  await writeFile(join(evidenceDir,'fixture-log.json'),JSON.stringify({status:success?'passed':'failed',calls:fixture.state.calls,cases},null,2));
  try{if(profile){await api('DELETE',`/api/llm/models/${profile.id}`);assert.ok(!(await api('GET','/api/llm/models')).some(m=>m.id===profile.id));}}finally{await fixture.close();}
 }
}
