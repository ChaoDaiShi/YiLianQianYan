import assert from "node:assert/strict";
import { createServer } from "node:http";
import { join } from "node:path";

export async function runRichWorkspace({ page, frontendPort, backendPort, controlToken, evidenceDir, restartBackend }) {
  const origin = "http://127.0.0.1:" + frontendPort;
  const base = "http://127.0.0.1:" + backendPort;
  const headers = { "Content-Type": "application/json", "X-Yilian-Control-Session": controlToken };
  const checks = [];
  const screenshots = [];
  async function send(method, path, body) {
    return fetch(base + path, {method, headers, body: body === undefined ? undefined : JSON.stringify(body)});
  }
  async function api(method, path, body) {
    const response = await send(method, path, body);
    const raw = await response.text();
    assert.ok(response.ok, method + " " + path + ": " + response.status + " " + raw.slice(0, 300));
    return raw ? JSON.parse(raw) : undefined;
  }
  async function readUntil(path, validate) {
    const deadline = Date.now() + 12_000;
    while (Date.now() < deadline) {
      const found = await api("GET", path);
      if (validate(found)) return found;
      await new Promise(done => setTimeout(done, 120));
    }
    assert.fail("Expected changed state at " + path);
  }
  async function capture(name) {
    const screenshot = join(evidenceDir, name + ".png");
    await page.screenshot({path:screenshot,animations:"disabled"});
    screenshots.push(screenshot);
  }
  const { workspaces } = await api("GET", "/api/v1/workspaces");
  const workspace = workspaces.find(item => item.is_default);
  assert.ok(workspace);
  const rootPath = "/workspaces/" + workspace.id;
  await page.goto(origin + rootPath, {waitUntil:"domcontentloaded"});
  await page.getByRole("tab", {name:"页面",exact:true}).click();
  const create = async name => {
    const dialog = page.getByRole("dialog");
    await dialog.getByLabel("页面标题").fill(name);
    await dialog.getByRole("button", {name:"创建页面",exact:true}).click();
    await page.getByRole("heading", {name,exact:true}).waitFor();
    return new URL(page.url()).searchParams.get("page");
  };
  await page.getByRole("button", {name:"新建页面",exact:true}).click();
  const root = await create("验收根页面");
  await page.getByLabel("页面操作").selectOption("child");
  const child = await create("验收子页面");
  await page.getByLabel("页面操作").selectOption("child");
  const leaf = await create("验收三级页面");
  const leafApi = "/api/v1/pages/" + leaf;
  await page.getByLabel("内容块 1", {exact:true}).fill("## 原始标题\n\n- 编辑器自动保存");
  const original = await readUntil(leafApi, data => data.blocks[0]?.text === "## 原始标题\n\n- 编辑器自动保存");
  assert.equal(original.parent_page_id, child);
  const cycle = await send("PATCH", "/api/v1/pages/" + root, {revision:1,parent_page_id:leaf});
  assert.equal(cycle.status, 409);
  await page.reload({waitUntil:"domcontentloaded"});
  await page.getByRole("heading", {name:"验收三级页面",exact:true}).waitFor();
  assert.equal(await page.getByLabel("内容块 1", {exact:true}).inputValue(), original.blocks[0].text);
  await capture("pages-autosave");
  checks.push("nested_page_creation_autosave_reload_and_cycle_rejection");

  const task = await api("POST", "/api/tasks", {workspace_id:workspace.id,title:"真实任务状态"});
  const conversation = await api("POST", "/api/conversations", {title:"富交互验收",use_default_workspace:true});
  const bytes = Buffer.from("iVBORw0KGgoAAAANSUhEUgAAAAwAAAAMCAIAAADZF8uwAAAAFElEQVR4nGN4NXEnQcQwqmgwKgIASuk9UDnf664AAAAASUVORK5CYII=", "base64");
  const imageResult = await fetch(base + "/api/resources/ingest?name=rich-workspace.png", {
    method:"POST", headers:{...headers,"Content-Type":"image/png"}, body:bytes,
  });
  assert.ok(imageResult.ok, await imageResult.clone().text());
  const image = await imageResult.json();
  await api("POST", "/api/resource-bindings", {
    target:{kind:"conversation",conversation_id:conversation.id}, resource_ids:[image.id],
  });

  const blocks = [
    {type:"text",id:"text",text:"**组件内说明**"},
    {type:"image",id:"image",resource_id:image.id},
    {type:"table",id:"table",title:"精确数据",columns:["月份","数量"],rows:[["二月","12"],["一月","8"]]},
    {type:"chart",id:"chart",title:"销售趋势",kind:"line",labels:["一月","二月"],values:[8,12]},
    {type:"form",id:"form",title:"验收表单",fields:[{name:"note",label:"补充备注",required:true}]},
    {type:"task_status",id:"task",task_id:task.id},
    {type:"page_proposal",id:"proposal",page_id:leaf,revision:original.revision,changes:[{block_id:original.blocks[0].id,markdown:"## 应用 AI 建议后\n\n- 已修改"},{reference_page_id:root}]},
    {type:"page_create_proposal",id:"create",parent_page_id:leaf,title:"AI 建页验收",markdown:"# 可审查的新页面"},
  ];
  const agUi = {type:"CUSTOM",name:"yilian.rich_content",value:{schema_version:1,blocks:[{type:"text",id:"event",text:"AG-UI 消息扩展"}]}};
  const a2ui = [
    {version:"1.0",createSurface:{surfaceId:"report",dataModel:{title:"旧报表"}}},
    {version:"1.0",updateComponents:{surfaceId:"report",components:[{id:"root",component:"Text",text:{"@path":"/title"}}]}},
    {version:"1.0",updateDataModel:{surfaceId:"report",path:"/title",value:"A2UI 数据更新"}},
  ];
  const answer = "富内容验收正文\n\n```yilian-rich\n" + JSON.stringify({schema_version:1,blocks}) + "\n```\n"
    + "```ag-ui\n" + JSON.stringify(agUi) + "\n```\n"
    + "```a2ui\n" + JSON.stringify(a2ui) + "\n```";
  const provider = createServer(async (request,response) => {
    let raw = "";
    for await (const chunk of request) raw += chunk;
    if (request.method === "GET") {
      response.writeHead(200, {"content-type":"application/json"});
      response.end(JSON.stringify({data:[{id:"deepseek-chat"}]}));
      return;
    }
    const body = JSON.parse(raw);
    if (!body.stream) {
      response.writeHead(200, {"content-type":"application/json"});
      response.end(JSON.stringify({id:"rich-workspace-probe",choices:[{index:0,message:{role:"assistant",content:"pong"},finish_reason:"stop"}]}));
      return;
    }
    response.writeHead(200, {"content-type":"text/event-stream"});
    response.end("data: " + JSON.stringify({choices:[{index:0,delta:{content:answer},finish_reason:null}]}) + "\n\ndata: " +
      JSON.stringify({choices:[{index:0,delta:{},finish_reason:"stop"}]}) + "\n\ndata: [DONE]\n\n");
  });
  await new Promise(resolve => provider.listen(0,"127.0.0.1",resolve));
  try {
    const model = await api("POST","/api/llm/models",{
      provider:"deepseek", label:"富内容回环协议", model:"deepseek-chat",
      base_url:"http://127.0.0.1:" + provider.address().port + "/v1", api_format:"openai",
      api_key:"rich-workspace-test-key",temperature:0,max_tokens:4096,invoke_timeout_ms:15_000,
    });
    await api("POST","/api/llm/models/" + model.id + "/verify");
    await api("POST","/api/llm/models/" + model.id + "/activate");
    const chat = "/chat/" + conversation.id;
    await page.goto(origin + chat, {waitUntil:"domcontentloaded"});
    await page.getByRole("textbox", {name:"聊天输入",exact:true}).fill("你好，请在回答中列出查看用的图表和表单。");
    await page.getByRole("button", {name:"发送消息",exact:true}).click();
    await page.getByText("富内容验收正文", {exact:true}).waitFor();
    await page.getByText("组件内说明", {exact:true}).waitFor();
    await page.getByRole("img", {name:"rich-workspace.png",exact:true}).waitFor();
    await page.getByText("真实任务状态", {exact:true}).waitFor();
    await page.getByText("AG-UI 消息扩展", {exact:true}).waitFor();
    await page.getByText("A2UI 数据更新", {exact:true}).waitFor();
    checks.push("ag_ui_custom_event_and_a2ui_data_update_render_as_internal_blocks");
    await page.getByRole("table").getByRole("button", {name:"数量",exact:true}).click();
    await page.getByRole("combobox", {name:"查看数据点",exact:true}).selectOption("1");
    await page.getByRole("textbox", {name:"补充备注",exact:true}).fill("重启后仍在");
    await page.getByRole("button", {name:"提交表单",exact:true}).click();
    const conversationApi = "/api/conversations/" + conversation.id;
    const messages = await readUntil(conversationApi, result => result.messages.some(message => message.role === "assistant" && message.content.includes("yilian-rich")));
    const message = messages.messages.find(message => message.role === "assistant" && message.content.includes("yilian-rich"));
    const richApi = "/api/v1/conversations/" + conversation.id + "/messages/" + message.id + "/rich-content";
    await readUntil(richApi, result => result.states.form?.state.note === "重启后仍在" && result.states.chart?.state.selected === 1 && result.states.table?.state.column === 1);
    await page.reload({waitUntil:"domcontentloaded"});
    await page.getByRole("textbox", {name:"补充备注",exact:true}).waitFor();
    assert.equal(await page.getByRole("textbox", {name:"补充备注",exact:true}).inputValue(),"重启后仍在");
    assert.equal(await page.getByRole("combobox", {name:"查看数据点",exact:true}).inputValue(),"1");
    await page.getByRole("combobox", {name:"查看数据点",exact:true}).scrollIntoViewIfNeeded();
    const selectorIcon = await page.locator(".rich-chart select + svg").boundingBox();
    assert.ok(selectorIcon && selectorIcon.width <= 24, "Chart styles should preserve the size of the data selector icon");
    await capture("rich-chat-content");
    checks.push("real_backend_renders_six_rich_types_and_reloads_submitted_state");

    await page.getByRole("button", {name:"审查修改",exact:true}).click();
    const proposal = page.getByRole("dialog", {name:"审查页面修改",exact:true});
    await proposal.getByText("原始标题", {exact:false}).waitFor();
    await proposal.getByText("应用 AI 建议后", {exact:false}).waitFor();
    await proposal.getByText("验收根页面", {exact:true}).waitFor();
    assert.equal((await api("GET",leafApi)).revision,original.revision);
    await proposal.getByRole("button", {name:"应用修改",exact:true}).click();
    const edited = await readUntil(leafApi, data => data.blocks[0]?.text.includes("应用 AI 建议后"));
    assert.equal(edited.blocks[1]?.page_id, root);
    const restored = await api("POST", leafApi + "/revisions/" + original.revision + "/restore", {revision:edited.revision});
    assert.equal(restored.blocks[0].text, original.blocks[0].text);
    assert.equal(restored.blocks.length, 1);
    const editReplay = await api("POST", richApi + "/proposal/actions", {action:"apply_page_proposal",state_version:0});
    assert.equal(editReplay.page.revision, restored.revision);
    checks.push("ai_proposal_requires_review_and_rollback_restores_original_content");

    const childrenApi = "/api/v1/pages?workspace_id=" + workspace.id + "&parent_page_id=" + leaf;
    await page.getByRole("button", {name:"审查新建页面",exact:true}).click();
    const createProposal = page.getByRole("dialog", {name:"审查新建页面",exact:true});
    await createProposal.getByText("可审查的新页面", {exact:false}).waitFor();
    assert.equal((await api("GET",childrenApi)).pages.length, 0);
    await createProposal.getByRole("button", {name:"创建页面",exact:true}).click();
    const children = await readUntil(childrenApi, result => result.pages.length === 1);
    const proposedPage = children.pages[0];
    assert.equal(proposedPage.title, "AI 建页验收");
    assert.equal((await api("GET", "/api/v1/pages/" + proposedPage.id)).blocks[0].text, "# 可审查的新页面");
    const replay = await send("POST", richApi + "/create/actions", {action:"apply_page_proposal",state_version:0});
    assert.ok(replay.ok);
    assert.equal((await api("GET",childrenApi)).pages.length, 1);
    checks.push("ai_create_page_requires_review_and_replay_keeps_a_single_child");

    await page.getByRole("button", {name:"保存到页面",exact:true}).click();
    const save = page.getByRole("dialog", {name:"保存聊天成果",exact:true});
    await save.getByRole("textbox", {name:"页面标题",exact:true}).fill("富交互永久成果");
    await save.getByRole("button", {name:"保存成果",exact:true}).click();
    await save.getByRole("link", {name:"打开富交互永久成果",exact:true}).click();
    await page.getByRole("heading", {name:"富交互永久成果",exact:true}).waitFor();
    await page.getByText("重启后仍在", {exact:true}).waitFor();
    await page.getByRole("img", {name:"rich-workspace.png",exact:true}).waitFor();
    const savedPage = await api("GET", "/api/v1/pages/" + new URL(page.url()).searchParams.get("page"));
    assert.equal(savedPage.blocks[0].type,"rich");
    assert.equal(savedPage.blocks[0].snapshot.states.chart.state.selected,1);
    checks.push("chat_artifact_contains_committed_component_state_and_owned_image");
    const pageUrl = page.url();
    await page.goto("about:blank");
    await restartBackend();
    await page.goto(pageUrl,{waitUntil:"domcontentloaded"});
    await page.getByRole("heading",{name:"富交互永久成果",exact:true}).waitFor();
    await page.goto(origin + rootPath + "?tab=pages&page=" + leaf, {waitUntil:"domcontentloaded"});
    await page.getByRole("heading",{name:"验收三级页面",exact:true}).waitFor();
    await page.getByLabel("页面操作").selectOption("task");
    const taskDialog = page.getByRole("dialog", {name:"从页面创建任务"});
    await taskDialog.getByLabel("任务标题").fill("页面上下文验收任务");
    await taskDialog.getByRole("button", {name:"创建任务",exact:true}).click();
    const taskList = await readUntil("/api/tasks?workspace_id=" + workspace.id, data => data.tasks.some(item => item.title === "页面上下文验收任务"));
    const pageTask = taskList.tasks.find(item => item.title === "页面上下文验收任务");
    assert.deepEqual(pageTask.page_source, {page_id:leaf, revision:restored.revision});
    assert.equal(pageTask.status, "draft");
    await page.getByRole("link", {name:"来源页面 · 版本 " + restored.revision,exact:true}).waitFor();
    const foreignWorkspace = await api("POST", "/api/workspaces", {name:"页面任务隔离验收"});
    const forbidden = await send("POST", "/api/tasks", {workspace_id:foreignWorkspace.id,title:"不可跨区绑定",page_source:pageTask.page_source});
    assert.equal(forbidden.status, 403);
    checks.push("page_creates_draft_task_with_versioned_workspace_context");
    await page.goto(pageUrl,{waitUntil:"domcontentloaded"});
    await page.getByRole("heading",{name:"富交互永久成果",exact:true}).waitFor();
    await page.getByText("重启后仍在",{exact:true}).waitFor();
    await page.getByRole("link",{name:"聊天来源",exact:true}).click();
    await page.waitForURL(url => url.pathname === chat && url.searchParams.get("message") === message.id);
    await page.getByRole("textbox",{name:"补充备注",exact:true}).waitFor();
    assert.equal(await page.getByRole("textbox",{name:"补充备注",exact:true}).inputValue(),"重启后仍在");
    await page.getByRole("link", {name:"页面已创建，打开页面",exact:true}).waitFor();
    await page.getByRole("link", {name:`修改已应用于版本 ${edited.revision}，打开页面`,exact:true}).waitFor();
    assert.equal(await page.getByRole("button", {name:"审查新建页面",exact:true}).count(), 0);
    checks.push("backend_restart_preserves_page_source_link_and_original_component_state");
    await page.goto(pageUrl,{waitUntil:"domcontentloaded"});
    await page.getByRole("heading",{name:"富交互永久成果",exact:true}).waitFor();
    await page.setViewportSize({width:390,height:844});
    const geometry = await page.locator(".workspace-pages").evaluate(element => ({
      layout:element.clientWidth,content:element.scrollWidth,viewport:document.documentElement.clientWidth,
    }));
    assert.ok(geometry.content <= geometry.layout + 1, JSON.stringify(geometry));
    await capture("pages-mobile");
    checks.push("narrow_workspace_does_not_overflow");
    return {status:"passed",real_backend:true,controlled_provider:true,checks,screenshots};
  } finally {
    provider.closeAllConnections();
    await new Promise(resolve => provider.close(resolve));
  }
}
