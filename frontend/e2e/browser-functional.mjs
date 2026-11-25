import assert from 'node:assert/strict';
import {spawn} from 'node:child_process';
import {createServer} from 'node:http';
import {mkdtemp,mkdir,writeFile,readFile,access} from 'node:fs/promises';
import {join,resolve} from 'node:path';
import {tmpdir} from 'node:os';
import {randomUUID,createHash} from 'node:crypto';
import {chromium} from 'playwright-core';
import {ownedNativeFileDialog} from './native-browser-file-dialog.mjs';

assert.ok(process.env.YILIAN_NATIVE_BROWSER_BINARY,'Explicit owned fixture binary required');
const mode=process.env.YILIAN_BROWSER_FUNCTIONAL_CASE||'all';assert.ok(['all','storage'].includes(mode));
const root=resolve(import.meta.dirname,'../..'),isolated=await mkdtemp(join(tmpdir(),process.env.YILIAN_BROWSER_SHORT_ROOT==='1'?'yb-':'yilian-browser-functional-'));
const evidence=join(isolated,'evidence'),workspace=join(isolated,'workspace');
await Promise.all([mkdir(evidence),mkdir(workspace),mkdir(join(isolated,'data')),mkdir(join(isolated,'other-data'))]);
console.log('[browser-functional] evidence '+evidence);
const children=[],servers=[],checks=[],logs=[],diagnostics=[],uploads=[],exitProofs=[];
let browser,main,app,token,launchIndex=0;
const pause=ms=>new Promise(resolve=>setTimeout(resolve,ms));
async function until(read,predicate,label,ms=35000){const end=Date.now()+ms;let last;while(Date.now()<end){try{last=await read();if(predicate(last))return last;}catch(error){last=String(error);}await pause(100);}throw new Error(label+': '+JSON.stringify(last));}
async function bounded(work,ms,label){let timer;try{return await Promise.race([work,new Promise((_,reject)=>timer=setTimeout(()=>reject(new Error(label+' timed out')),ms))]);}finally{clearTimeout(timer);}}
async function listen(server){servers.push(server);await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));return server.address().port;}
async function freePort(){const server=createServer();await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));const port=server.address().port;await new Promise(resolve=>server.close(resolve));return port;}
function launch(command,args,env,label){const child=spawn(command,args,{cwd:label==='vite'?join(root,'frontend'):root,env:{...process.env,...env},windowsHide:label!=='tauri',stdio:['ignore','pipe','pipe']});for(const stream of [child.stdout,child.stderr])stream.on('data',data=>{const line=`[${label}] ${data}`;logs.push(line);void writeFile(join(evidence,'services-live.log'),logs.join(''));});children.push(child);return child;}
async function closeChild(child){if(child.exitCode!==null||child.signalCode!==null)return;child.kill();await bounded(new Promise(resolve=>child.once('exit',resolve)),5000,'owned child exit');}
async function check(name,run){const proof=await run();checks.push({name,passed:true,proof});await writeFile(join(evidence,'checks.json'),JSON.stringify(checks,null,2));console.log('[browser-functional] PASS '+name);return proof;}
async function invoke(command,args={}){return bounded(main.evaluate(({command,args})=>window.__TAURI_INTERNALS__.invoke(command,args),{command,args}),40000,command);}
const tabs=()=>invoke('native_browser_tabs');
const current=id=>tabs().then(items=>{const tab=items.find(tab=>tab.tab_id===id);assert.ok(tab,'Native tab must exist');return tab;});
const target=tab=>({tab_id:tab.tab_id,session_id:tab.session_id,navigation_id:tab.navigation_id});
async function execute(action,payload={},tab){return invoke('native_browser_execute',{request:{operation_id:randomUUID(),action,payload,target:tab?target(tab):null,owner:null}});}
async function observe(tab){const reply=await execute('observe',{},await current(tab.tab_id));assert.equal(reply.state,'verified');return reply.data.observation;}
async function select(tab){await main.evaluate(id=>{history.pushState({},'',`/browser?tab=${encodeURIComponent(id)}`);dispatchEvent(new PopStateEvent('popstate'));},tab.tab_id);await until(()=>observe(tab),value=>value?.text,'selected native view visible');}
async function open(path='/',ephemeral=false,workspaceId=null){const reply=await execute('open',{url:fixtureUrl+path,ephemeral,workspace_id:workspaceId,allowed_origins:[fixtureUrl]});assert.equal(reply.state,'verified',JSON.stringify(reply));const tab=reply.data.tab;await select(tab);return tab;}
async function human(tab,selector,key,chrome=false){return invoke('native_browser_fixture_schedule',{action:'human_input',key,request:{operation_id:randomUUID(),action:'click',target:target(await current(tab.tab_id)),payload:{selector,chrome},owner:null}});}
async function cookieMetadata(tab){return invoke('native_browser_fixture_schedule',{action:'cookie_metadata',request:{operation_id:randomUUID(),action:'observe',target:target(await current(tab.tab_id)),payload:{},owner:null}});}
async function chromeState(tab){const result=await invoke('native_browser_fixture_schedule',{action:'human_input',request:{operation_id:randomUUID(),action:'observe',target:target(await current(tab.tab_id)),payload:{chrome:true,inspect:true},owner:null}});return result.result.value;}
async function remember(tab){const observation=await observe(tab);const element=observation.elements.find(element=>element.name==='Remember login');assert.ok(element);const reply=await execute('click',{observation_id:observation.observation_id,element_id:element.element_id},await current(tab.tab_id));assert.equal(reply.state,'result_unknown');const state=await until(()=>observe(tab),o=>o.text.includes(marker),'fixture login state');assertLogin(state);await invoke('native_browser_acknowledge_unknown',{operationId:reply.data.operation_id});return state;}
async function pending(tab,kind){return until(()=>invoke('native_browser_ui_state'),state=>state.requests.some(item=>item.tab_id===tab.tab_id&&item.kind===kind),'pending '+kind).then(state=>state.requests.find(item=>item.tab_id===tab.tab_id&&item.kind===kind));}
async function api(method,path,body){const response=await fetch(base+path,{method,signal:AbortSignal.timeout(15000),headers:{'Content-Type':'application/json','X-Yilian-Control-Session':token},body:body===undefined?undefined:JSON.stringify(body)});assert.ok(response.ok,method+' '+path+': '+response.status);return response.json();}
const frontendPort=await freePort(),backendPort=await freePort(),debugPort=await freePort();
const frontend=`http://127.0.0.1:${frontendPort}`,base=`http://127.0.0.1:${backendPort}`;
const marker='remember-'+randomUUID(),payload='Owned native download '+randomUUID();
function assertLogin(observation){assert.ok(observation.text.includes('cookie=remember='+marker),'Persistent authentication cookie is present: '+observation.text.slice(0,600));assert.ok(observation.text.includes('local='+marker),'Persistent localStorage marker is present: '+observation.text.slice(0,600));}
const website=createServer(async(req,res)=>{
 if(req.url==='/download'){res.writeHead(200,{'Content-Type':'text/plain','Content-Disposition':'attachment; filename="browser-fixture.txt"'});res.end(payload);return;}
 if(req.url==='/upload'){let body='';for await(const part of req)body+=part;uploads.push(body);res.end('uploaded');return;}
 if(req.url==='/popup'){res.setHeader('Content-Type','text/html; charset=utf-8');res.end('<title>Managed popup</title><h1>Managed popup</h1><button id="reply" onclick="opener?.postMessage(\'managed-response\',location.origin)">Reply to opener</button><button id="close" onclick="window.close()">Close popup</button>');return;}
 res.setHeader('Content-Type','text/html; charset=utf-8');
 res.end(`<!doctype html><meta charset="utf-8"><title>Functional browser</title><style>body{font:18px sans-serif;padding:16px}button,a,input{display:inline-block;margin:10px;padding:10px}p{margin:10px}</style><h1>Functional native browser</h1><p>Needle one. Needle two.</p><button id="remember" onclick="document.cookie='remember=${marker}; Max-Age=86400; SameSite=Lax';localStorage.remember='${marker}';render()">Remember login</button><p id="state"></p><p id="click-trace">no click</p><a id="popup" href="/popup" target="_blank" rel="opener">Human popup</a><button id="script" onclick="setTimeout(()=>window.open('/popup'),6000)">Script popup</button><p id="reply">no opener response</p><input id="upload" type="file" aria-label="Upload"><p id="uploaded">no upload</p><a id="download" href="/download">Download fixture</a><button id="permission" onclick="navigator.geolocation.getCurrentPosition(()=>document.querySelector('#permission-result').textContent='unexpected location',e=>document.querySelector('#permission-result').textContent='permission denied '+e.code)">Request location</button><p id="permission-result"></p><script>addEventListener("click",e=>{document.querySelector("#click-trace").textContent="click="+e.target.id+"; trusted="+e.isTrusted+"; active="+navigator.userActivation.isActive},true);function render(){document.querySelector('#state').textContent='cookie='+document.cookie+'; local='+(localStorage.remember||'empty')}render();addEventListener('message',e=>{if(e.origin===location.origin)document.querySelector('#reply').textContent=e.data});document.querySelector('#upload').onchange=async e=>{const file=e.target.files[0];if(file){const text=await file.text();await fetch('/upload',{method:'POST',body:text});document.querySelector('#uploaded').textContent='uploaded '+file.name}};</script>`);
});
const fixtureUrl=`http://127.0.0.1:${await listen(website)}`;
await writeFile(join(evidence,'ports.json'),JSON.stringify({frontendPort,backendPort,debugPort,fixtureUrl}));
async function start(data='data'){
 ++launchIndex;
 const accountRoot=data==='data'?isolated:join(isolated,'account-b');await mkdir(accountRoot,{recursive:true});
 app=launch(process.env.YILIAN_NATIVE_BROWSER_BINARY,[],{OPENAI_API_KEY:'',RUST_LOG:'info',YILIAN_NATIVE_FIXTURE_ROOT:accountRoot,YILIAN_NATIVE_FIXTURE_KEY:'local-controlled-placeholder',YILIAN_DATA_DIR:join(accountRoot,'data'),YILIAN_WORKSPACE:join(accountRoot,'workspace'),YILIAN_HOST:`127.0.0.1:${backendPort}`,YILIAN_ALLOWED_ORIGINS:frontend,YILIAN_NATIVE_BROWSER_FRONTEND_URL:frontend,WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS:`--remote-debugging-port=${debugPort} --enable-logging --vmodule=sqlite_persistent_cookie_store=2,cookie_monster=1 --log-file=${join(evidence,"cookie-debug-"+launchIndex+".log")}`},'tauri');
 await until(async()=>{try{return(await fetch(`http://127.0.0.1:${debugPort}/json/list`)).json();}catch{return[];}},items=>items.some(item=>item.url.startsWith(frontend)),'owned app debug target',90000);
 browser=await chromium.connectOverCDP(`http://127.0.0.1:${debugPort}`);
 main=await until(()=>browser.contexts().flatMap(c=>c.pages()).find(p=>p.url().startsWith(frontend)),Boolean,'owned main');
 const monitoredPid=app.pid;
 main.on('pageerror',error=>diagnostics.push({message:error.message,stack:error.stack,appPid:monitoredPid,at:Date.now()}));
 await main.addInitScript(()=>{
  window.__fixtureEventUnlistenErrors=[];
  const plugin=window.__TAURI_EVENT_PLUGIN_INTERNALS__;
  if(plugin){const unregister=plugin.unregisterListener;plugin.unregisterListener=function(event,id){try{return unregister.call(this,event,id);}catch(error){window.__fixtureEventUnlistenErrors.push({event,id,registeredKeys:Object.keys(window.__internal_unstable_listeners_object_id__?.[event]||{}),message:String(error),stack:error.stack});throw error;}};}
 });
 await main.evaluate(()=>sessionStorage.setItem('yilian.setup.deferred','true'));
 await main.goto(frontend+'/browser');
 token=await invoke('get_control_session_token');
 await until(()=>api('GET','/api/health'),body=>body.status==='healthy','backend health');
}
async function stop(){
 const oldPid=app.pid;const contentPids=[...new Set(await Promise.all((await tabs()).map(tab=>invoke('native_browser_fixture_native_state',{tabId:tab.tab_id}).then(state=>state.browser_process_id))))].filter(pid=>Number.isSafeInteger(pid)&&pid>0);
 await invoke('native_browser_fixture_schedule',{action:'exit'});
 await until(()=>app.exitCode!==null||app.signalCode!==null,Boolean,'actual application process exits',20000);
 await browser.close().catch(()=>{});browser=null;main=null;
 await until(async()=>{try{await fetch(base+'/api/health',{signal:AbortSignal.timeout(500)});return false;}catch{return true;}},Boolean,'old backend process stopped');
 await until(async()=>{try{await fetch(`http://127.0.0.1:${debugPort}/json/version`,{signal:AbortSignal.timeout(500)});return false;}catch{return true;}},Boolean,'old WebView2 process stopped',20000);
 await until(()=>contentPids.every(pid=>{try{process.kill(pid,0);return false;}catch(error){if(error.code==='ESRCH')return true;throw error;}}),Boolean,'all owned content browser processes exited',20000);
 exitProofs.push({appPid:oldPid,contentPids,all_exited:true});await writeFile(join(evidence,'exit-proofs.json'),JSON.stringify(exitProofs,null,2));
}

