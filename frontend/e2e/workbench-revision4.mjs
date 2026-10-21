import assert from "node:assert/strict";
import {createServer} from "node:http";
import {writeFile,readFile} from "node:fs/promises";
import {join} from "node:path";
import {execFileSync} from "node:child_process";

export async function workbenchRevision4({page,frontendPort,backendPort,controlToken,evidenceDir,workspace}) {
 const origin=`http://127.0.0.1:${frontendPort}`,base=`http://127.0.0.1:${backendPort}`;
 const headers={"Content-Type":"application/json","X-Yilian-Control-Session":controlToken};
 const facts={checks:[],requests:[]};
 async function api(method,path,body){const r=await fetch(base+path,{method,headers,body:body===undefined?undefined:JSON.stringify(body)});const raw=await r.text();assert.ok(r.ok,`${method} ${path} ${r.status} ${raw.slice(0,300)}`);return raw?JSON.parse(raw):null;}
 async function eventually(read,valid,label,ms=20000){const end=Date.now()+ms;let value;do{value=await read();if(valid(value))return value;await new Promise(r=>setTimeout(r,100));}while(Date.now()<end);assert.fail(`${label}: ${JSON.stringify(value)}`);}
 const shot=async name=>{await page.waitForTimeout(450);return page.screenshot({path:join(evidenceDir,`revision4-${name}.png`)});};
 const navigate=async path=>{await page.goto(origin+path);await page.locator(".nav-rail").waitFor();};
 let requestSequence=0;
 const provider=createServer(async(req,res)=>{
  let raw="";for await(const c of req)raw+=c;const body=JSON.parse(raw);
  const question=body.messages.filter(m=>m.role==="user").at(-1)?.content??"";
  if(!body.stream){res.writeHead(200,{"Content-Type":"application/json"});res.end(JSON.stringify({id:"qa-verify",choices:[{index:0,message:{role:"assistant",content:"pong"},finish_reason:"stop"}],usage:{prompt_tokens:1,completion_tokens:1,total_tokens:2}}));return;}
  const tool=body.messages.some(m=>m.role==="tool");
  facts.requests.push({model:body.model,thinking:body.thinking,reasoning:body.messages.some(m=>m.reasoning_content),question,tool});
  if(tool)await new Promise(r=>setTimeout(r,2500));
  const failure=question.includes("错误日志");
  const call={id:`qa-tool-${++requestSequence}`,type:"function",function:{name:failure?"read_file":"write_file",arguments:JSON.stringify(failure?{path:"missing-qa-diagnostic.txt"}:{path:`qa-background-${question.includes("刷新")?"refresh":"route"}.txt`,content:"BACKGROUND_EXECUTION_OK"})}};
  const delta=tool?{content:`${question}：后台工作已完成。`}:{content:"正在处理工具操作。",reasoning_content:"检查执行条件。",tool_calls:[{index:0,...call}]};
  res.writeHead(200,{"Content-Type":"text/event-stream"});
  res.end(`data: ${JSON.stringify({choices:[{index:0,delta,finish_reason:null}]})}\n\ndata: ${JSON.stringify({choices:[{index:0,delta:{},finish_reason:tool?"stop":"tool_calls"}],usage:{prompt_tokens:30,completion_tokens:10,total_tokens:40}})}\n\ndata: [DONE]\n\n`);
 });
 await new Promise(r=>provider.listen(0,"127.0.0.1",r));
 try{
  const model=await api("POST","/api/llm/models",{provider:"deepseek",label:"验收模型 A",model:"deepseek-chat",base_url:`http://127.0.0.1:${provider.address().port}/v1`,api_format:"openai",api_key:"controlled-test-credential",temperature:0,max_tokens:1024,invoke_timeout_ms:15000});
  await api("POST",`/api/llm/models/${model.id}/verify`);
  const defaultModel=await api("POST","/api/llm/models",{provider:"deepseek",label:"默认模型",model:"deepseek-v4-flash",base_url:`http://127.0.0.1:${provider.address().port}/v1`,api_format:"openai",api_key:"controlled-test-credential",temperature:0,max_tokens:1024,invoke_timeout_ms:15000});
  await api("POST",`/api/llm/models/${defaultModel.id}/verify`);
  await api("POST",`/api/llm/models/${defaultModel.id}/activate`);
  await navigate("/settings?section=appearance");await page.getByRole("radio",{name:/白天/}).click();
  await navigate("/chat");
  const labels=await page.locator(".nav-rail a").allTextContents();assert.ok(labels[0].includes("主页"));assert.ok(labels.some(v=>v.includes("画布")));facts.checks.push("homepage_first_canvas_renamed");
  async function setMode(mode){await page.getByRole("combobox",{name:"审批模式",exact:true}).selectOption(mode);await eventually(()=>api("GET","/api/security/approval-mode"),v=>v.mode===mode,"approval mode saved");}
  await setMode("full_approval");
  await page.getByRole("combobox",{name:"对话模型",exact:true}).selectOption(model.id);
  await page.getByRole("combobox",{name:"思考模式",exact:true}).selectOption("thinking");
  async function send(text){const field=page.locator("textarea").last();await field.fill(text);await page.getByRole("button",{name:"发送消息",exact:true}).click();await page.waitForURL(/\/chat\/.+/);return new URL(page.url()).pathname.split("/").at(-1);}
  const id=await send("切换页面验证");
  await page.getByRole("button",{name:"查看待确认",exact:true}).waitFor();
  await page.getByRole("link",{name:"能力",exact:true}).click();
  await page.getByRole("link",{name:"主页",exact:true}).click();
  await page.locator(".conversation-row").getByRole("button",{name:"切换页面验证",exact:true}).click();
  await page.getByRole("button",{name:"查看待确认",exact:true}).click();
  await page.getByRole("button",{name:"允许此次工具操作",exact:true}).waitFor();
  await shot("approval-restored");facts.checks.push("pending_approval_survives_navigation");
  await page.getByRole("button",{name:"允许此次工具操作",exact:true}).click();
  await eventually(()=>api("GET",`/api/conversations/${id}`),v=>v.execution_history.some(r=>r.executionStatus==="succeeded"),"tool actually executed");
  await page.getByRole("link",{name:"系统与模型设置",exact:true}).click();
  await eventually(()=>api("GET",`/api/conversations/${id}`),v=>v.run_status==="completed","background completion");
  await navigate(`/chat/${id}`);
  await page.getByText("切换页面验证：后台工作已完成。",{exact:true}).waitFor();
  if(workspace)assert.equal(await readFile(join(workspace,"qa-background-route.txt"),"utf8"),"BACKGROUND_EXECUTION_OK");
  assert.ok(facts.requests.some(r=>r.model==="deepseek-chat"&&r.thinking?.type==="enabled"));
  assert.ok(facts.requests.some(r=>r.tool&&r.reasoning));
  assert.ok(facts.requests.filter(r=>r.question==="切换页面验证").every(r=>r.model==="deepseek-chat"));
  const persistedModels=await api("GET","/api/llm/models");assert.equal(persistedModels.find(row=>row.active)?.id,defaultModel.id);
  facts.modelSelection={default:"deepseek-v4-flash",selected:"deepseek-chat",globalDefaultPreserved:true};facts.checks.push("approval_resume_background_completion_real_file","selected_model_thinking_and_reasoning_roundtrip");
  await shot("home-completed");

  await navigate("/chat");
  const refreshId=await send("刷新页面验证");
  await page.getByRole("button",{name:"查看待确认",exact:true}).waitFor();
  await page.reload();
  await page.getByRole("button",{name:"查看待确认",exact:true}).click();
  await page.getByRole("button",{name:"允许此次工具操作",exact:true}).click();
  await page.reload();
  await eventually(()=>api("GET",`/api/conversations/${refreshId}`),v=>v.run_status==="completed","approval continuation survives dropped SSE");
  await page.getByText("刷新页面验证：后台工作已完成。",{exact:true}).waitFor();
  facts.checks.push("hard_reload_recovers_approval_and_running_completion");

  await navigate("/chat");await setMode("partial_approval");
  const errorId=await send("错误日志验证");
  await eventually(()=>api("GET",`/api/conversations/${errorId}`),v=>v.run_status==="completed","partial approval auto low risk");
  const errors=await api("GET","/api/logs/tools?status=error");assert.ok(errors.entries.some(e=>e.conversationId===errorId&&e.name==="read_file"));
  await navigate("/logs");await page.getByText("read_file",{exact:true}).first().waitFor();await shot("tool-error-log");facts.checks.push("partial_approval_low_risk_runs","persistent_tool_error_diagnostics");
  await navigate("/chat");await setMode("full_access");
  assert.equal((await api("GET","/api/security/approval-mode")).mode,"full_access");
  facts.checks.push("three_permission_modes_persist");

  const agent=await api("POST","/api/agents",{name:"画布智能体",user_prompt:"使用中文回答。",allowed_tools:["read_file","write_file"],capabilities:[]});
  const canvas=await api("POST","/api/connectors",{name:"主页消息画布",agent_id:agent.id});
  await navigate(`/chat/${id}`);
  await page.getByRole("link",{name:"在画布查看",exact:true}).click();
  await page.getByRole("combobox",{name:"会话目标画布",exact:true}).selectOption(canvas.canvas_id);
  await page.getByRole("button",{name:"连接会话并打开",exact:true}).click();
  await page.locator(".message-branch-node").first().waitFor();
  assert.ok(await page.locator(".message-branch-node").count()>=2);
  await shot("linked-message-canvas");
  await page.getByRole("link",{name:"在主页打开对话",exact:true}).click();
  assert.equal(new URL(page.url()).pathname,`/chat/${id}`);
  await page.getByRole("link",{name:"在画布查看",exact:true}).click();
  await page.getByRole("combobox",{name:"当前画布",exact:true}).waitFor();
  await eventually(()=>page.getByRole("combobox",{name:"当前画布",exact:true}).inputValue(),value=>value===canvas.canvas_id,"bound conversation canvas resolved");
  await page.getByRole("button",{name:"删除当前画布",exact:true}).click();
  await page.getByRole("button",{name:"确认删除",exact:true}).click();
  await eventually(()=>api("GET","/api/connectors"),v=>v.canvases.length===0,"canvas deleted");
  assert.ok((await api("GET",`/api/conversations/${id}`)).messages.length>0);
  facts.checks.push("homepage_canvas_two_way_links","canvas_delete_preserves_conversation");

  await navigate("/capabilities");await page.getByRole("button",{name:/^Skill/}).click();
  const dialog=page.getByRole("dialog");await dialog.waitFor();assert.equal(await dialog.getByRole("tablist").count(),0);
  assert.ok((await dialog.boundingBox()).width<=740);await shot("capability-dialog");facts.checks.push("capability_dialog_has_no_category_tabs_narrow_width");
  await navigate("/settings?section=appearance");
  const font=page.getByRole("slider",{name:/字体|字号/});await font.focus();await font.press("Home");for(let i=0;i<6;i++)await font.press("ArrowRight");
  await eventually(()=>page.evaluate(()=>getComputedStyle(document.documentElement).fontSize),v=>v==="18px","font applies");
  await page.reload();await eventually(()=>page.evaluate(()=>getComputedStyle(document.documentElement).fontSize),v=>v==="18px","font persists");
  await font.focus();await font.press("Home");for(let i=0;i<2;i++)await font.press("ArrowRight");assert.ok((await page.getByRole("dialog").boundingBox()).width<=560);await shot("appearance-font");facts.checks.push("theme_font_size_persisted_narrow_settings_dialog");

  const manifest={schema_version:1,id:"qa.settings",name:"工作区插件",version:"1",contributes:{settings:[{id:"display",title:"显示偏好",description:"管理插件展示行为",fields:[{key:"enabled",label:"显示插件提示",kind:"toggle",default:true},{key:"layout",label:"布局",kind:"select",default:"compact",options:["compact","normal"]}]}]}};
  const zip=execFileSync("python",["-c","import sys,io,zipfile,base64; b=io.BytesIO(); z=zipfile.ZipFile(b,'w'); z.writestr('plugin.json',sys.stdin.buffer.read()); z.close(); print(base64.b64encode(b.getvalue()).decode())"],{input:JSON.stringify(manifest),encoding:"utf8",windowsHide:true}).trim();
  const preview=await api("POST","/api/capabilities/imports/inspect",{format:"zip",data_base64:zip});
  const installed=await api("POST","/api/capabilities/imports/confirm",{preview_id:preview.preview_id});
  await api("PUT","/api/capabilities/imports/qa.settings",{revision:installed.revision,action:"enable"});
  await navigate("/settings");
  assert.equal(await page.getByRole("button",{name:"展开节点管理器",exact:true}).getAttribute("aria-expanded"),"false");
  const data=await api("GET","/api/settings/canvas");assert.equal(data.state.connected_node_ids.length,14);
  await page.getByRole("button",{name:"展开节点管理器",exact:true}).click();
  await page.getByRole("region",{name:"插件设置",exact:true}).waitFor();
  const entry=page.locator(".settings-manager-node").filter({hasText:"工作区插件 · 显示偏好"});
  await entry.getByRole("button",{name:/^连接/}).click();await shot("system-plugin-node-manager");
  await entry.getByRole("button",{name:"配置",exact:true}).click();
  await page.getByRole("checkbox",{name:"显示插件提示",exact:true}).uncheck();
  await page.getByRole("button",{name:"保存设置",exact:true}).click();
  await eventually(()=>api("GET","/api/plugins/qa.settings/settings/display"),v=>v.values.enabled===false,"plugin settings persist");
  await shot("plugin-settings");facts.checks.push("all_system_settings_default_nodes","plugin_settings_discovered_connected_edited_persisted");
  await navigate("/settings?section=appearance");await page.getByRole("radio",{name:/夜间/}).click();
  await navigate(`/chat/${id}`);
  await page.getByText("切换页面验证：后台工作已完成。",{exact:true}).waitFor();
  for(const selector of [".composer-card",".composer-textarea",".conversation-composer"]){assert.equal(await page.locator(selector).first().evaluate(node=>getComputedStyle(node).backgroundColor),"rgba(0, 0, 0, 0)");}
  await shot("home-dark");facts.checks.push("light_dark_transparent_composer");
  assert.equal(await page.locator("vite-error-overlay").count(),0);
  await writeFile(join(evidenceDir,"revision4-facts.json"),JSON.stringify(facts,null,2));
  return {status:"PASS",real_backend:true,real_tool_execution:true,controlled_provider:true,...facts};
 }finally{await new Promise(r=>provider.close(r));}
}
