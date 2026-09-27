import assert from 'node:assert/strict';
import {writeFile} from 'node:fs/promises';
import {join} from 'node:path';
export async function chatViewRace({page,frontendPort,evidenceDir}){
 const timeline=[],pending=[];
 const note=(type,data={})=>timeline.push({at:Date.now(),type,...data});
 await page.route('**/src/api/chat.ts',route=>route.fulfill({contentType:'text/javascript',body:`export function sendMessage(text,id,handler){window.__chat.handler=handler;const controller=new AbortController();window.__chat.controller=controller;return controller;} export async function stopGeneration(id){window.__chat.stopped=id;}` }));
 for(const component of ['ChatInput','WorkbenchHome','ChatHeader','MessageList'])await page.route(`**/src/components/chat/${component}.tsx`,route=>route.fulfill({contentType:'text/javascript',body:`import React from '/node_modules/.vite/deps/react.js';export default function Leaf(props){const t=window.__chat;if(props.onSend){t.send=props.onSend;t.stop=props.onStop;t.loading=props.isLoading;}if(props.messages){t.messages=props.messages;t.error=props.error;t.streaming=props.streaming;}return React.createElement('pre',{id:'${component}'},JSON.stringify(props.messages||[]));}`}));
 await page.route('**/api/**',async route=>{
  const url=new URL(route.request().url());
  if(!url.pathname.startsWith('/api/'))return route.fallback();
  if(/^\/api\/conversations\/[^/]+$/.test(url.pathname)){
   const id=url.pathname.split('/').pop();note('history_start',{id});pending.push({id,route});return;
  }
  const body=url.pathname==='/api/settings'?{provider_readiness:{model:{available:true,configured:true}}}:url.pathname==='/api/workflows'?{workflows:[],active_id:null}:url.pathname==='/api/approvals/pending'?{approvals:[]}:[];
  await route.fulfill({json:body});
 });
 const state=()=>page.evaluate(()=>({messages:window.__chat.messages,anchor:window.__chat.anchor,execution:window.__chat.execution.state,error:window.__chat.error}));
 const event=async e=>{note('SSE',e);await page.evaluate(e=>window.__chat.handler(e),e);};
 const release=async(id,messages)=>{const item=pending.find(p=>p.id===id&&!p.done);assert.ok(item);item.done=true;note('history_end',{id,messages});await item.route.fulfill({json:{id,title:id,updated_at:1,messages}});await page.waitForFunction(id=>document.querySelector('#anchor')?.textContent.includes(id),id);};
 try{
  await page.goto(`http://127.0.0.1:${frontendPort}/e2e/fixtures/chat-view.html`);
  await page.waitForFunction(()=>typeof window.__chat?.send==='function');
  await page.evaluate(()=>window.__chat.send('first question'));note('optimistic',await state());
  const started=page.waitForRequest(r=>r.url().endsWith('/api/conversations/new-A'));
  await event({type:'connected',conversation_id:'new-A'});await started;
  await event({type:'token',conversation_id:'new-A',token:'instant assistant'});
  await event({type:'done',conversation_id:'new-A',message_id:'live-assistant'});
  await event({type:'stream_end',conversation_id:'new-A'});
  await page.waitForFunction(()=>window.__chat.messages.some(m=>m.id==='live-assistant'));note('live_completed',await state());
  await release('new-A',[{id:'persisted-user',role:'user',content:'first question',created_at:1}]);
  note('after_old_snapshot',await state());
  assert.ok((await state()).messages.some(m=>m.id==='live-assistant'),'Older history snapshot overwrote the instant live assistant');
  return {status:'passed',checks:['instant first reply preserved after older history']};
 }finally{await writeFile(join(evidenceDir,'chat-view-timeline.json'),JSON.stringify(timeline,null,2));}
}
