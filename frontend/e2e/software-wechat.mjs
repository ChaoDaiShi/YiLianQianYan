// Explicitly authorized real-WeChat WINDOW compatibility acceptance.
// No screenshots, window titles, UIA, chat/content/profile reads, login or sending.
import assert from "node:assert/strict";
import {spawn, execFileSync} from "node:child_process";
import {createServer} from "node:http";
import {mkdtemp, mkdir, readFile, writeFile} from "node:fs/promises";
import {createHash, randomUUID} from "node:crypto";
import {join, resolve, basename} from "node:path";
import {tmpdir} from "node:os";
import {connect} from "node:net";
import {chromium} from "playwright-core";

assert.equal(process.platform, "win32");
assert.equal(process.env.YILIAN_REAL_WECHAT_WINDOW_TEST, "1", "Requires explicit authorization for real WeChat window testing");
assert.equal(process.env.YILIAN_SOFTWARE_BUILD_CONFIRMED, "1");
const root = resolve(import.meta.dirname, "../..");
const application = resolve(process.env.YILIAN_NATIVE_BROWSER_BINARY || "");
const executable = resolve(process.env.YILIAN_WECHAT_EXE || "");
const targetHwnd = process.env.YILIAN_WECHAT_HWND;
const targetPid = Number(process.env.YILIAN_WECHAT_PID);
assert.equal(basename(application), "yilian-native-browser-fixture.exe");
assert.match(basename(executable), /^(Weixin|WeChat)\.exe$/i);
assert.match(targetHwnd || "", /^\d+$/);
assert.ok(Number.isSafeInteger(targetPid) && targetPid > 0);
const isolated = await mkdtemp(join(tmpdir(), "yilian-wechat-window-"));
const evidence = join(isolated, "evidence");
await Promise.all([mkdir(evidence), mkdir(join(isolated, "workspace")), mkdir(join(isolated, "data"))]);
console.log(`[wechat-window] evidence ${evidence}`);
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
const children = [], checks = [], logs = [], metadata = [], errors = [], timings = {};
let browser, main, app, token, session, before, failure, restoreConfirmed = false;
async function freePort() {const server=createServer();await new Promise(r=>server.listen(0,"127.0.0.1",r));const port=server.address().port;await new Promise(r=>server.close(r));return port;}
const ports = {backend: await freePort(), frontend: await freePort(), debug: await freePort()};
const base=`http://127.0.0.1:${ports.backend}`, frontend=`http://127.0.0.1:${ports.frontend}`;
async function portOpen(port) {return new Promise(resolve=>{const socket=connect({host:"127.0.0.1",port});const finish=value=>{socket.destroy();resolve(value);};socket.once("connect",()=>finish(true));socket.once("error",()=>finish(false));socket.setTimeout(1000,()=>finish(false));});}
async function until(read, accept, label, ms=30000) {const deadline=Date.now()+ms;let value;while(Date.now()<deadline){value=await read();if(accept(value))return value;await delay(100);}throw new Error(`${label}: ${JSON.stringify(value)}`);}
function launch(command,args,env,label) {const child=spawn(command,args,{cwd:label==="vite"?join(root,"frontend"):root,env:{...process.env,...env},windowsHide:true,stdio:["ignore","pipe","pipe"]});children.push(child);for(const stream of [child.stdout,child.stderr])stream.on("data",b=>logs.push(`[${label}] ${b}`));return child;}
function check(name) {checks.push(name);console.log(`[wechat-window] PASS ${name}`);}
function native(label) {
 const value=JSON.parse(execFileSync("powershell.exe",["-NoProfile","-NonInteractive","-WindowStyle","Hidden","-File",join(root,"frontend/e2e/software-window-metadata.ps1")],{cwd:root,encoding:"utf8",timeout:12000,windowsHide:true,env:{...process.env,YILIAN_TEST_WINDOW_PID:String(targetPid),YILIAN_TEST_WINDOW_EXE:executable,YILIAN_TEST_WINDOW_HANDLE:targetHwnd}}).replace(/^\uFEFF/,""));
 metadata.push({label,...value});return value;
}
async function api(method,path,body) {const response=await fetch(base+path,{method,signal:AbortSignal.timeout(15000),headers:{"Content-Type":"application/json","X-Yilian-Control-Session":token},body:body===undefined?undefined:JSON.stringify(body)});const text=await response.text();assert.equal(response.status,200,`${path}: ${text}`);return text?JSON.parse(text):null;}
async function current() {return (await api("GET","/api/software")).sessions.find(s=>s.id===session.id);}
async function button(name) {const locator=main.getByRole("button",{name,exact:true});await until(()=>locator.isEnabled(),Boolean,`${name} enabled`);await locator.evaluate(element=>element.click());}
async function embed(label) {
 restoreConfirmed=false;
 await button("嵌回忆涟千言");
 return checkEmbedded(label);
}
async function checkEmbedded(label) {
 session=await until(current,s=>s.window.mode==="embedded"&&s.state==="active",label);
 const actual=native(label);
 assert.equal(actual.desktopParent,true,"WeChat must retain its own top-level input queue");
 assert.equal(actual.owner,before.owner);
 const presentationBits=0x10000000n|0x20000000n|0x01000000n; // visible, minimized, maximized
 assert.equal(BigInt(actual.style)&~presentationBits,BigInt(before.style)&~presentationBits);
 assert.equal(actual.exstyle,before.exstyle);
 assert.equal(actual.visible,true);assert.equal(actual.responds,true);
 assert.equal(actual.ownsCenter,true,"The actual WeChat window must own its display region");
 assert.equal(actual.clientHit,true,"WeChat must handle a live native mouse hit-test in its client area");
 assert.equal(session.window.identity.hwnd,targetHwnd);assert.equal(session.window.identity.pid,targetPid);
 return actual;
}
function assertRestored(label) {
 const actual=native(label);
 for(const key of ["hwnd","pid","parent","owner","style","exstyle","visible","minimized","dpi"])assert.equal(actual[key],before[key],`${label}: ${key}`);
 assert.deepEqual(actual.placement,before.placement,`${label}: original placement`);
 assert.deepEqual(actual.rect,before.rect,`${label}: original rectangle`);
 assert.equal(actual.responds,true);restoreConfirmed=true;return actual;
}
async function manifest() {const files=["src-tauri/src/software/mount.rs","src-tauri/src/software/dock.rs","src-tauri/src/software/identity.rs","src-tauri/src/software/geometry.rs","src-tauri/src/software_bridge.rs","frontend/src/features/software/SoftwarePage.tsx","frontend/src/features/software/SoftwareSessionPanel.tsx","frontend/e2e/software-wechat.mjs","frontend/e2e/software-window-metadata.ps1"];return Promise.all(files.map(async path=>({path,sha256:createHash("sha256").update(await readFile(join(root,path))).digest("hex")})));}
try {
 before=native("before-attachment");assert.equal(before.desktopParent,true);assert.equal(before.owner,"0");
 await writeFile(join(evidence,"source-start.json"),JSON.stringify(await manifest(),null,2));
 await writeFile(join(evidence,"binaries.json"),JSON.stringify(await Promise.all([application,executable].map(async path=>({path,sha256:createHash("sha256").update(await readFile(path)).digest("hex")}))),null,2));
 launch(process.execPath,[join(root,"frontend/node_modules/vite/bin/vite.js"),"--host","127.0.0.1","--port",String(ports.frontend),"--strictPort"],{VITE_API_BASE:base},"vite");
 await until(async()=>fetch(frontend,{signal:AbortSignal.timeout(1000)}).then(r=>r.ok).catch(()=>false),Boolean,"Vite ready");
 app=launch(application,[],{OPENAI_API_KEY:"",YILIAN_NATIVE_FIXTURE_ROOT:isolated,YILIAN_NATIVE_FIXTURE_KEY:"local-window-test-placeholder",YILIAN_HOST:`127.0.0.1:${ports.backend}`,YILIAN_ALLOWED_ORIGINS:frontend,YILIAN_NATIVE_BROWSER_FRONTEND_URL:frontend,WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS:`--remote-debugging-port=${ports.debug}`},"tauri");
 await until(async()=>fetch(`http://127.0.0.1:${ports.debug}/json/list`,{signal:AbortSignal.timeout(1000)}).then(r=>r.json()).catch(()=>[]),v=>v.some(t=>t.type==="page"&&t.url.startsWith(frontend)),"owned Tauri renderer");
 browser=await chromium.connectOverCDP(`http://127.0.0.1:${ports.debug}`);
 main=await until(()=>browser.contexts().flatMap(c=>c.pages()).find(p=>p.url().startsWith(frontend)),Boolean,"owned main page");
 main.on("pageerror",e=>errors.push(e.message));
 await main.evaluate(()=>sessionStorage.setItem("yilian.setup.deferred","true"));
 await main.goto(frontend+"/software",{waitUntil:"domcontentloaded"});
 token=await until(()=>main.evaluate(()=>window.__TAURI_INTERNALS__.invoke("get_control_session_token")),v=>typeof v==="string"&&v.length>=32,"trusted token");
 await main.evaluate(()=>window.__TAURI_INTERNALS__.invoke("native_software_bind"));
 await until(()=>api("GET","/api/software"),v=>v.available,"native host");
 const registration=await api("POST","/api/software/registrations",{name:"微信窗口兼容性测试",adapter:"wechat",executable,workspace_id:null,allow_observe:false,allow_actions:false});
 assert.equal(registration.allow_observe,false);assert.equal(registration.allow_actions,false);
 const candidates=(await api("POST",`/api/software/registrations/${registration.id}/discover`,{})).candidates;
 assert.deepEqual(candidates.map(c=>[c.identity.hwnd,c.identity.pid]),[[targetHwnd,targetPid]],
  "Discovery must expose the actual WeChat window without helper processes");
 await button("刷新软件状态");
 await button("连接软件");
 session=await until(async()=>(await api("GET","/api/software")).sessions.find(s=>s.registration_id===registration.id),Boolean,"app-level connection");
 await checkEmbedded("first-embed");
 assert.equal(await main.getByRole("dialog",{name:"选择已运行窗口"}).count(),0);
 check("one click on WeChat connects and embeds the verified HWND without a process chooser");
 const startRect = metadata.at(-1).rect;
 const movement = await main.evaluate(() => window.__TAURI_INTERNALS__.invoke("native_browser_fixture_schedule", {action:"move_main_for_mount"}));
 assert.ok(movement.dx || movement.dy, "The fixture host must physically move");
 const moved = await until(() => native("host-move"), n => n.rect.Left === startRect.Left + movement.dx && n.rect.Top === startRect.Top + movement.dy, "docked target tracks physical host movement", 7000);
 assert.ok(moved.clientHit && moved.responds && moved.desktopParent);
 check("moving the host keeps the real WeChat client under its new mouse coordinates");
 const initial=moved.rect;
 await main.setViewportSize({width:960,height:760});
 await until(async()=>{const s=await current();return {s,n:native("resize-sample")};},v=>v.s.window.mode==="embedded"&&JSON.stringify(v.n.rect)!==JSON.stringify(initial),"embedded size follows workspace");
 check("embedded WeChat follows workspace size without content interaction");
 const generation=(await current()).generation;
 const restoreStart=performance.now();
 session=await api("POST",`/api/software/sessions/${session.id}/present`,{generation,operation_id:randomUUID(),mode:"independent"});
 timings.restoreRequestMs=Math.round(performance.now()-restoreStart);
 assertRestored("explicit-restore");await button("刷新软件状态");check("explicit restore preserves original parent/style/exstyle/placement/visibility and process");
 await main.setViewportSize({width:1366,height:844});
 await embed("before-history-dialog");await button("操作记录");await main.getByRole("dialog",{name:"软件操作记录"}).waitFor();assertRestored("dialog-restore");await main.getByRole("dialog",{name:"软件操作记录"}).getByRole("button",{name:"关闭",exact:true}).evaluate(b=>b.click());check("trusted modal restores real WeChat before opening");
 await embed("before-route-leave");await main.getByRole("link",{name:"更多",exact:true}).evaluate(element=>element.click());await until(current,s=>s.window.mode==="independent","route restoration");assertRestored("route-restore");check("leaving workbench restores real WeChat without stopping its process");
 await main.getByRole("link",{name:"软件挂载",exact:true}).evaluate(element=>element.click());await main.getByRole("heading",{name:"软件挂载",exact:true}).waitFor();await embed("before-independent-display");
 const sessionId=session.id;
 if (before.visible && !before.minimized) {
   await button("独立显示");
   session=await until(current,s=>s.window.mode==="independent"&&s.state==="active","independent display");
   assertRestored("visible-independent");
 } else {
   session=await api("POST",`/api/software/sessions/${session.id}/present`,{generation:(await current()).generation,operation_id:randomUUID(),mode:"independent"});
   assertRestored("restore-initial-hidden-or-minimized-state");
   await button("刷新软件状态");
 }
 await embed("returned-from-independent");
 assert.equal(session.id,sessionId);
 check("real WeChat detaches and returns to the same Tauri session");
 await main.evaluate(()=>window.__TAURI_INTERNALS__.invoke("native_browser_fixture_schedule",{action:"exit"}));
 await until(()=>app.exitCode,code=>code!==null,"normal host exit",15000);assert.equal(app.exitCode,0);assertRestored("normal-exit-restore");check("normal Tauri exit restores real WeChat and leaves it alive");
 assert.deepEqual(errors,[]);check("host page has no runtime errors; no UIA, screenshots, messages or chat reads were used");
} catch(error) {failure=String(error.stack||error);console.error(`[wechat-window] FAIL ${failure}`);}
finally {
 let cleanupError;
 if(app&&app.exitCode===null&&token&&session){try{session=await current();await api("POST",`/api/software/sessions/${session.id}/release`,{generation:session.generation,operation_id:randomUUID()});assertRestored("finally-release");}catch(error){cleanupError=String(error);}}
 const preserveHost=app?.exitCode===null&&session&&!restoreConfirmed;
 if(!preserveHost){if(browser)await browser.close().catch(()=>{});for(const child of children.reverse()){if(child.exitCode===null&&child.signalCode===null){child.kill();await until(()=>child.exitCode!==null||child.signalCode!==null,Boolean,"owned child cleanup",5000);}}}
 if(cleanupError&&!failure)failure=cleanupError;
 const portChecks=await Promise.all(Object.entries(ports).map(async([name,port])=>({name,port,open:await portOpen(port)})));
 if(!preserveHost&&portChecks.some(item=>item.open)&&!failure)failure="An owned service port remained open after cleanup";
 await writeFile(join(evidence,"result.json"),JSON.stringify({status:failure?"failed":"passed",failure,checks,timings,metadata,errors,ports:portChecks,restoreConfirmed,preserveHost,cleanupError,isolated,boundary:"Real WeChat window metadata and parenting only; no screenshots, titles, UIA, account actions, chat content, sending or model calls."},null,2));
 await writeFile(join(evidence,"source-end.json"),JSON.stringify(await manifest(),null,2));
 await writeFile(join(evidence,"services.log"),logs.join(""));
 console.log(`[wechat-window] ${failure?"failed":"passed"}; ${checks.length} checks; ${evidence}`);
}
if(failure)process.exitCode=1;
