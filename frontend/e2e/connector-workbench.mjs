import assert from "node:assert/strict";
import { createServer } from "node:http";
import { writeFile,mkdir } from "node:fs/promises";
import { join } from "node:path";
export async function connectorWorkbench({page,frontendPort,backendPort,controlToken,evidenceDir,workspace}){
 const base=`http://127.0.0.1:${backendPort}`,origin=`http://127.0.0.1:${frontendPort}`;
 async function api(method,path,body){const response=await fetch(base+path,{method,headers:{"Content-Type":"application/json","X-Yilian-Control-Session":controlToken},body:body===undefined?undefined:JSON.stringify(body)});const text=await response.text();assert.ok(response.ok,`${method} ${path}: ${response.status} ${text.slice(0,300)}`);return text?JSON.parse(text):null;}
 const agent=await api("POST","/api/agents",{name:"验收助手",user_prompt:"回答时使用中文。",allowed_tools:[],capabilities:[]});
 const conversation=await api("POST","/api/conversations",{title:"连接器验收"});
 const a=await api("POST","/api/connectors",{name:"日常工作",agent_id:agent.id});
 const b=await api("POST","/api/connectors",{name:"项目演示",agent_id:agent.id});
 const c=await api("POST","/api/connectors",{name:"学习资料",agent_id:agent.id});
 await api("POST",`/api/connectors/${a.canvas_id}/bindings`,{kind:"conversation_agent",source_id:`conversation:${conversation.id}`,target_id:`agent:${agent.id}`});
 const memory=await api("POST","/api/memories",{content:"偏好中文，保留原始文件",category:"preference",source:"manual"});
 await api("POST",`/api/connectors/${a.canvas_id}/bindings`,{kind:"agent_memory",source_id:`agent:${agent.id}`,target_id:`memory:${memory.id}`});
 const knowledge=await api("POST","/api/memories",{content:"桌面工作区资料",category:"knowledge",source:"manual"});
 await api("POST",`/api/connectors/${a.canvas_id}/bindings`,{kind:"agent_knowledge",source_id:`agent:${agent.id}`,target_id:`knowledge:${knowledge.id}`});
 const resources=(await api("GET","/api/connectors/resources")).resources;
 for(const kind of ["tool","skill"]){const resource=resources.find(r=>r.kind===kind);if(resource)await api("POST",`/api/connectors/${a.canvas_id}/bindings`,{kind:"agent_capability",source_id:`agent:${agent.id}`,target_id:resource.id});}
 await page.goto(`${origin}/more`);await page.getByRole("heading",{name:"更多",exact:true}).waitFor();
 assert.equal(await page.locator(".nav-rail a").filter({hasText:"搜索"}).count(),0);
 assert.equal(await page.locator(".nav-rail a").filter({hasText:"工作流"}).count(),0);
 const order=await page.locator(".navigation-card-grid .navigation-entry").evaluateAll(nodes=>nodes.map(n=>({title:n.querySelector("h3")?.textContent,x:n.getBoundingClientRect().x,y:n.getBoundingClientRect().y})));
 assert.ok(order.length>=6);assert.ok(order[0].x<order[1].x&&order[1].x<order[2].x);assert.equal(order[3].x,order[0].x);assert.ok(order[3].y>order[0].y);
 await page.waitForTimeout(250);await page.screenshot({path:join(evidenceDir,"more.png")});
 await page.goto(`${origin}/capabilities`);await page.getByRole("button",{name:/Skill/}).first().waitFor();
 assert.equal(await page.getByRole("dialog").count(),0);
 const capabilityOrder=await page.locator(".capability-navigation-card").evaluateAll(nodes=>nodes.map(node=>({x:node.getBoundingClientRect().x,y:node.getBoundingClientRect().y})));
 assert.equal(capabilityOrder.length,6);assert.ok(capabilityOrder[0].x<capabilityOrder[1].x&&capabilityOrder[1].x<capabilityOrder[2].x);assert.equal(capabilityOrder[3].x,capabilityOrder[0].x);
 await page.waitForTimeout(250);await page.screenshot({path:join(evidenceDir,"capability-navigation.png")});
 await page.getByRole("button",{name:/Skill/}).first().click();await page.getByRole("dialog").first().waitFor();
 await page.waitForTimeout(250);await page.screenshot({path:join(evidenceDir,"capabilities.png")});
 await page.goto(`${origin}/connectors`);await page.getByRole("heading",{name:"连接器",exact:true}).waitFor();
 await page.locator(".react-flow__node").first().waitFor();
  await page.getByRole("button",{name:"编辑连接器验收",exact:true}).click();
  await page.getByRole("textbox",{name:"对话名称"}).fill("连接器中修改的对话");
  await page.getByRole("button",{name:"保存名称",exact:true}).click();
  assert.equal((await api("GET",`/api/conversations/${conversation.id}`)).title,"连接器中修改的对话");
  await page.getByRole("dialog").last().locator('button[aria-label^="关闭"]').click();
 await page.getByRole("button",{name:"编辑权限与范围",exact:true}).click();
 const permission=page.getByRole("combobox",{name:"验收助手 读取文件权限",exact:true});const savedPermission=page.waitForResponse(response=>response.url().endsWith("/api/security/access-rules")&&response.request().method()==="PUT");await permission.selectOption("approve");await savedPermission;
 const rules=(await api("GET","/api/security/access-rules")).rules;assert.ok(rules.some(rule=>rule.scope==="agent"&&rule.subject_id===agent.id&&rule.permission_id==="filesystem.read"&&rule.mode==="approve"));assert.ok(!rules.some(rule=>rule.scope==="default"));
 await page.waitForTimeout(250);await page.screenshot({path:join(evidenceDir,"agent-permissions.png")});
 await page.getByRole("dialog").last().locator('button[aria-label^="关闭"]').click();
 await page.getByRole("button",{name:"编辑桌面工作区资料",exact:true}).click();await page.getByRole("textbox",{name:"知识内容",exact:true}).waitFor();
 await page.getByRole("dialog").last().locator('button[aria-label^="关闭"]').click();
 await page.waitForTimeout(250);await page.screenshot({path:join(evidenceDir,"connectors.png")});
 const view=await api("GET",`/api/connectors/${a.canvas_id}`);assert.ok(view.nodes.some(n=>n.entity_id===conversation.id));
 const hiddenMemory=`memory:${memory.id}`;const hideSaved=page.waitForResponse(response=>response.url().endsWith(`/api/connectors/${a.canvas_id}/display`)&&response.request().method()==="PUT");
 await page.locator(`.react-flow__node[data-id="${hiddenMemory}"]`).focus();await page.keyboard.press("Enter");await page.keyboard.press("Delete");await hideSaved;
 const hiddenView=await api("GET",`/api/connectors/${a.canvas_id}`);assert.ok(hiddenView.display.hidden_node_ids.includes(hiddenMemory));assert.ok(hiddenView.bindings.some(binding=>binding.target_id===hiddenMemory),"hiding node keeps runtime binding");
 await api("PUT",`/api/connectors/${a.canvas_id}/display`,{...hiddenView.display,hidden_node_ids:[]});await page.goto(`${origin}/connectors`);
 const memoryBinding=hiddenView.bindings.find(binding=>binding.target_id===hiddenMemory);const edgeDeleted=page.waitForResponse(response=>response.url().includes(`/api/connectors/${a.canvas_id}/bindings/`)&&response.request().method()==="DELETE");
 await page.locator(`.react-flow__edge[data-id="${memoryBinding.id}"]`).focus();await page.keyboard.press("Enter");await page.keyboard.press("Delete");await edgeDeleted;
 assert.ok(!(await api("GET",`/api/connectors/${a.canvas_id}`)).bindings.some(binding=>binding.target_id===hiddenMemory),"edge-only deletion unbinds");assert.equal((await api("GET",`/api/memories/${memory.id}`)).id,memory.id);
 await api("POST",`/api/connectors/${a.canvas_id}/bindings`,{kind:"agent_memory",source_id:`agent:${agent.id}`,target_id:hiddenMemory});await page.goto(`${origin}/connectors`);
 await page.getByRole("button",{name:"显示与布局",exact:true}).waitFor();
  await page.getByRole("button",{name:"显示与布局",exact:true}).click();
  await page.getByRole("combobox",{name:"显示模式"}).selectOption("conversations");
  await page.getByRole("button",{name:"保存显示配置",exact:true}).click();
 assert.equal((await api("GET",`/api/connectors/${a.canvas_id}`)).display.mode,"conversations");
  await page.getByRole("button",{name:"管理全部画布",exact:true}).click();
  await page.locator("article").filter({hasText:"项目演示"}).getByRole("button",{name:"删除此画布"}).click();
  await page.getByRole("button",{name:"确认删除",exact:true}).click();
  await page.locator('[role="dialog"] article').filter({hasText:"项目演示"}).waitFor({state:"detached"});
  let canvases=(await api("GET","/api/connectors")).canvases;assert.ok(canvases.some(x=>x.id===a.canvas_id)&&!canvases.some(x=>x.id===b.canvas_id)&&canvases.some(x=>x.id===c.canvas_id));
  await page.getByRole("dialog").last().locator('button[aria-label^="关闭"]').click();
 await api("DELETE",`/api/connectors/${c.canvas_id}`);
 await page.goto(`${origin}/settings`);await page.getByRole("button",{name:/对话模型与模型池/}).click();await page.getByRole("dialog").first().waitFor();
 await page.waitForTimeout(250);await page.screenshot({path:join(evidenceDir,"model-pools-empty.png")});
 const observed=[];const provider=createServer(async(req,res)=>{let raw="";for await(const part of req)raw+=part;const body=JSON.parse(raw);observed.push(body.model);if(body.model==="unavailable-primary"){res.writeHead(503,{"Content-Type":"application/json"});res.end(JSON.stringify({error:{code:"service_unavailable",message:"controlled primary unavailable"}}));return;}if(body.stream){res.writeHead(200,{"Content-Type":"text/event-stream"});res.end('data: '+JSON.stringify({id:"fixture",choices:[{index:0,delta:{content:"模型池二级回复已完成。"},finish_reason:null}]})+'\n\ndata: '+JSON.stringify({id:"fixture",choices:[{index:0,delta:{},finish_reason:"stop"}],usage:{prompt_tokens:12,completion_tokens:5,total_tokens:17}})+'\n\ndata: [DONE]\n\n');}else{res.writeHead(200,{"Content-Type":"application/json"});res.end(JSON.stringify({id:"fixture",choices:[{index:0,message:{role:"assistant",content:"模型池二级回复已完成。"},finish_reason:"stop"}],usage:{prompt_tokens:12,completion_tokens:5,total_tokens:17}}));}});
 await new Promise(resolve=>provider.listen(0,"127.0.0.1",resolve));const port=provider.address().port;const modelIds=[];let poolId;
 try{
  for(const model of ["unavailable-primary","working-secondary"]){const row=await api("POST","/api/llm/models",{provider:"local",label:model,model,base_url:`http://127.0.0.1:${port}/v1`,api_format:"openai",api_key:"controlled-fixture-credential",temperature:0,max_tokens:128,invoke_timeout_ms:5000});modelIds.push(row.id);}
  const pool=await api("POST","/api/model-pools",{label:"验收模型池",service_kind:"chat",enabled:true,attempt_timeout_ms:5000,total_timeout_ms:15000,tiers:modelIds.map((id,i)=>({id:`level-${i}`,label:`第${i+1}级`,profile_ids:[id]}))});poolId=pool.id;
  await api("PUT",`/api/model-pool-bindings/${agent.id}`,{pool_id:poolId});
  const currentDisplay=(await api("GET",`/api/connectors/${a.canvas_id}`)).display;
  await api("PUT",`/api/connectors/${a.canvas_id}/display`,{...currentDisplay,mode:"all"});
  await page.goto(`${origin}/connectors`);await page.getByRole("button",{name:"编辑验收模型池",exact:true}).waitFor();
  const boxes=await page.locator(".react-flow__node").evaluateAll(nodes=>nodes.map(node=>{const b=node.getBoundingClientRect();return {x:b.x,y:b.y,right:b.right,bottom:b.bottom}}));
  for(let i=0;i<boxes.length;i++)for(let j=i+1;j<boxes.length;j++){const a=boxes[i],b=boxes[j];assert.ok(a.right<=b.x||b.right<=a.x||a.bottom<=b.y||b.bottom<=a.y,"newly bound node must not overlap saved nodes");}
  await page.waitForTimeout(250);await page.screenshot({path:join(evidenceDir,"connectors-with-model-pool.png")});
  await page.getByRole("button",{name:"编辑验收模型池",exact:true}).click();await page.getByRole("combobox",{name:"绑定模型",exact:true}).waitFor();await page.waitForTimeout(250);await page.screenshot({path:join(evidenceDir,"model-pool-editor.png")});
  await page.getByRole("dialog").last().locator('button[aria-label^="关闭"]').click();
  await page.goto(`${origin}/chat/${conversation.id}`);
  assert.equal((await page.getByRole("button",{name:"与小涟语音对话",exact:true}).innerText()).trim(),"");
  await page.getByPlaceholder("告诉小昔涟你想完成什么……").fill("请回复一句简短的话，不调用工具。");
  const chatResponse=page.waitForResponse(response=>response.url()===base+"/api/chat"&&response.request().method()==="POST");
  await page.getByRole("button",{name:"发送消息",exact:true}).click();
  const response=await chatResponse;assert.ok(response.ok());const stream=await response.text();
  await page.getByText("模型池二级回复已完成。",{exact:true}).first().waitFor();
  await page.waitForTimeout(400);await page.waitForTimeout(250);await page.screenshot({path:join(evidenceDir,"model-pool-chat.png")});
  assert.ok(stream.includes('"phase":"switched"'),"actual model_route switched event");assert.ok((await api("GET",`/api/conversations/${conversation.id}`)).messages.some(message=>message.role==="assistant"&&message.content.includes("模型池二级回复已完成")),"persisted second provider answer");assert.ok(observed.indexOf("unavailable-primary")>=0&&observed.indexOf("working-secondary")>observed.indexOf("unavailable-primary"));
  await writeFile(join(evidenceDir,"model-pool-events.txt"),stream);
  const usage=await api("GET","/api/llm/usage");assert.ok(usage.total_tokens>=17);
  await page.goto(`${origin}/system`);await page.getByText("Token 用量",{exact:true}).waitFor();await page.waitForTimeout(250);await page.screenshot({path:join(evidenceDir,"monitor.png")});
 }finally{
  if(poolId){await api("PUT",`/api/model-pool-bindings/${agent.id}`,{pool_id:null});await api("DELETE",`/api/model-pools/${poolId}?expected_revision=1`);}
  for(const id of modelIds)await api("DELETE",`/api/llm/models/${id}`);
  await new Promise(resolve=>provider.close(resolve));
 }
 await api("DELETE",`/api/connectors/${a.canvas_id}`);assert.equal((await api("GET","/api/connectors")).canvases.length,0);
 await page.goto(`${origin}/connectors`);await page.getByText("还没有连接器画布",{exact:true}).waitFor();await page.waitForTimeout(250);await page.screenshot({path:join(evidenceDir,"last-canvas-deleted.png")});
 return {real_backend:true,browser_ui:true,controlled_provider:true,checks:{row_major_navigation:true,capability_dialog:true,connector_persistence:true,node_hide_preserves_binding:true,edge_delete_unbinds:true,new_node_placement:true,icon_voice_controls:true,targeted_delete:true,last_canvas_empty:true,agent_scoped_permissions:true,knowledge_node_editor:true,model_pool_downgrade:true,usage:true},evidence:evidenceDir};
}