try{
 launch(process.execPath,[join(root,'frontend/node_modules/vite/bin/vite.js'),'--host','127.0.0.1','--port',String(frontendPort),'--strictPort'],{VITE_API_BASE:base},'vite');
 await until(async()=>{try{return(await fetch(frontend)).ok;}catch{return false;}},Boolean,'isolated Vite');
 await start();
 const a=await api('POST','/api/workspaces',{name:'Persistent A',description:'Owned fixture',root_path:workspace});
 const b=await api('POST','/api/workspaces',{name:'Persistent B',description:'Owned fixture',root_path:workspace});
 let tab=await open('/',false,a.id);
 await writeFile(join(evidence,'native-profile-before.json'),JSON.stringify(await invoke('native_browser_fixture_native_state',{tabId:tab.tab_id}),null,2));
 const initialLogin=await remember(tab);await writeFile(join(evidence,'login-before.json'),JSON.stringify({text:initialLogin.text},null,2));
 if(mode==='storage'){
  const before=await cookieMetadata(tab);await writeFile(join(evidence,'cookie-metadata-before.json'),JSON.stringify({...before,now_unix:Date.now()/1000},null,2));
  assert.equal(before.cookies.length,1);assert.equal(before.cookies[0].is_session,false);assert.ok(before.cookies[0].expires>Date.now()/1000+3600);
  await check('storage_cookie_has_native_persistent_expiry',async()=>before);
  const beforePid=app.pid;const nativeBefore=await invoke('native_browser_fixture_native_state',{tabId:tab.tab_id});await stop();
  const cookieFile=join(nativeBefore.profile_path,'Network','Cookies');await writeFile(join(evidence,'cookie-disk-after-stop.json'),JSON.stringify({path:cookieFile,exists:await access(cookieFile).then(()=>true,()=>false)},null,2));
  await start();tab=await open('/',false,a.id);const after=await cookieMetadata(tab);await writeFile(join(evidence,'cookie-metadata-after.json'),JSON.stringify({...after,now_unix:Date.now()/1000},null,2));
  await writeFile(join(evidence,'native-profile-after.json'),JSON.stringify(await invoke('native_browser_fixture_native_state',{tabId:tab.tab_id}),null,2));
  await check('storage_cookie_and_localstorage_survive_actual_restart',async()=>{assertLogin(await observe(tab));assert.equal(after.cookies.length,1);assert.notEqual(app.pid,beforePid);return{beforePid,afterPid:app.pid,before,after};});
 }else{
 await check('native_find_and_zoom_are_real',async()=>{
  const zoom=await invoke('native_browser_command',{target:target(await current(tab.tab_id)),command:'zoom',payload:{factor:1.2}});assert.equal(zoom.factor,1.2);
  await invoke('native_browser_command',{target:target(await current(tab.tab_id)),command:'zoom',payload:{factor:1}});
  const find=await invoke('native_browser_command',{target:target(await current(tab.tab_id)),command:'find',payload:{query:'Needle'}});assert.ok(find.match_count>=2,JSON.stringify(find));assert.ok(find.active_match_index>=1);
  await invoke('native_browser_command',{target:target(await current(tab.tab_id)),command:'find',payload:{query:''}});return{zoom,find};
 });
 await check('human_target_blank_preserves_opener_and_managed_close',async()=>{
  const before=(await tabs()).map(t=>t.tab_id);const clicked=await human(tab,'#popup');console.log('[browser-functional] popup click '+JSON.stringify({clicked,state:await invoke('native_browser_ui_state'),native:await invoke('native_browser_fixture_native_state',{tabId:tab.tab_id}),page:await observe(tab).then(o=>o.text.slice(0,1500),error=>String(error))}));
  const popup=await until(tabs,items=>items.some(t=>!before.includes(t.tab_id)),'managed popup').then(items=>items.find(t=>!before.includes(t.tab_id)));
  await select(popup);await until(()=>observe(popup),o=>o.text.includes('Managed popup'),'popup ready');
  await human(popup,'#reply');await select(tab);await until(()=>observe(tab),o=>o.text.includes('managed-response'),'native window.opener response');
  await select(popup);await human(popup,'#close').catch(()=>{});await until(tabs,items=>!items.some(t=>t.tab_id===popup.tab_id),'window.close removed native managed tab');await select(tab);
  return{parent:tab.tab_id,popup:popup.tab_id,opener_response:true,managed_close:true};
 });
 await check('script_popup_has_explicit_action',async()=>{
  await human(tab,'#script');const request=await pending(tab,'popup');assert.ok(request.url.endsWith('/popup'));
  await invoke('native_browser_resolve_request',{id:request.id,decision:'open'});
  const popup=(await tabs()).find(t=>t.tab_id!==tab.tab_id);assert.ok(popup);await execute('close',{},popup);await select(tab);return{request:request.kind,opened:true};
 });
 await check('physical_native_shortcut_moves_focus_to_address',async()=>{
  await human(tab,'h1','Ctrl+L');await until(()=>main.getByRole('textbox',{name:'网址',exact:true}).evaluate(el=>el===document.activeElement&&document.hasFocus()),Boolean,'native address focus');
  await main.keyboard.type('localhost:3000/actual-native-focus');assert.equal(await main.getByRole('textbox',{name:'网址',exact:true}).inputValue(),'localhost:3000/actual-native-focus');
  await main.getByRole('textbox',{name:'网址',exact:true}).fill(fixtureUrl+'/');return{native_focus:true,typed:true};
 });
 await check('native_tab_create_switch_and_close_shortcuts',async()=>{
  await select(tab);await human(tab,'h1','Ctrl+T');await until(()=>main.locator('.browser-start').count(),count=>count===1,'Ctrl+T new page');await select(tab);
  const sibling=await open('/?shortcut=sibling',false,a.id);await select(sibling);await human(sibling,'h1','Ctrl+Shift+Tab');await until(()=>main.getByRole('tab',{selected:true}).getAttribute('title'),url=>url===fixtureUrl+'/','Ctrl+Shift+Tab selected original');
  await select(tab);await human(tab,'h1','Ctrl+Tab');await until(()=>main.getByRole('tab',{selected:true}).getAttribute('title'),url=>url===fixtureUrl+'/?shortcut=sibling','Ctrl+Tab selected sibling');
  await select(sibling);await human(sibling,'h1','Ctrl+W');await until(tabs,items=>!items.some(item=>item.tab_id===sibling.tab_id),'Ctrl+W closes correct managed tab');await select(tab);return{new_tab:true,previous:true,next:true,closed_target:true};
 });
 await check('detached_content_and_chrome_new_tab_shortcuts_return_to_main',async()=>{
  await execute('present',{mode:'detached'},await current(tab.tab_id));await until(()=>current(tab.tab_id),item=>item.mode==='detached','native detached ownership');
  await until(()=>chromeState(tab),state=>state.tabReady,'detached trusted chrome ready');
  const content=await human(tab,'h1','Ctrl+T');assert.equal(content.surface,'content');await until(()=>main.locator('.browser-start').count(),count=>count===1,'detached content Ctrl+T');
  await select(tab);const chrome=await human(tab,'h1','Ctrl+T',true);assert.equal(chrome.surface,'chrome');await until(()=>main.locator('.browser-start').count(),count=>count===1,'detached chrome DOM Ctrl+T');
  await execute('present',{mode:'embedded',x:120,y:150,width:800,height:450},await current(tab.tab_id));await select(tab);return{content:true,chrome:true,same_tab:tab.tab_id};
 });
 await check('permission_is_explicit_and_denial_reaches_page',async()=>{await human(tab,'#permission');const request=await pending(tab,'permission');await invoke('native_browser_resolve_request',{id:request.id,decision:'deny'});await until(()=>observe(tab),o=>o.text.includes('permission denied 1'),'native permission callback');return{denied:true,coordinates_read:false};});
 await check('native_upload_cancel_and_selected_file',async()=>{
  const cancel=ownedNativeFileDialog({appPid:app.pid,action:'cancel',recordChild:child=>children.push(child)});await Promise.all([cancel,human(tab,'#upload')]);assert.equal(uploads.length,0);
  const path=join(isolated,'上传验证.txt'),contents='Owned upload '+randomUUID();await writeFile(path,contents);
  const choose=ownedNativeFileDialog({appPid:app.pid,action:'choose',path,recordChild:child=>children.push(child)});const proof=await Promise.all([choose,human(tab,'#upload')]);await until(()=>uploads.includes(contents),Boolean,'selected upload reaches owned server');return{cancelled_without_upload:true,selected_content_exact:true,dialog:proof[0]};
 });
 await check('download_save_cancel_then_selected_destination',async()=>{
  await human(tab,'#download');const request=await pending(tab,'download');const destination=join(isolated,'下载验证.txt');assert.equal(await access(destination).then(()=>true,()=>false),false);
  const cancel=ownedNativeFileDialog({appPid:app.pid,action:'cancel',recordChild:child=>children.push(child)});const cancelled=await Promise.all([cancel,invoke('native_browser_resolve_request',{id:request.id,decision:'save'})]);assert.equal(cancelled[1].cancelled,true);assert.equal(await access(destination).then(()=>true,()=>false),false);assert.ok((await invoke('native_browser_ui_state')).requests.some(r=>r.id===request.id));
  const choose=ownedNativeFileDialog({appPid:app.pid,action:'choose',path:destination,suggestedName:request.suggested_name,recordChild:child=>children.push(child)});await Promise.all([choose,invoke('native_browser_resolve_request',{id:request.id,decision:'save'})]);await until(()=>readFile(destination,'utf8').catch(()=>''),value=>value===payload,'selected native download exact bytes');return{cancelled:true,saved:true,sha256:createHash('sha256').update(payload).digest('hex')};
 });
 await check('shared_login_survives_one_tab_close_and_project_isolation',async()=>{
  assertLogin(await observe(tab));
  const sibling=await open('/',false,a.id);assert.ok((await observe(sibling)).text.includes(marker));await execute('close',{},tab);tab=sibling;await select(tab);assert.ok((await observe(tab)).text.includes(marker));
  const other=await open('/',false,b.id);assert.ok(!(await observe(other)).text.includes(marker));await execute('close',{},other);
  const temporary=await open('/',true,a.id);assert.ok(!(await observe(temporary)).text.includes(marker));await remember(temporary);await execute('close',{},temporary);await select(tab);assertLogin(await observe(tab));return{shared:true,sibling_survives_close:true,other_project_isolated:true,temporary_isolated:true};
 });
 await main.screenshot({path:join(evidence,'functional-native-final.png'),animations:'disabled'});
 await check('persistent_cookie_and_localstorage_survive_true_process_restart',async()=>{const previousPid=app.pid;await stop();await start();tab=await open('/',false,a.id);await writeFile(join(evidence,'native-profile-after.json'),JSON.stringify(await invoke('native_browser_fixture_native_state',{tabId:tab.tab_id}),null,2));assertLogin(await observe(tab));assert.notEqual(app.pid,previousPid);const temporary=await open('/',true,a.id);assert.ok(!(await observe(temporary)).text.includes(marker));await execute('close',{},temporary);await select(tab);return{previousPid,currentPid:app.pid,persistent_cookie_and_storage:true,temporary_absent:true};});
 await check('clearing_shared_profile_data_really_removes_login',async()=>{const sibling=await open('/',false,a.id);await invoke('native_browser_command',{target:target(await current(sibling.tab_id)),command:'clear_data',payload:{}});await select(sibling);assert.ok(!(await until(()=>observe(sibling),o=>o.text.includes('cookie='),'cleared document')).text.includes(marker));await select(tab);assert.ok(!(await observe(tab)).text.includes(marker));return{shared_profile_cleared:true};});
 const personalA=await open('/',false,null);await remember(personalA);assertLogin(await observe(personalA));const personalProfileA=await invoke('native_browser_fixture_native_state',{tabId:personalA.tab_id});
 await stop();
 await check('other_account_profile_uses_separate_native_store',async()=>{await start('other-data');const personal=await open('/',false,null);const otherProfile=await invoke('native_browser_fixture_native_state',{tabId:personal.tab_id});assert.notEqual(otherProfile.profile_path,personalProfileA.profile_path,'The isolated fixture must use a genuinely different account data root');assert.ok(!(await observe(personal)).text.includes(marker));return{separate_profile_directory:true};});
 await check('returning_to_original_account_retains_its_own_store',async()=>{await stop();await start();const personal=await open('/',false,null);assertLogin(await observe(personal));return{original_profile_preserved:true};});
 await check('same_native_find_query_restarts_after_document_navigation',async()=>{
  const page=(await tabs())[0];const prior=await current(page.tab_id);
  const first=await invoke('native_browser_command',{target:target(prior),command:'find',payload:{query:'Needle'}});assert.ok(first.match_count>=2);
  await execute('reload',{},prior);await until(()=>current(page.tab_id),value=>value.navigation_id>prior.navigation_id&&!value.loading,'reloaded native document');
  const second=await invoke('native_browser_command',{target:target(await current(page.tab_id)),command:'find',payload:{query:'Needle'}});assert.ok(second.match_count>=2,JSON.stringify(second));return{first,second};
 });
 }
 assert.deepEqual(diagnostics,[]);
 await writeFile(join(evidence,'result.json'),JSON.stringify({status:'passed',mode,checks,diagnostics,isolated,boundary:'Actual owned Tauri/WebView2, native COM, selected local fixtures and true application process exit/restart; no external accounts/models.'},null,2));
 console.log(JSON.stringify({status:'passed',checks:checks.length,evidence}));
}catch(error){await writeFile(join(evidence,'failure.txt'),String(error.stack||error));await writeFile(join(evidence,'diagnostics.json'),JSON.stringify(diagnostics,null,2));if(main){await main.evaluate(()=>window.__fixtureEventUnlistenErrors||[]).then(data=>writeFile(join(evidence,'event-unlisten-errors.json'),JSON.stringify(data,null,2))).catch(()=>{});await bounded(main.screenshot({path:join(evidence,'failure.png'),timeout:5000}),6000,'failure screenshot').catch(()=>{});}throw error;
}finally{
 if(main)await bounded(invoke('native_browser_fixture_schedule',{action:'exit'}),1500,'cleanup exit').catch(()=>{});
 if(browser)await bounded(browser.close(),3000,'owned debug close').catch(()=>{});
 const cleanup=await Promise.allSettled(children.slice().reverse().map(closeChild));
 for(const server of servers){server.closeAllConnections();await new Promise(resolve=>server.close(resolve));}
 await writeFile(join(evidence,'cleanup.json'),JSON.stringify(cleanup.map(item=>({status:item.status,reason:item.status==='rejected'?String(item.reason):null})),null,2));
 await writeFile(join(evidence,'services.log'),logs.join(''));
 if(cleanup.some(item=>item.status==='rejected'))throw new Error('Owned fixture cleanup failed');
}
