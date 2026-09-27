import assert from 'node:assert/strict';
import {writeFile} from 'node:fs/promises';
import {join} from 'node:path';
export async function chatViewRace({page,frontendPort,evidenceDir}){
 const timeline=[],pending=[],checks=[],waiters=[],approvalWaiters=[];let stamp=0,holdApproval=false;
 const note=(type,data={})=>timeline.push({at:Date.now(),type,...data});
 await page.route('**/src/api/conversations.ts',route=>route.fulfill({contentType:'text/javascript',body:`export * from '/src/api/conversations.ts?actual';import {loadConversation as actual} from '/src/api/conversations.ts?actual';export async function loadConversation(id){const result=await actual(id);window.__chat.loaded=(window.__chat.loaded||[]).concat(result.updated_at);return result;}`}));
 await page.route('**/src/api/chat.ts',route=>route.fulfill({contentType:'text/javascript',body:`export function sendMessage(text,id,handler){window.__chat.handler=handler;const controller=new AbortController();window.__chat.controller=controller;return controller;} export async function stopGeneration(id){window.__chat.stopped=id;}` }));
 for(const component of ['ChatInput','WorkbenchHome','ChatHeader','MessageList'])await page.route(`**/src/components/chat/${component}.tsx`,route=>route.fulfill({contentType:'text/javascript',body:`import React from '/node_modules/.vite/deps/react.js';export default function Leaf(props){const t=window.__chat;if(props.onSend){t.send=props.onSend;t.stop=props.onStop;t.loading=props.isLoading;}if(props.messages){t.messages=props.messages;t.error=props.error;t.streaming=props.streaming;}return React.createElement('pre',{id:'${component}'},JSON.stringify(props.messages||[]));}`}));
 await page.route('**/api/**',async route=>{
  const url=new URL(route.request().url());
  if(!url.pathname.startsWith('/api/'))return route.fallback();
  if(/^\/api\/conversations\/[^/]+$/.test(url.pathname)){
   const id=url.pathname.split('/').pop();note('history_start',{id});pending.push({id,route});waiters.shift()?.(id);return;
  }
  if(url.pathname==='/api/approvals/pending'&&holdApproval){holdApproval=false;approvalWaiters.push(route);return;}
  const body=url.pathname==='/api/settings'?{provider_readiness:{model:{available:true,configured:true}}}:url.pathname==='/api/workflows'?{workflows:[],active_id:null}:url.pathname==='/api/approvals/pending'?{approvals:[]}:[];
  await route.fulfill({json:body});
 });
 const state=()=>page.evaluate(()=>({messages:window.__chat.messages,anchor:window.__chat.anchor,execution:window.__chat.execution.state,error:window.__chat.error}));
 const event=async e=>{note('SSE',e);await page.evaluate(e=>window.__chat.handler(e),e);};
 const release=async(id,messages,execution_history=[],anchor=true)=>{
  const item=pending.find(p=>p.id===id&&!p.done);assert.ok(item);item.done=true;
  const version=++stamp;note('history_end',{id,messages,version});
  await item.route.fulfill({json:{id,title:id,updated_at:version,messages,execution_history}});
  await page.waitForFunction(v=>window.__chat.loaded?.includes(v),version);
  if(anchor)await page.waitForFunction(v=>JSON.parse(document.querySelector('#anchor').textContent)?.updated_at===v,version);
 };
 const waitHistory=()=>new Promise(resolve=>waiters.push(resolve));
 const switchTo=async id=>{const loaded=id?waitHistory():null;await page.evaluate(id=>window.__chat.switchTo(id),id);if(loaded)await loaded;};
 const refresh=async()=>{const loaded=waitHistory();await page.evaluate(()=>window.__chat.refresh());await loaded;};
 const message=(id,role,content)=>({id,role,content,created_at:1});
 const countAssistant=async count=>page.waitForFunction(n=>window.__chat.messages.filter(m=>m.role==='assistant').length===n,count);

 try{
  await page.goto(`http://127.0.0.1:${frontendPort}/e2e/fixtures/chat-view.html`);
  await page.waitForFunction(()=>typeof window.__chat?.send==='function');
  await page.evaluate(()=>window.__chat.send('first question'));note('optimistic',await state());
  const started=waitHistory();
  await event({type:'connected',conversation_id:'new-A'});await started;
  await event({type:'token',conversation_id:'new-A',token:'instant assistant'});
  await event({type:'done',conversation_id:'new-A',message_id:'live-assistant'});
  await event({type:'stream_end',conversation_id:'new-A'});
  await page.waitForFunction(()=>window.__chat.messages.some(m=>m.id==='live-assistant'));note('live_completed',await state());
  await release('new-A',[{id:'persisted-user',role:'user',content:'first question',created_at:1}]);
  note('after_old_snapshot',await state());
  assert.ok((await state()).messages.some(m=>m.id==='live-assistant'),'Older history snapshot overwrote the instant live assistant');
  checks.push('instant first reply preserved after older history');
  assert.equal((await state()).messages.filter(m=>m.role==='user').length,1);
  // Idle refresh can replace local IDs with authoritative persisted IDs without merging duplicates.
  await refresh();await release('new-A',[message('persisted-user','user','first question'),message('persisted-assistant','assistant','instant assistant')]);
  await page.waitForFunction(()=>window.__chat.messages.some(m=>m.id==='persisted-assistant'));
  await event({type:'done',conversation_id:'new-A',message_id:'live-assistant'});
  assert.equal((await state()).messages.length,2);checks.push('idle refresh and no duplicate with different persisted IDs');

  // A's delayed response cannot update B messages, execution, or voice anchor.
  await switchTo('A');await switchTo('B');await release('B',[message('B-user','user','B history')]);
  await release('A',[message('A-user','user','A stale history')],[],false);
  assert.deepEqual((await state()).messages.map(m=>m.id),['B-user']);assert.equal((await state()).anchor.conversation_id,'B');checks.push('conversation switch and stale voice anchor');

  // Refresh begins before a live turn, including an active tool and approval.
  await refresh();await page.evaluate(()=>window.__chat.send('B question'));
  await event({type:'connected',conversation_id:'B'});
  await event({type:'tool_start',conversation_id:'B',tool_call_id:'B-tool',tool_name:'read_file'});
  await event({type:'approval_required',conversation_id:'B',tool_call_id:'B-tool',approval_id:'B-approval',tool_name:'read_file'});
  await release('B',[message('B-user','user','B history')]);
  assert.equal((await state()).execution.records['B-tool'].approvalStatus,'pending');
  await event({type:'approval_resolved',conversation_id:'B',tool_call_id:'B-tool',approval_id:'B-approval',status:'approved'});
  await event({type:'token',conversation_id:'B',token:'B live reply'});
  await event({type:'stream_end',conversation_id:'B'});
  await event({type:'done',conversation_id:'B',message_id:'B-live'});
  await countAssistant(1);await event({type:'done',conversation_id:'B',message_id:'B-live'});await countAssistant(1);
  assert.equal((await state()).messages.filter(m=>m.id==='B-live').length,1);
  checks.push('refresh preserves live execution and approval; stream_end/done and duplicate done append once');

  // Provider error and stop retain their existing completion semantics.
  await refresh();await page.evaluate(()=>window.__chat.send('error question'));
  await event({type:'error',conversation_id:'B',error:'fixture provider unavailable'});
  await release('B',[]);
  assert.equal((await state()).error,'fixture provider unavailable');assert.equal(await page.evaluate(()=>window.__chat.loading),false);
  await page.evaluate(()=>window.__chat.send('stop question'));
  await page.evaluate(()=>window.__chat.stop());
  assert.equal(await page.evaluate(()=>window.__chat.stopped),'B');assert.equal(await page.evaluate(()=>window.__chat.controller.signal.aborted),true);
  assert.equal(await page.evaluate(()=>window.__chat.loading),false);checks.push('provider error preserved; stop remains effective');

  // Refresh snapshot is older than the completed assistant, not merely an active turn.
  await refresh();await page.evaluate(()=>window.__chat.send('refresh question'));
  await event({type:'token',conversation_id:'B',token:'refresh reply'});
  await event({type:'done',conversation_id:'B',message_id:'refresh-live'});await countAssistant(2);
  await release('B',[]);assert.ok((await state()).messages.some(m=>m.id==='refresh-live'));
  checks.push('refresh revision cannot erase completed assistant');
  // Delayed approval list from A must not enter B's store after navigation.
  await switchTo('approval-A');holdApproval=true;
  await release('approval-A',[message('approval-user','user','approval history')]);
  await switchTo('approval-B');await release('approval-B',[message('B-only','user','B only')]);
  assert.equal(approvalWaiters.length,1);
  const approvalResponse=page.waitForResponse(r=>r.url().endsWith('/api/approvals/pending'));
  await approvalWaiters[0].fulfill({json:{approvals:[{approval_id:'stale-A',conversation_id:'approval-A',status:'pending'}]}});await approvalResponse;
  assert.equal(await page.evaluate(()=>window.__chat.approvals().some(a=>a.approval_id==='stale-A')),false);
  assert.equal((await state()).anchor.conversation_id,'approval-B');checks.push('late approval hydration discarded by conversation epoch');
  // All wire events arrive in one JS task, before the promoted identity hydrates.
  await switchTo(null);await page.waitForFunction(()=>window.__chat.execution.state.conversationId===null);
  const batchedHistory=waitHistory();
  await page.evaluate(async()=>{
    await window.__chat.send('batched question');
    for(const event of [{type:'connected',conversation_id:'batch'},
      {type:'token',conversation_id:'batch',token:'batched assistant'},
      {type:'done',conversation_id:'batch',message_id:'batch-live'},
      {type:'stream_end',conversation_id:'batch'}])window.__chat.handler(event);
  });
  await batchedHistory;await countAssistant(1);
  await release('batch',[message('batch-persisted-user','user','batched question')]);
  assert.equal((await state()).messages.find(m=>m.role==='assistant').id,'batch-live');
  checks.push('batched connected/token/done before first history request');
  return {status:'passed',component:'real ChatView mounted in system Edge; controlled transport/leaf components',checks};
 }finally{await writeFile(join(evidenceDir,'chat-view-timeline.json'),JSON.stringify(timeline,null,2));}
}
