import assert from "node:assert/strict";
import {createServer} from "node:http";
import {writeFile,readFile} from "node:fs/promises";
import {join,resolve} from "node:path";
import {execFileSync} from "node:child_process";

export async function workbenchRevision5({page,frontendPort,backendPort,controlToken,evidenceDir,workspace}) {
 assert.ok(resolve(workspace).toLowerCase().startsWith("f:\\target\\"),"fixture must be isolated off checkout");
 const origin=`http://127.0.0.1:${frontendPort}`,base=`http://127.0.0.1:${backendPort}`;
 const headers={"Content-Type":"application/json","X-Yilian-Control-Session":controlToken};
 const facts={status:"PASS",checks:[],requests:[],composer:{}};
 async function api(method,path,body){const response=await fetch(base+path,{method,headers,body:body===undefined?undefined:JSON.stringify(body)});const raw=await response.text();assert.ok(response.ok,`${method} ${path} ${response.status} ${raw.slice(0,300)}`);return raw?JSON.parse(raw):null;}
 async function eventually(read,valid,label,timeout=20000){let value;const end=Date.now()+timeout;do{value=await read();if(valid(value))return value;await new Promise(r=>setTimeout(r,100));}while(Date.now()<end);assert.fail(`${label}: ${JSON.stringify(value)}`);}
 const navigate=async path=>{await page.goto(origin+path);await page.locator(".nav-rail").waitFor();};
 const shot=async name=>{await page.waitForTimeout(400);await page.screenshot({path:join(evidenceDir,`revision5-${name}.png`)});};
 const git=(...args)=>execFileSync("git",args,{cwd:workspace,encoding:"utf8",windowsHide:true});
 git("init","-b","qa-workbench");
 await writeFile(join(workspace,"notes.txt"),"Initial notes\n");
 await writeFile(join(workspace,"staged.txt"),"Initial stage\n");
 git("add","--","notes.txt","staged.txt");git("-c","user.name=Workbench QA","-c","user.email=qa@local.invalid","commit","-m","QA fixture");
 await writeFile(join(workspace,"staged.txt"),"Prepared stage\n");git("add","--","staged.txt");
 await writeFile(join(workspace,"设计草案.md"),"# 待整理的设计\n");
 const indexBefore=await readFile(join(workspace,".git/index"));
 let sequence=0;
 const provider=createServer(async(req,res)=>{
  let raw="";for await(const chunk of req)raw+=chunk;const body=JSON.parse(raw);
  if(!body.stream){res.writeHead(200,{"Content-Type":"application/json"});res.end(JSON.stringify({id:"verify",choices:[{index:0,message:{role:"assistant",content:"pong"},finish_reason:"stop"}]}));return;}
  const last=body.messages.findLastIndex(message=>message.role==="user");const question=body.messages[last]?.content??"";
  const hasTool=body.messages.slice(last+1).some(message=>message.role==="tool");
  const isWork=question.includes("工作说明")||question.includes("后台写入");const isCreate=question.includes("短文");
  const wantsTool=(isWork||isCreate)&&!hasTool;
  facts.requests.push({question,userHistory:body.messages.filter(m=>m.role==="user").map(m=>m.content),hasTool,model:body.model});
  if(hasTool)await new Promise(r=>setTimeout(r,1200));
  const delta=wantsTool?{content:"正在生成并保存文件。",tool_calls:[{index:0,id:`r5-tool-${++sequence}`,type:"function",function:{name:"write_file",arguments:JSON.stringify({path:isCreate?"创作.md":question.includes("后台写入")?"background.txt":"notes.txt",content:isCreate?"花瓣落在水面，新的想法缓缓展开。":"工作说明已实际写入。"})}}]}:{content:question==="你好"?"你好，我在。":hasTool?"文件已经保存并检查完成。":`分支回答：${question}`};
  res.writeHead(200,{"Content-Type":"text/event-stream"});res.end(`data: ${JSON.stringify({choices:[{index:0,delta,finish_reason:null}]})}\n\ndata: ${JSON.stringify({choices:[{index:0,delta:{},finish_reason:wantsTool?"tool_calls":"stop"}],usage:{prompt_tokens:20,completion_tokens:10,total_tokens:30}})}\n\ndata: [DONE]\n\n`);
 });
 await new Promise(r=>provider.listen(0,"127.0.0.1",r));
 try{
  const model=await api("POST","/api/llm/models",{provider:"local",label:"桌面验收模型",model:"workbench-model",base_url:`http://127.0.0.1:${provider.address().port}/v1`,api_format:"openai",api_key:"controlled-test-credential",temperature:0,max_tokens:1024,invoke_timeout_ms:15000});
  await api("POST",`/api/llm/models/${model.id}/verify`);await api("POST",`/api/llm/models/${model.id}/activate`);
  await api("PUT","/api/security/approval-mode",{mode:"partial_approval"});
  await navigate("/settings?section=appearance");await page.getByRole("radio",{name:/白天/}).click();
  await navigate("/chat");
  const readMode=()=>page.locator(".compact-composer").getAttribute("data-workspace-mode");
  await eventually(readMode,value=>value==="chat","chat mode");
  facts.composer.chat=await page.locator(".compact-composer .composer-card").evaluate(el=>({height:el.getBoundingClientRect().height,width:el.getBoundingClientRect().width}));
  assert.ok(facts.composer.chat.height>=50&&facts.composer.chat.height<=62,`chat height ${facts.composer.chat.height}`);
  assert.equal(await page.locator(".compact-composer select").count(),0);
  await shot("compact-chat");facts.checks.push("single_line_chat_composer_and_collapsed_controls");
  async function send(text){await page.getByRole("textbox",{name:"聊天输入",exact:true}).fill(text);await page.getByRole("button",{name:"发送消息",exact:true}).click();await page.waitForURL(/\/chat\/.+/);return new URL(page.url()).pathname.split("/").at(-1);}
  const id=await send("你好");
  await page.getByText("你好，我在。",{exact:true}).waitFor();
  assert.equal(await readMode(),"chat");
  await send("请整理并保存工作说明");
  await eventually(readMode,value=>value==="work","actual tool automatically switches to work");
  await eventually(()=>api("GET",`/api/conversations/${id}`),v=>v.run_status==="completed"&&v.workspace_mode==="work","work finished persisted mode");
  assert.equal(await readFile(join(workspace,"notes.txt"),"utf8"),"工作说明已实际写入。");
  facts.composer.work=await page.locator(".compact-composer .composer-card").evaluate(el=>({height:el.getBoundingClientRect().height,width:el.getBoundingClientRect().width}));
  assert.ok(facts.composer.work.height<=132,`work height ${facts.composer.work.height}`);
  await page.getByRole("complementary",{name:"Git 文件变更",exact:true}).waitFor();
  await page.getByRole("button",{name:"查看工作区差异 notes.txt",exact:true}).waitFor();
  await shot("work-git");facts.checks.push("chat_to_work_on_actual_tool","work_composer_compact","git_actual_change_list");
  await page.getByRole("button",{name:"查看工作区差异 notes.txt",exact:true}).click();
  await page.getByRole("dialog").getByLabel("文件差异内容").filter({hasText:"+工作说明已实际写入。"}).waitFor();
  await shot("git-diff");
  await page.getByRole("dialog").getByRole("button",{name:/^关闭/}).first().click();
  assert.deepEqual(await readFile(join(workspace,".git/index")),indexBefore);facts.checks.push("git_diff_matches_file_and_keeps_index");
  async function mode(label,expected){await page.getByRole("button",{name:"切换工作模式",exact:true}).click();await page.getByRole("dialog",{name:"切换工作模式",exact:true}).getByRole("button",{name:new RegExp(`^${label}`)}).click();await eventually(readMode,value=>value===expected,`manual ${expected}`);}
  await mode("创造","create");
  await send("创作一段短文并保存");
  await eventually(()=>api("GET",`/api/conversations/${id}`),v=>v.run_status==="completed"&&v.workspace_mode==="create"&&v.messages.some(m=>m.role==="user"&&m.content==="创作一段短文并保存"),"create mode actual artifact");
  assert.ok((await readFile(join(workspace,"创作.md"),"utf8")).includes("花瓣"));
  await page.reload();await eventually(readMode,value=>value==="create","create mode survives reload");
  await shot("create-mode");facts.checks.push("manual_create_real_file_and_mode_persistence");
  await mode("聊天","chat");
  await page.getByRole("link",{name:"画布",exact:true}).click();
  await page.locator(".conversation-canvas-group").waitFor();
  const original=await api("GET",`/api/conversations/${id}`);const sourceMessage=original.messages.find(m=>m.role==="assistant"&&m.content==="你好，我在。");assert.ok(sourceMessage);
  const overview=await api("GET","/api/message-canvas");assert.equal(overview.conversations.length,1);assert.equal((await api("GET","/api/connectors")).canvases.length,0);
  assert.ok(overview.conversations[0].nodes.some(node=>node.content==="你好，我在。"));facts.checks.push("all_homepage_history_visible_without_binding_or_canvas_creation");
  await page.locator(`[data-id="${sourceMessage.id}"]`).getByRole("button",{name:"从此回答创建分支对话",exact:true}).click();
  await page.getByRole("textbox",{name:"分支对话提问",exact:true}).fill("从这里展开一个新问题");await page.getByRole("button",{name:"发送到新分支",exact:true}).click();
  const branch=await eventually(()=>api("GET","/api/message-canvas"),v=>v.conversations.length===2&&v.conversations.some(c=>c.branch&&c.status==="completed"),"branch conversation persisted");
  await eventually(()=>page.locator(".conversation-canvas-group").count(),v=>v===2,"visible two conversation groups");
  await page.locator(".conversation-canvas-branch-edge").first().waitFor();
  assert.equal(branch.conversations.find(c=>c.branch).branch.parent_node_id,sourceMessage.id);
  const branchRequest=facts.requests.find(req=>req.question==="从这里展开一个新问题");assert.ok(branchRequest);assert.deepEqual(branchRequest.userHistory,["你好","从这里展开一个新问题"]);
  assert.deepEqual((await api("GET",`/api/conversations/${id}`)).messages,original.messages);
  await eventually(()=>page.locator(".conversation-canvas-branch-edge .react-flow__edge-path").first().getAttribute("d"),value=>value?.includes("C"),"curved branch connection");facts.checks.push("curved_branch_connections");
  await page.getByRole("button",{name:"取消",exact:true}).click();
  await page.getByRole("button",{name:"关闭消息详情",exact:true}).click();
  await page.getByRole("combobox",{name:"定位画布对话",exact:true}).selectOption(id);await shot("conversation-groups-branch");facts.checks.push("separate_branch_group_connected_to_actual_answer","branch_keeps_ancestry_excludes_sibling_and_source_unchanged");
  await page.reload();await eventually(()=>page.locator(".conversation-canvas-group").count(),v=>v===2,"groups survive refresh");
  facts.checks.push("conversation_group_state_and_relationships_persist");
  await navigate("/settings");
  await eventually(()=>page.locator('.settings-canvas-stage .react-flow__node [role="button"]').count(),v=>v===7,"six grouped settings and center");
  await shot("settings-groups");
  await page.getByRole("button",{name:"编辑模型与服务",exact:true}).click();
  assert.equal(await page.getByRole("dialog",{name:"模型与服务",exact:true}).locator(".settings-group-card").count(),5);
  await shot("settings-second-level");
  await page.getByRole("button",{name:"配置对话模型",exact:true}).click();
  await page.getByRole("dialog",{name:"对话模型与模型池",exact:true}).waitFor();
  await page.getByRole("dialog").getByRole("button",{name:"关闭",exact:true}).click();
  await page.getByRole("dialog",{name:"模型与服务",exact:true}).waitFor();
  await page.getByRole("button",{name:"关闭模型与服务",exact:true}).click();
  await page.getByRole("button",{name:"展开节点管理器",exact:true}).click();
  assert.equal(await page.locator(".settings-node-manager details[open]").count(),0);
  await page.getByLabel("展开模型与服务设置",{exact:true}).click();await shot("settings-group-manager");facts.checks.push("six_first_level_settings_groups","second_level_actual_forms_and_return","collapsed_grouped_node_manager");
  await api("PUT","/api/security/approval-mode",{mode:"full_approval"});
  await navigate(`/chat/${id}`);await send("审批后后台写入");
  await page.getByRole("button",{name:"查看待确认",exact:true}).waitFor();
  await page.getByRole("link",{name:"能力",exact:true}).click();
  await navigate(`/chat/${id}`);await page.getByRole("button",{name:"查看待确认",exact:true}).click();await page.getByRole("button",{name:"允许此次工具操作",exact:true}).click();
  await page.reload();
  await eventually(()=>api("GET",`/api/conversations/${id}`),v=>v.run_status==="completed","background after approval refresh");
  assert.equal(await readFile(join(workspace,"background.txt"),"utf8"),"工作说明已实际写入。");facts.checks.push("background_approval_recovery_regression");
  await writeFile(join(evidenceDir,"revision5-facts.json"),JSON.stringify(facts,null,2));return facts;
 }finally{await new Promise(r=>provider.close(r));}
}
