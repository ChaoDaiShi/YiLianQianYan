// Synthetic HTTP fixtures only. No desktop IPC, real WeChat, chat reads or sends.
import assert from "node:assert/strict";
import {mkdtemp, writeFile} from "node:fs/promises";
import {existsSync} from "node:fs";
import {join} from "node:path";
import {tmpdir} from "node:os";
import {chromium} from "playwright-core";

const evidence=await mkdtemp(join(tmpdir(),"yilian-software-recovery-"));
console.log(`[software-recovery] evidence ${evidence}`);
const executablePath=["C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe","C:/Program Files/Microsoft/Edge/Application/msedge.exe"].find(existsSync);
assert.ok(executablePath);
const browser=await chromium.launch({executablePath,headless:true,chromiumSandbox:true});
const page=await browser.newPage({viewport:{width:1366,height:844}});
page.setDefaultTimeout(30000);
const errors=[],calls=[],checks=[];
const layout=[];
page.on("pageerror",e=>errors.push(e.message));
page.on("console",message=>{if(message.type()==="error")console.error(`[browser-console] ${message.text()}`);});
page.on("requestfailed",request=>console.error(`[browser-request] ${request.url()}: ${request.failure()?.errorText}`));
const registration={id:"controlled-registration",revision:1,name:"受控窗口重建测试",adapter:"wechat",executable:"C:/Controlled/Weixin.exe",workspace_id:null,allow_observe:false,allow_actions:false};
const identity={hwnd:"101",pid:100,process_started_at:"134381000000000001",executable:registration.executable,class_name:"ControlledQtWindow"};
const lost={id:"old-session",registration_id:registration.id,generation:9,state:"lost",launched_by_host:false,last_error:"window_closed: The selected window no longer exists",window:{identity,mode:"embedded",visible:true,focused:true,minimized:false,embedding_supported:true,reason:null}};
const replacement={...identity,hwnd:"202"};
let sessions=[structuredClone(lost)],failure;
function check(name){checks.push(name);console.log(`[software-recovery] PASS ${name}`);}
await page.addInitScript(()=>sessionStorage.setItem("yilian.setup.deferred","true"));
await page.route("**/api/**",async route=>{
 const request=route.request(),path=new URL(request.url()).pathname,method=request.method();
 if(!path.startsWith("/api/"))return route.continue();
 const headers={"Access-Control-Allow-Origin":"*","Access-Control-Allow-Headers":"*","Access-Control-Allow-Methods":"GET,POST,PUT,DELETE,OPTIONS"};
 if(method==="OPTIONS")return route.fulfill({status:204,headers});
 const reply=body=>route.fulfill({json:body,headers});
 if(path.startsWith("/api/software"))calls.push({path,method,body:method==="GET"?null:request.postDataJSON()});
 if(path==="/api/software")return reply({available:true,registrations:[registration],sessions,history:[]});
 if(path.endsWith("/discover"))return reply({candidates:[{identity:replacement,visible:true,minimized:false},{identity:{...replacement,hwnd:"303"},visible:false,minimized:false}]});
 if(path.endsWith("/attach")){
  assert.deepEqual(request.postDataJSON().identity,replacement);
  const attached={...lost,id:"new-session",generation:1,state:"active",last_error:null,window:{...lost.window,identity:replacement,mode:"independent",focused:false}};
  sessions.push(attached);return reply(attached);
 }
 if(path.endsWith("/present")){
  assert.equal(path,"/api/software/sessions/new-session/present","No presentation request may target the lost window");
  const current=sessions.find(s=>s.id==="new-session");current.window.mode=request.postDataJSON().mode;current.generation++;return reply(current);
 }
 if(path==="/api/workspaces")return reply({workspaces:[]});
 if(path==="/api/voice/session")return reply(null);
 if(path==="/api/conversations"||path==="/api/llm/models")return reply([]);
 if(path==="/api/chat/readiness")return reply({configured:false,available:false,provider:"",model:""});
 if(path.endsWith("/features"))return reply({features:[]});
 return reply({});
});
await page.route("**/src/api/controlSession.ts*",route=>route.fulfill({contentType:"application/javascript",body:'export const CONTROL_SESSION_HEADER="X-Yilian-Control-Session";export async function initializeControlSession(){};export function controlSessionHeaders(){return {[CONTROL_SESSION_HEADER]:"controlled-recovery-fixture-placeholder"};}'}));
try{
 await page.goto((process.env.YILIAN_SOFTWARE_FRONTEND||"http://localhost:1420")+"/software",{waitUntil:"commit"});
 await page.getByRole("button",{name:"重新连接软件",exact:true}).waitFor();
 const viewport=page.getByLabel("软件原生显示区域",{exact:true});
 assert.equal(await viewport.getAttribute("data-native-mode"),"none");
 assert.equal(await page.getByText("原窗口已失效",{exact:true}).count(),1);
 assert.equal((await viewport.textContent()).trim(),"");
 assert.ok(!(await page.locator(".software-session-state").textContent()).includes("主窗嵌入"));
 check("lost session with cached embedded/visible flags renders an unavailable window, never a connected viewport");
 for(const width of [1366,960,390]){
  await page.setViewportSize({width,height:844});
  const bounds=await page.evaluate(()=>({width:document.documentElement.clientWidth,scroll:document.documentElement.scrollWidth}));
  assert.ok(bounds.scroll<=bounds.width+2);
  await page.screenshot({path:join(evidence,`lost-${width}.png`),animations:"disabled"});
 }
 check("unavailable-state layout fits 1366/960/390 CSS widths");
 await page.setViewportSize({width:1366,height:844});
 await page.getByRole("button",{name:"重新连接软件",exact:true}).click();
 const dialog=page.getByRole("dialog",{name:"选择已运行窗口",exact:true});await dialog.waitFor();
 assert.equal(calls.filter(c=>c.path.endsWith("/present")).length,0);
 assert.equal(await dialog.getByRole("radio").count(),2);
 assert.equal(await dialog.getByRole("radio").first().isChecked(),false);
 assert.equal(await dialog.getByRole("button",{name:"连接所选窗口",exact:true}).isDisabled(),true);
 check("reselect opens discovery without restoring the vanished HWND or automatically choosing a replacement");
 await dialog.locator(".software-candidate").filter({hasText:"HWND 202"}).getByRole("radio").check();
 await dialog.getByRole("button",{name:"连接所选窗口",exact:true}).click();await dialog.waitFor({state:"hidden"});
 await page.getByRole("button",{name:"嵌回忆涟千言",exact:true}).click();
 await page.waitForFunction(()=>document.querySelector('[aria-label="软件原生显示区域"]')?.getAttribute("data-native-mode")==="embedded");
 assert.ok((await viewport.textContent()).includes("窗口会恢复独立显示"));
 assert.equal(calls.filter(c=>c.path.endsWith("/attach")).length,1);
 check("explicit selection creates a new session and embeds only its new HWND");
 if(process.argv.includes("--compact")){
  for(const width of [1366,960,390]){
   await page.setViewportSize({width,height:844});
   await page.waitForTimeout(180);
   const geometry=await page.evaluate(()=>{const main=document.querySelector('.software-main').getBoundingClientRect(),panel=document.querySelector('.software-session-panel').getBoundingClientRect(),view=document.querySelector('.software-viewport').getBoundingClientRect();return {main:{height:main.height,width:main.width},panel:{height:panel.height},view:{height:view.height,width:view.width,top:view.top},client:document.documentElement.clientWidth,scroll:document.documentElement.scrollWidth};});
   layout.push({width,...geometry});
   await page.screenshot({path:join(evidence,`compact-${width}.png`),animations:"disabled"});
   assert.ok(geometry.scroll<=geometry.client+2,`No horizontal overflow at ${width}`);
   assert.ok(geometry.view.height/geometry.main.height>=(width>=960?0.7:0.55),`Native display must dominate: ${JSON.stringify({width,...geometry})}`);
  }
  check("native display occupies at least 70% of desktop work area and remains dominant at 390 CSS width");
  await page.setViewportSize({width:1366,height:844});
  await page.getByRole("button",{name:"会话详情",exact:true}).click();
  const details=page.getByRole("dialog",{name:"软件会话详情",exact:true});await details.waitFor();
  assert.ok((await details.textContent()).includes(registration.executable));
  assert.ok((await details.textContent()).includes("附着已有进程；仍由用户管理"));
  await details.getByRole("button",{name:"关闭",exact:true}).click();await details.waitFor({state:"hidden"});
  assert.equal(await viewport.getAttribute("data-native-mode"),"independent");
  await page.getByRole("button",{name:"嵌回忆涟千言",exact:true}).click();
  await page.waitForFunction(()=>document.querySelector('[aria-label="软件原生显示区域"]')?.getAttribute("data-native-mode")==="embedded");
  check("details remain in a modal and preserve restoration before opening plus explicit re-embedding after close");
 }
 assert.deepEqual(errors,[]);check("no page runtime errors");
}catch(error){failure=String(error.stack||error);console.error(failure);await writeFile(join(evidence,"failure-body.txt"),await page.locator("body").innerText());await page.screenshot({path:join(evidence,"failure.png")});}
finally{await writeFile(join(evidence,"result.json"),JSON.stringify({status:failure?"failed":"passed",failure,checks,calls,errors,layout,boundary:"Mock API and synthetic identities; no native window or real WeChat interaction."},null,2));await browser.close();}
if(failure)process.exitCode=1;
