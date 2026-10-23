import assert from "node:assert/strict";
import { join } from "node:path";
import { mkdir, writeFile } from "node:fs/promises";

/** Final integrated browser/API acceptance, isolated by core-paths.mjs. */
export async function v1R3({page,frontendPort,backendPort,controlToken,evidenceDir,workspace}) {
  const origin="http://127.0.0.1:"+frontendPort;
  const api=async(method,path,body)=>{
    const response=await fetch("http://127.0.0.1:"+backendPort+path,{
      method,headers:{"Content-Type":"application/json","X-Yilian-Control-Session":controlToken},
      body:body===undefined?undefined:JSON.stringify(body),
    });
    const value=await response.json();
    assert.ok(response.ok,method+" "+path+" "+JSON.stringify(value));
    return value;
  };
  const checks=[];
  const capture=async name=>page.screenshot({path:join(evidenceDir,name+".png"),fullPage:false,animations:"disabled"});
  await page.goto(origin+"/knowledge");
  await page.getByRole("button",{name:"AI 构建知识",exact:true}).click();
  const buildDialog=page.getByRole("dialog");
  await buildDialog.getByRole("textbox").first().fill("构建数据结构知识体系");
  assert.ok((await buildDialog.boundingBox()).height < 600,"Empty build dialog must size to its content");
  await buildDialog.getByRole("button",{name:"深度",exact:true}).click();
  assert.equal(await buildDialog.getByRole("button",{name:"深度",exact:true}).getAttribute("aria-pressed"),"true");
  assert.equal(await buildDialog.getByLabel("构建记录",{exact:true}).count(),0);
  const inputBox=await buildDialog.getByRole("textbox").first().boundingBox();
  const startBox=await buildDialog.getByRole("button",{name:"开始构建",exact:true}).boundingBox();
  assert.ok(Math.abs((inputBox.x+inputBox.width/2)-(startBox.x+startBox.width/2))<4);
  await capture("r3-knowledge-deep");
  await buildDialog.getByRole("button",{name:"快速",exact:true}).click();
  assert.equal(await buildDialog.getByRole("button",{name:"快速",exact:true}).getAttribute("aria-pressed"),"true");
  await buildDialog.getByRole("button",{name:"构建历史",exact:true}).click();
  await page.getByRole("dialog",{name:"构建历史",exact:true}).waitFor();
  await capture("r3-knowledge-history-dialog");
  await page.getByRole("button",{name:"关闭构建历史",exact:true}).click();
  checks.push("knowledge_modes_preserve_original_page");
  await buildDialog.getByRole("button",{name:"关闭AI 构建知识",exact:true}).click();
  const folder=join(workspace,"R3资料分类");
  await mkdir(join(folder,"子目录"),{recursive:true});
  await writeFile(join(folder,"概念.md"),"# 数据结构\nR3_FOLDER_ROOT");
  await writeFile(join(folder,"子目录","树.md"),"# 树\nR3_FOLDER_TREE");
  await page.getByLabel("选择知识文件夹",{exact:true}).setInputFiles(folder);
  const importedDialog=page.getByRole("dialog",{name:"导入结果",exact:true});
  await importedDialog.waitFor();
  let imported;
  for(let i=0;i<100;i++){
    imported=await api("GET","/api/knowledge/documents");
    if(imported.collections.some(collection=>collection.name==="R3资料分类"&&collection.document_count===2))break;
    await new Promise(resolve=>setTimeout(resolve,100));
  }
  const collection=imported.collections.find(collection=>collection.name==="R3资料分类");
  assert.equal(collection.document_count,2);
  assert.ok(imported.memberships.some(member=>member.relative_path==="R3资料分类/子目录/树.md"));
  await capture("r3-folder-import-dialog");
  await importedDialog.getByRole("button",{name:"关闭导入结果",exact:true}).click();
  await page.getByRole("combobox",{name:"知识分类",exact:true}).selectOption(collection.id);
  await page.locator(".knowledge-document-open").filter({hasText:"树.md"}).waitFor();
  await page.reload();
  await page.getByRole("combobox",{name:"知识分类",exact:true}).waitFor();
  assert.ok((await api("GET","/api/knowledge/documents?collection_id="+collection.id)).documents.length===2);
  await capture("r3-folder-collection");
  checks.push("folder_import_dialog_and_persistent_collection_paths");
  await page.setViewportSize({width:1280,height:720});
  await page.goto(origin+"/account");
  await page.locator(".cloud-section-nav").waitFor();
  await page.getByRole("button",{name:"登录并连接",exact:true}).waitFor();
  const navBefore=await page.locator(".cloud-section-nav").boundingBox();
  await page.locator(".cloud-content").evaluate(element=>{element.scrollTop=element.scrollHeight;});
  const navAfter=await page.locator(".cloud-section-nav").boundingBox();
  assert.ok(Math.abs(navBefore.y-navAfter.y)<1);
  assert.equal(await page.evaluate(()=>document.documentElement.scrollHeight>innerHeight+1),false);
  await capture("r3-account-independent-scroll");
  checks.push("account_content_scroll_keeps_navigation_fixed");
  await page.setViewportSize({width:1440,height:900});
  for(const [section,title,heading,action] of [["vision","图像识别","图像识别服务","识别图片"],["image","图像生成","图像生成服务","生成图片"]]){
    await page.goto(origin+"/settings?section="+section);
    const service=page.getByRole("dialog",{name:title,exact:true});
    await service.getByRole("heading",{name:heading,exact:true}).waitFor();
    await service.getByRole("button",{name:action,exact:true}).waitFor();
    assert.equal(await service.getByRole("button",{name:"新建模型池",exact:true}).count(),0);
    await capture("r3-service-"+section);
  }
  await page.goto(origin+"/settings");
  for(const name of ["编辑模型与服务","编辑语音"]){
    const group=page.getByRole("button",{name,exact:true});
    await group.waitFor();
    if(await group.getAttribute("aria-expanded")!=="true")await group.click();
  }
  await page.locator('.react-flow__node[data-id="setting:tts"]').waitFor();
  const boxes=await page.locator('.settings-canvas-stage .react-flow__node[data-id^="setting:"]').evaluateAll(nodes=>nodes.map(node=>{const rect=node.getBoundingClientRect();return{x:rect.x,y:rect.y,right:rect.right,bottom:rect.bottom,id:node.dataset.id};}));
  for(let i=0;i<boxes.length;i++)for(let j=i+1;j<boxes.length;j++)assert.ok(boxes[i].right<=boxes[j].x||boxes[j].right<=boxes[i].x||boxes[i].bottom<=boxes[j].y||boxes[j].bottom<=boxes[i].y,boxes[i].id+" overlaps "+boxes[j].id);
  await capture("r3-settings-expanded-no-overlap");
  checks.push("image_services_open_real_forms_and_settings_groups_do_not_overlap");
  for(const [category,label] of [["skill","Skill"],["mcp","MCP"],["tool","工具"],["plugin","插件"],["import","导入与本地发现"]]){
    await page.goto(origin+"/capabilities?category="+category);
    const navigation=page.getByRole("navigation",{name:"能力分类",exact:true});
    await navigation.getByRole("button",{name:label,exact:true}).waitFor();
    assert.equal(await navigation.getByRole("button",{name:label,exact:true}).getAttribute("aria-current"),"page");
    assert.equal(await page.getByRole("dialog").count(),0);
    assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth>innerWidth),false);
    await capture("r3-capability-"+category);
  }
  checks.push("capability_categories_render_persistent_navigation_without_outer_modal");
  await page.goto(origin+"/capabilities?category=import&import_tab=structured");
  await page.getByRole("textbox",{name:"JSON 内容",exact:true}).fill(JSON.stringify({name:"R8导入工作流",definition:{schema_version:1,entry_node_id:"end",nodes:[{id:"end",kind:"output",config:{type:"output",template:"R8_IMPORTED"}}],edges:[]}}));
  await page.getByRole("button",{name:"确认导入配置",exact:true}).click();
  await page.getByRole("link",{name:"打开已导入工作流",exact:true}).click();
  await page.getByRole("textbox",{name:"工作流名称",exact:true}).waitFor();
  assert.equal(await page.getByRole("textbox",{name:"工作流名称",exact:true}).inputValue(),"R8导入工作流");
  assert.equal(await page.getByRole("dialog").count(),0);
  checks.push("structured_import_opens_actual_saved_workflow");

  const memory=await api("POST","/api/memories",{content:"R3验收偏好中文资料",category:"preference",source:"manual"});
  const first=await api("GET","/api/memories/"+memory.id+"/governance");
  await api("POST","/api/memories/"+memory.id+"/correct",{content:"R3验收偏好中英文原始资料",expected_revision:first.revision,reason:"用户明确纠正"});
  const corrected=await api("GET","/api/memories/"+memory.id+"/governance");
  assert.equal(corrected.revision,first.revision+1);
  const history=await api("GET","/api/memories/"+memory.id+"/history");
  assert.equal(history.length,2);
  await api("POST","/api/memories/"+memory.id+"/withdraw",{expected_revision:corrected.revision,reason:"结束该偏好"});
  const removed=await api("GET","/api/memories/"+memory.id+"/governance");
  assert.equal(removed.status,"withdrawn");
  await page.goto(origin+"/memory");
  await page.getByRole("button",{name:"已撤回",exact:true}).click();
  await page.getByText("R3验收偏好中英文原始资料",{exact:true}).first().waitFor();
  await capture("r3-memory-history");
  await page.getByText("历史版本（3）",{exact:true}).click();
  await page.getByText("用户明确纠正",{exact:false}).waitFor();
  await capture("r3-memory-versions");
  checks.push("memory_revision_correction_withdrawal_persisted");

  const graph=await api("POST","/api/workflow-graphs",{
    name:"R3原工作流参数验收",description:"本地隔离验收",
    definition:{schema_version:1,entry_node_id:"result",nodes:[{id:"result",kind:"output",config:{type:"output",template:"ORIGINAL"}}],edges:[]},
  });
  const run=await api("POST","/api/workflow-graphs/"+graph.id+"/run",{inputs:{result:{type:"output",template:"R3_RUNTIME_ONLY_OUTPUT"}}});
  let observed;
  for(let i=0;i<100;i++){
    observed=await api("GET","/api/workflow-runs/"+run.run_id);
    if(observed.status==="completed"||observed.status==="failed")break;
    await new Promise(resolve=>setTimeout(resolve,100));
  }
  assert.equal(observed.status,"completed");
  assert.ok(JSON.stringify(observed).includes("R3_RUNTIME_ONLY_OUTPUT"));
  assert.equal((await api("GET","/api/workflow-graphs/"+graph.id)).definition.nodes[0].config.template,"ORIGINAL");
  checks.push("workflow_inputs_apply_to_run_without_mutating_graph");
  await page.goto(origin+"/capabilities?category=workflow");
  assert.equal(await page.getByRole("dialog").count(),0);
  await page.getByText("R3原工作流参数验收",{exact:true}).first().waitFor();
  await capture("r3-original-workflow");
  checks.push("original_workflow_page_and_history_reachable");
  await page.goto(origin+"/capabilities?category=workflow&workflow="+graph.id);
  await page.getByRole("textbox",{name:"工作流名称",exact:true}).waitFor();
  assert.equal(await page.getByRole("textbox",{name:"工作流名称",exact:true}).inputValue(),graph.name);
  await page.getByRole("button",{name:"选择节点 result",exact:true}).click();
  await capture("r3-workflow-editor");
  await page.getByRole("button",{name:"运行",exact:true}).click();
  const runDialog=page.getByRole("dialog",{name:"运行 · "+graph.name,exact:true});
  await runDialog.getByRole("textbox",{name:"result 运行参数",exact:true}).fill("R3_UI_RUN_OUTPUT");
  await capture("r3-workflow-run-inputs");
  const runResponse=page.waitForResponse(response=>response.url().endsWith("/api/workflow-graphs/"+graph.id+"/run")&&response.request().method()==="POST");
  await runDialog.getByRole("button",{name:"开始运行",exact:true}).click();
  const uiRun=await (await runResponse).json();
  for(let i=0;i<100;i++){
    observed=await api("GET","/api/workflow-runs/"+uiRun.run_id);
    if(observed.status==="completed"||observed.status==="failed")break;
    await new Promise(resolve=>setTimeout(resolve,100));
  }
  assert.equal(observed.status,"completed");
  assert.ok(JSON.stringify(observed).includes("R3_UI_RUN_OUTPUT"));
  assert.equal((await api("GET","/api/workflow-graphs/"+graph.id)).definition.nodes[0].config.template,"ORIGINAL");
  checks.push("original_editor_run_dialog_starts_real_parameterized_run");
  await page.setViewportSize({width:800,height:600});
  await page.goto(origin+"/capabilities?category=workflow&workflow="+graph.id);
  await page.getByRole("textbox",{name:"工作流名称",exact:true}).waitFor();
  assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth>innerWidth),false);
  await capture("r3-workflow-narrow");
  await page.setViewportSize({width:1440,height:900});
  checks.push("original_workflow_narrow_layout");

  const project=await api("POST","/api/workspaces",{name:"R3验收空间",root_path:workspace});
  const conversation=await api("POST","/api/conversations",{title:"R3空间关联"});
  await api("PUT","/api/conversations/"+conversation.id+"/workspace",{workspace_id:project.id});
  assert.equal((await api("GET","/api/conversations/"+conversation.id+"/workspace")).workspace_id,project.id);
  checks.push("conversation_workspace_context_persisted");
  await page.goto(origin+"/automations");
  await page.getByRole("button",{name:/新建/}).first().waitFor();
  await capture("r3-automations");
  await page.goto(origin+"/channels");
  await page.getByText("尚未配置消息渠道",{exact:true}).waitFor();
  await capture("r3-channels");
  await page.getByText("支持范围与官方协议",{exact:true}).click();
  await page.getByText("微信 iLink · 尚未支持",{exact:true}).waitFor();
  checks.push("automation_and_channel_pages_render_real_empty_state");
  await page.setViewportSize({width:800,height:600});
  await page.goto(origin+"/knowledge");
  await page.getByRole("button",{name:"AI 构建知识",exact:true}).waitFor();
  assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth>innerWidth),false);
  await capture("r3-knowledge-narrow");
  await page.setViewportSize({width:1440,height:900});
  checks.push("knowledge_narrow_layout");
  const result={status:"passed",checks,provider_evidence:"Knowledge model/search protocol covered by Rust loopback integration; browser does not call paid services."};
  await writeFile(join(evidenceDir,"v1-r3.json"),JSON.stringify(result,null,2));
  return result;
}
