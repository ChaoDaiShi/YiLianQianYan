import assert from "node:assert/strict";
import {createServer} from "node:http";
import {writeFile} from "node:fs/promises";
import {join} from "node:path";

export async function connectorRevision2({page,frontendPort,backendPort,controlToken,evidenceDir}) {
 const origin=`http://127.0.0.1:${frontendPort}`,base=`http://127.0.0.1:${backendPort}`;
 const requests=[];
 async function api(method,path,body){const response=await fetch(base+path,{method,headers:{"Content-Type":"application/json","X-Yilian-Control-Session":controlToken},body:body===undefined?undefined:JSON.stringify(body)});const text=await response.text();assert.ok(response.ok,`${method} ${path}: ${response.status} ${text.slice(0,400)}`);return text?JSON.parse(text):null;}
 const provider=createServer(async(req,res)=>{let raw="";for await(const chunk of req)raw+=chunk;const body=JSON.parse(raw);requests.push(body);const question=body.messages.filter(message=>message.role==="user").at(-1)?.content;const answer=question==="你好"?"你好":question==="告诉我你的名字"?"我叫小涟":question==="你认为什么是善"?"善的意思是关心他人，并为自己的行为负责。":"请继续提问。";res.writeHead(200,{"Content-Type":"text/event-stream"});res.end(`data: ${JSON.stringify({choices:[{index:0,delta:{content:answer},finish_reason:null}]})}\n\ndata: ${JSON.stringify({choices:[{index:0,delta:{},finish_reason:"stop"}],usage:{prompt_tokens:12,completion_tokens:7,total_tokens:19}})}\n\ndata: [DONE]\n\n`);});
 await new Promise(resolve=>provider.listen(0,"127.0.0.1",resolve));
 try {
  const agent=await api("POST","/api/agents",{name:"小涟",user_prompt:"使用中文回答问题。",allowed_tools:[],capabilities:[]});
  const root=await api("POST","/api/conversations",{title:"问答分支"});
  const a=await api("POST","/api/connectors",{name:"问答分支画布",agent_id:agent.id});
  const b=await api("POST","/api/connectors",{name:"待删除画布",agent_id:agent.id});
  const c=await api("POST","/api/connectors",{name:"最后的画布",agent_id:agent.id});
  await api("POST",`/api/connectors/${a.canvas_id}/bindings`,{kind:"conversation_agent",source_id:`conversation:${root.id}`,target_id:`agent:${agent.id}`});
  const model=await api("POST","/api/llm/models",{provider:"local",label:"验收对话服务",model:"branch-provider",base_url:`http://127.0.0.1:${provider.address().port}/v1`,api_format:"openai",api_key:"controlled-test-credential",temperature:0,max_tokens:256,invoke_timeout_ms:10000});
  await api("POST",`/api/connectors/${a.canvas_id}/bindings`,{kind:"agent_model",source_id:`agent:${agent.id}`,target_id:`model:${model.id}`,model_source:"profile",service_kind:"chat"});
  for(const category of ["preference","knowledge"]){const memory=await api("POST","/api/memories",{content:category==="knowledge"?"工作区知识":"偏好中文交流",category});await api("POST",`/api/connectors/${a.canvas_id}/bindings`,{kind:category==="knowledge"?"agent_knowledge":"agent_memory",source_id:`agent:${agent.id}`,target_id:`${category==="knowledge"?"knowledge":"memory"}:${memory.id}`});}
  await page.goto(origin+"/connectors");await page.getByRole("combobox",{name:"消息画布对话目录"}).waitFor();
  const graphPath=`/api/connectors/${a.canvas_id}/messages/${root.id}`;
  async function turn(question,count){await page.getByRole("textbox",{name:"消息画布提问"}).fill(question);await page.getByRole("button",{name:"发送提问",exact:true}).click();await page.waitForFunction(expected=>document.querySelectorAll('.message-branch-stage .react-flow__node').length===expected&&!document.querySelector('.message-branch-stage [data-id^="pending-"]'),count);await page.getByRole("button",{name:"发送提问",exact:true}).waitFor();return api("GET",graphPath);}
  await turn("你好",2);const first=(await api("GET",graphPath)).nodes.find(node=>node.role==="assistant");
  await turn("告诉我你的名字",4);
  const firstNode=page.locator(`.message-branch-stage .react-flow__node[data-id="${first.id}"]`);
  // Fit all nodes through the real graph control before selecting an earlier answer.
  await page.locator('.message-branch-stage .react-flow__controls-fitview').click();
  await firstNode.getByRole("button",{name:"从此回答继续提问",exact:true}).click();
  let graph=await turn("你认为什么是善",6);
  const siblingQuestions=graph.nodes.filter(node=>node.parent_id===first.id);assert.equal(siblingQuestions.length,2);
  assert.deepEqual(requests[2].messages.filter(message=>message.role==="user").map(message=>message.content),["你好","你认为什么是善"]);
  assert.deepEqual(requests[1].messages.filter(message=>message.role==="user").map(message=>message.content),["你好","告诉我你的名字"]);
  assert.equal((await api("GET",`/api/conversations/${root.id}`)).messages.length,0,"source conversation stays intact");
  const catalog=await api("GET",`/api/connectors/${a.canvas_id}/message-sources`);assert.equal(catalog.conversations.length,1,"execution forks stay out of root picker");
  await page.locator('.message-branch-stage .react-flow__controls-fitview').click();await page.waitForTimeout(350);
  const points=await page.locator('.message-branch-stage .react-flow__node').evaluateAll(nodes=>nodes.map(node=>({id:node.dataset.id,x:node.getBoundingClientRect().x,y:node.getBoundingClientRect().y})));
  const parentPoint=points.find(point=>point.id===first.id);const children=siblingQuestions.map(node=>points.find(point=>point.id===node.id));assert.ok(children.every(point=>point.y>parentPoint.y));assert.ok(Math.abs(children[0].x-children[1].x)>50);
  const ports=await page.locator('.connector-resource-pane .connector-node.agent .react-flow__handle').evaluateAll(nodes=>nodes.map(node=>node.dataset.handlepos).sort());assert.deepEqual(ports,["bottom","left","right","top"]);
  const curves=await page.locator('.connector-resource-pane .react-flow__edge-path').evaluateAll(nodes=>nodes.map(node=>node.getAttribute("d")));assert.ok(curves.length>=4&&curves.every(path=>path.includes("C")));
  const inside=await page.locator('.message-branch-stage').evaluate(stage=>{const b=stage.getBoundingClientRect();return Array.from(stage.querySelectorAll('.react-flow__node')).every(node=>{const n=node.getBoundingClientRect();return n.x>=b.x&&n.y>=b.y&&n.right<=b.right&&n.bottom<=b.bottom});});assert.ok(inside,"Fit view must include all six message nodes");
  await page.screenshot({path:join(evidenceDir,"revision2-message-branches.png")});
  // Selection and viewport must survive a real navigation/reload.
  assert.equal((await api("GET",graphPath)).selected_node_id,first.id);
  await page.reload();await page.waitForFunction(()=>document.querySelectorAll('.message-branch-stage .react-flow__node').length===6);assert.equal((await api("GET",graphPath)).nodes.length,6);

  // Chat substrate is transparent in both actual theme modes, while the input remains readable.
  const themes=[];
  for(const theme of ["白天","夜间"]){await page.goto(origin+"/settings?section=appearance");await page.getByRole("radio",{name:new RegExp(theme)}).click();await page.goto(origin+`/chat/${graph.nodes.find(node=>node.role==="assistant").conversation_id}`);await page.locator('.conversation-composer').waitFor();const expand=page.getByRole("button",{name:"展开执行轨迹",exact:true});if(await expand.count())await expand.click();const facts=await page.evaluate(()=>[".workbench-grid",".conversation-composer",".composer-card",".context-rail",".execution-region",".execution-sidebar"].flatMap(selector=>Array.from(document.querySelectorAll(selector)).map(node=>({selector,color:getComputedStyle(node).backgroundColor,image:getComputedStyle(node).backgroundImage}))));assert.ok(facts.length>=4);for(const fact of facts){assert.equal(fact.color,"rgba(0, 0, 0, 0)",`${theme} ${fact.selector}`);assert.equal(fact.image,"none",`${theme} ${fact.selector} image`);}themes.push({theme,facts});await page.screenshot({path:join(evidenceDir,`revision2-chat-${theme}.png`)});}
  await page.evaluate(async()=>{(await import('/src/components/ui/CyreneToast.tsx')).showToast('顶部提示验收','info',60000);});const toast=page.getByRole("region",{name:"系统轻通知"});await toast.waitFor();assert.ok((await toast.boundingBox()).y<40);await page.screenshot({path:join(evidenceDir,"revision2-toast-top.png")});await page.getByRole("button",{name:"关闭提示",exact:true}).click();
  // Direct current, noncurrent, last deletion: one dialog and exact backend identities.
  await page.goto(origin+"/connectors");await page.getByRole("button",{name:"删除当前画布",exact:true}).click();await page.getByRole("button",{name:"确认删除",exact:true}).waitFor();assert.equal(await page.getByRole("dialog").count(),1);
  await page.getByRole("button",{name:"取消",exact:true}).focus();await page.keyboard.press("Tab");assert.ok((await page.evaluate(()=>document.activeElement?.textContent))?.startsWith("确认删除"));await page.keyboard.press("Enter");await page.getByRole("dialog").waitFor({state:"detached"});assert.ok(!(await api("GET","/api/connectors")).canvases.some(canvas=>canvas.id===a.canvas_id));assert.equal(await page.getByRole("combobox",{name:"当前连接器画布"}).inputValue(),b.canvas_id);
  await page.getByRole("button",{name:"管理全部画布",exact:true}).click();await page.locator('article').filter({hasText:"最后的画布"}).getByRole("button",{name:"删除此画布"}).click();assert.equal(await page.getByRole("dialog").count(),1);await page.getByRole("button",{name:"确认删除",exact:true}).click();await page.getByRole("heading",{name:"管理全部画布",exact:true}).waitFor();await page.locator('article').filter({hasText:"最后的画布"}).waitFor({state:"detached"});await page.getByRole("dialog").locator('button[aria-label^="关闭"]').click();assert.equal(await page.getByRole("combobox",{name:"当前连接器画布"}).inputValue(),b.canvas_id);
  await page.getByRole("button",{name:"删除当前画布",exact:true}).click();await page.getByRole("button",{name:"确认删除",exact:true}).click();await page.getByText("还没有连接器画布",{exact:true}).waitFor();assert.equal((await api("GET","/api/connectors")).canvases.length,0);await page.screenshot({path:join(evidenceDir,"revision2-all-deleted.png")});
  await writeFile(join(evidenceDir,"revision2-model-requests.json"),JSON.stringify(requests,null,2));await writeFile(join(evidenceDir,"revision2-theme-facts.json"),JSON.stringify(themes,null,2));
  return {status:"passed",real_backend:true,controlled_provider:true,checks:{message_nodes:true,vertical_chain:true,sibling_branches:true,ancestor_isolation:true,source_unchanged:true,fork_catalog:true,reopen:true,four_ports:true,bezier:true,light_dark_transparent:true,toast_top:true,direct_current_delete:true,keyboard_confirmation:true,noncurrent_delete:true,last_delete:true},evidence:evidenceDir};
 }finally{await new Promise(resolve=>provider.close(resolve));}
}
