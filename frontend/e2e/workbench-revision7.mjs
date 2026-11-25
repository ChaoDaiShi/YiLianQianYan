import assert from "node:assert/strict";
import { createServer } from "node:http";
import { mkdir, readFile, writeFile, access } from "node:fs/promises";
import { join, relative, resolve } from "node:path";
import { execFileSync } from "node:child_process";

const ROOT_QUESTION = "R7 根对话你好";
const ROOT_ANSWER = "R7 真实问候回答";
const WORK_QUESTION = "R7 请执行实际文件写入";
const WORK_ANSWER = "R7 文件已经实际写入。";
const TOOL_REASONING = "R7_TOOL_REASONING：读取用户目标后，调用真实文件工具保存内容，再根据工具结果回复。";
const LATER_QUESTION = "R7 原路径后续问题";
const BRANCH_QUESTION = "R7 从问候回答展开另一条路径";
const INDEPENDENT_QUESTION = "R7 独立问题";
const FILE_CONTENT = "R7_ACTUAL_FILE_CONTENT";
const SKILL_ARTIFACT = "R7_SKILL_PROCEDURE_EXECUTED";
const HOST_SKILL = "revision7-native-figma-host-only";
const KNOWLEDGE_TAIL = "R7_KNOWLEDGE_TAIL_MARKER";
const KNOWLEDGE_QUESTION = `请查询 ${KNOWLEDGE_TAIL}`;
const SKILL_QUESTION = "请执行 Codex 技能 revision7-codex-real，并按技能说明将结果保存到 skill-procedure-result.txt。";

/** Actual app/API/Git/filesystem flow with a local model protocol fixture. */
export async function runWorkbenchRevision7({ page, frontendPort, backendPort, controlToken, evidenceDir, workspace }) {
  const checkout = resolve(import.meta.dirname, "..", "..");
  const workspacePath = resolve(workspace);
  const relationship = relative(checkout, workspacePath);
  assert.ok(relationship.startsWith("..") || resolve(checkout).slice(0, 2) !== workspacePath.slice(0, 2), "Git fixture must be outside the checkout");
  let existingGit = false;
  try { await access(join(workspacePath, ".git")); existingGit = true; } catch (error) { if (error.code !== "ENOENT") throw error; }
  assert.equal(existingGit, false, "The harness must supply a new isolated workspace");

  const origin = `http://127.0.0.1:${frontendPort}`;
  const base = `http://127.0.0.1:${backendPort}`;
  const headers = { "Content-Type": "application/json", "X-Yilian-Control-Session": controlToken };
  const facts = { status: "RUNNING", real_backend: true, controlled_provider: true, checks: [], requests: [], screenshots: [] };
  async function api(method, path, body) {
    const response = await fetch(base + path, { method, headers, body: body === undefined ? undefined : JSON.stringify(body) });
    const raw = await response.text();
    assert.ok(response.ok, `${method} ${path}: ${response.status} ${raw.slice(0, 350)}`);
    return raw ? JSON.parse(raw) : null;
  }
  async function eventually(read, valid, label, timeout = 25_000) {
    const deadline = Date.now() + timeout;
    let value;
    do {
      value = await read();
      if (valid(value)) return value;
      await new Promise(resolveWait => setTimeout(resolveWait, 100));
    } while (Date.now() < deadline);
    assert.fail(`${label}: ${JSON.stringify(value)}`);
  }
  async function navigate(path) {
    await page.goto(origin + path);
    await page.locator(".nav-rail").waitFor();
  }
  async function screenshot(name) {
    const path = join(evidenceDir, `revision7-${name}.png`);
    await page.waitForTimeout(250);
    await page.screenshot({ path });
    facts.screenshots.push(path);
  }
  async function send(text) {
    // A stored conversation initially renders the home composer until its history arrives.
    const composer = new URL(page.url()).pathname.startsWith("/chat/") ? page.locator(".conversation-composer") : page;
    const input = composer.getByRole("textbox", { name: "聊天输入", exact: true });
    await input.fill(text);
    await composer.getByRole("button", { name: "发送消息", exact: true }).click();
    await page.waitForURL(/\/chat\/.+/);
    return new URL(page.url()).pathname.split("/").at(-1);
  }
  const completed = async (id, question) => eventually(() => api("GET", `/api/conversations/${id}`), conversation => conversation.run_status === "completed" && (!question || conversation.messages.some(message => message.role === "user" && message.content === question)), `completed conversation ${id}`);
  const git = (...args) => execFileSync("git", args, { cwd: workspacePath, encoding: "utf8", windowsHide: true });
  git("init", "-b", "qa-workbench-r6");
  await writeFile(join(workspacePath, "revision7-result.txt"), "Before work\n");
  await writeFile(join(workspacePath, "staged.txt"), "Before staged change\n");
  git("add", "--", "revision7-result.txt", "staged.txt");
  git("-c", "user.name=Workbench QA", "-c", "user.email=qa@local.invalid", "commit", "-m", "Create isolated R7 fixture");
  await writeFile(join(workspacePath, "staged.txt"), "Prepared staged change\n");
  git("add", "--", "staged.txt");
  const indexBefore = await readFile(join(workspacePath, ".git", "index"));

  const registeredWorkspace=await api("POST","/api/workspaces",{name:"验收项目",root_path:workspacePath});
  await api("POST","/api/workspaces",{name:"没有本地目录的项目"});
  const archived=await api("POST","/api/workspaces",{name:"归档项目"});await api("DELETE",`/api/workspaces/${archived.id}`);
  const missingWorkspace=await fetch(base+"/api/workspace-git",{headers});assert.equal(missingWorkspace.status,400);
  facts.checks.push("git_requires_explicit_registered_workspace_no_cwd_fallback");
  let sequence = 0;
  let retryRejected=false;
  let skillTarget;
  let skillRoot;
  const catalogRequests = [];
  const provider = createServer(async (request, response) => {
    if (request.method === "GET" && request.url === "/v1/models") {
      catalogRequests.push(request.headers.authorization === "Bearer controlled-test-credential");
      response.writeHead(200, { "Content-Type": "application/json" });
      response.end(JSON.stringify({ data: [
        { id: "r7-multimodal", input_modalities: ["text", "image"], output_modalities: ["text"] },
        { id: "r7-image", input_modalities: ["text"], output_modalities: ["image"] },
      ] }));
      return;
    }
    let raw = "";
    for await (const chunk of request) raw += chunk;
    const body = JSON.parse(raw);
    if (!body.stream) {
      response.writeHead(200, { "Content-Type": "application/json" });
      response.end(JSON.stringify({ id: "r7-verify", choices: [{ index: 0, message: { role: "assistant", content: "pong" }, finish_reason: "stop" }] }));
      return;
    }
    const lastUser = body.messages.findLastIndex(message => message.role === "user");
    const question = body.messages[lastUser]?.content ?? "";
    if(question==="R8_RETRY_PROTOCOL"&&!retryRejected){retryRejected=true;response.writeHead(400,{"Content-Type":"application/json"});response.end(JSON.stringify({error:{message:"controlled retry failure",type:"invalid_request_error",code:"invalid_request_error"}}));return;}
    const hasTool = body.messages.slice(lastUser + 1).some(message => message.role === "tool");
    const skillRequest = question === SKILL_QUESTION;
    const protocolTools=body.messages.slice(lastUser+1).filter(message=>message.role==="assistant").flatMap(message=>message.tool_calls??[]).map(call=>call.function?.name);
    const nextSkillTool=!protocolTools.includes("load_skill")?"load_skill":!protocolTools.includes("write_file")?"write_file":null;
    const tool = question === WORK_QUESTION&&!hasTool || skillRequest&&Boolean(nextSkillTool);
    const actualToolMessages = body.messages.slice(lastUser + 1).filter(message => message.role === "tool" && typeof message.content === "string");
    facts.requests.push({hostSkillAdvertised:body.messages.some(message=>message.role==="system"&&message.content?.includes(HOST_SKILL)), question, model: body.model, hasTool, userHistory: body.messages.filter(message => message.role === "user").map(message => message.content), reasoningRoundTrip: body.messages.some(message => message.reasoning_content?.includes("R7_TOOL_REASONING")), knowledgeGrounded: body.messages.some(message => message.role === "system" && typeof message.content === "string" && message.content.includes(KNOWLEDGE_TAIL)), skillToolHasMarker: actualToolMessages.some(message => message.content.includes("R7_CODEX_SKILL_FULL_TAIL")), skillToolHasRoot: Boolean(skillRoot) && actualToolMessages.some(message => message.content.includes(skillRoot)) });
    if (hasTool) await new Promise(resolveWait => setTimeout(resolveWait, 1600));
    const delta = tool ? {
      reasoning_content: skillRequest ? "调用已发现技能的实际读取工具。" : TOOL_REASONING,
      content: skillRequest ? "正在读取实际技能文件。" : "正在调用实际文件工具。",
      tool_calls: [{ index: 0, id: `r7-tool-${++sequence}`, type: "function", function: { name: skillRequest ? nextSkillTool : "write_file", arguments: JSON.stringify(skillRequest ? nextSkillTool==="load_skill"?{ name: skillTarget }:{path:"skill-procedure-result.txt",content:SKILL_ARTIFACT} : { path: "revision7-result.txt", content: FILE_CONTENT }) } }],
    } : { content: question === ROOT_QUESTION ? ROOT_ANSWER : question === WORK_QUESTION ? WORK_ANSWER : skillRequest ? "R7 技能工具读取结束。" : question === LATER_QUESTION ? "R7 原路径后续回答" : question === INDEPENDENT_QUESTION ? "R7 独立回答" : `R7 分支回答：${question}` };
    response.writeHead(200, { "Content-Type": "text/event-stream" });
    response.end(`data: ${JSON.stringify({ choices: [{ index: 0, delta, finish_reason: null }] })}\n\ndata: ${JSON.stringify({ choices: [{ index: 0, delta: {}, finish_reason: tool ? "tool_calls" : "stop" }], usage: { prompt_tokens: 25, completion_tokens: 15, total_tokens: 40 } })}\n\ndata: [DONE]\n\n`);
  });
  await new Promise(resolveListen => provider.listen(0, "127.0.0.1", resolveListen));

  try {
    const model = await api("POST", "/api/llm/models", { provider: "deepseek", label: "R7 协议验收模型", model: "deepseek-chat", base_url: `http://127.0.0.1:${provider.address().port}/v1`, api_format: "openai", api_key: "controlled-test-credential", temperature: 0, max_tokens: 2048, invoke_timeout_ms: 15_000 });
    await api("POST", `/api/llm/models/${model.id}/verify`);
    await api("POST", `/api/llm/models/${model.id}/activate`);
    await api("PUT", "/api/security/approval-mode", { mode: "partial_approval" });
    await navigate("/chat");
    const rootId = await send(ROOT_QUESTION);
    await completed(rootId, ROOT_QUESTION);
    await page.getByText(ROOT_ANSWER, { exact: true }).waitFor();
    await send(WORK_QUESTION);
    await eventually(() => api("GET", `/api/conversations/${rootId}`), conversation => conversation.execution_history.some(record => record.name === "write_file" && record.executionStatus === "succeeded"), "real write_file execution before refresh");
    await page.reload();
    const rootAfterWork = await completed(rootId, WORK_QUESTION);
    await page.getByText(WORK_ANSWER, { exact: true }).waitFor();
    assert.equal(await readFile(join(workspacePath, "revision7-result.txt"), "utf8"), FILE_CONTENT);
    assert.equal(rootAfterWork.workspace_mode, "auto");
    const workMessage = rootAfterWork.messages.find(message => message.role === "user" && message.content === WORK_QUESTION);
    assert.ok(workMessage);
    const { turns } = await api("GET", `/api/conversations/${rootId}/turns`);
    const workTurn = turns.find(turn => turn.user_message_id === workMessage.id);
    assert.equal(workTurn?.status, "completed");
    const context = await api("GET", `/api/conversations/${rootId}/context-manifest`);
    assert.equal(context.turn_id, workTurn.id);
    assert.ok(context.manifest.entries.some(entry => entry.kind === "interaction_route" && entry.source_id === "work" && entry.status === "resolved"));
    const actualThoughts = [...Object.values(rootAfterWork.message_reasoning ?? {}), ...Object.values(rootAfterWork.turn_reasoning ?? {})].join("\n");
    assert.ok(actualThoughts.includes("R7_TOOL_REASONING"));
    const answer = page.locator(".conversation-message-assistant").filter({ hasText: WORK_ANSWER });
    const process = answer.locator('.message-process[aria-label="本轮处理过程"]');
    await process.locator("summary").first().click();
    await process.locator(".message-reasoning").filter({ hasText: "R7_TOOL_REASONING" }).waitFor();
    await process.locator('.conversation-tool-card[data-status="success"]').waitFor();
    assert.equal(await page.locator('.conversation-assistant-avatar, [class*="conversation-assistant-avatar"]').count(), 0);
    assert.equal(await answer.locator("img").count(), 0);
    await screenshot("actual-process-after-refresh");
    facts.checks.push("auto_mode_persists_while_effective_turn_routes_to_work", "real_tool_file_and_provider_reasoning_survive_refresh", "assistant_has_no_avatar");

    const executionToggle = page.getByRole("button", { name: "执行与审批", exact: true });
    const gitToggle = page.getByRole("button", { name: "Git 文件变更", exact: true });
    assert.equal(await page.locator(".glass-header").getByRole("button", { name: "执行与审批", exact: true }).count(), 1);
    assert.equal(await page.locator(".glass-header").getByRole("button", { name: "Git 文件变更", exact: true }).count(), 1);
    assert.equal(await gitToggle.getAttribute("aria-pressed"),"false");
    assert.equal(await page.locator("#chat-execution-panel").count(),0);
    const layout=await page.locator(".ui-opt-chat").evaluate(el=>{const chat=el.querySelector(".chat-region");const parent=el.getBoundingClientRect(),box=chat.getBoundingClientRect();return {tracks:getComputedStyle(el).gridTemplateColumns,rightGap:parent.right-box.right,width:box.width};});
    assert.ok(layout.rightGap<=1,JSON.stringify(layout));
    assert.equal(await page.locator(".ui-opt-chat").getAttribute("data-history-collapsed"), "true");
    assert.equal(layout.tracks.split(" ").length,1);
    facts.layout=layout;facts.checks.push("work_mode_keeps_git_collapsed_and_no_right_reserved_gap");
    await screenshot("chat-no-right-gap");
    const gitRequests=[];const trackGit=request=>{if(new URL(request.url()).pathname==="/api/workspace-git")gitRequests.push(request.url());};page.on("request",trackGit);
    await gitToggle.click();
    await page.getByText("选择你要查看的本地工作空间。",{exact:true}).waitFor();
    assert.equal(gitRequests.length,0);
    const choices=await page.getByRole("combobox",{name:"Git 工作空间",exact:true}).locator("option").allTextContents();
    assert.ok(!choices.some(value=>value.includes("应用工作目录")||value.includes("没有本地目录的项目")||value.includes("归档项目")));
    await page.getByRole("combobox",{name:"Git 工作空间",exact:true}).selectOption(registeredWorkspace.id);
    await eventually(()=>gitRequests.length,v=>v>0,"explicit Git workspace request");
    assert.ok(gitRequests.every(value=>new URL(value).searchParams.get("workspace_id")===registeredWorkspace.id));page.off("request",trackGit);
    facts.checks.push("git_only_reads_explicit_workspace_and_starts_without_selection");
    await page.getByRole("complementary", { name: "Git 文件变更", exact: true }).waitFor();
    await page.getByRole("button", { name: "查看工作区差异 revision7-result.txt", exact: true }).click();
    await page.getByRole("dialog").getByLabel("文件差异内容").filter({ hasText: `+${FILE_CONTENT}` }).waitFor();
    await screenshot("git-diff");
    await page.getByRole("dialog").getByRole("button", { name: /^关闭/ }).click();
    await executionToggle.click();
    await page.locator(".execution-sidebar").waitFor();
    assert.equal(await gitToggle.getAttribute("aria-pressed"), "false");
    assert.equal(await executionToggle.getAttribute("aria-pressed"), "true");
    await executionToggle.click();
    await page.locator(".execution-sidebar").waitFor({ state: "hidden" });
    assert.deepEqual(await readFile(join(workspacePath, ".git", "index")), indexBefore);
    facts.checks.push("topbar_execution_git_toggles_control_actual_panels", "git_diff_matches_actual_file_and_preserves_index");

    await send(LATER_QUESTION);
    const sourceBeforeBranch = await completed(rootId, LATER_QUESTION);
    const sourceAnswer = sourceBeforeBranch.messages.find(message => message.role === "assistant" && message.content === ROOT_ANSWER);
    assert.ok(sourceAnswer);
    await navigate("/chat");
    const independentId = await send(INDEPENDENT_QUESTION);
    await completed(independentId, INDEPENDENT_QUESTION);
    assert.notEqual(independentId, rootId);
    const openBranches = page.getByRole("button", { name: "管理对话分支", exact: true });
    const branchDialog = page.getByRole("dialog", { name: "对话分支", exact: true });
    await navigate("/chat/" + rootId);
    await openBranches.click();
    await branchDialog.getByRole("combobox", { name: "新分支起点", exact: true }).selectOption(sourceAnswer.id);
    await branchDialog.getByRole("button", { name: "新建分支", exact: true }).click();
    await page.waitForURL(url => url.pathname.startsWith("/chat/") && url.pathname !== "/chat/" + rootId);
    const branchId = new URL(page.url()).pathname.split("/").at(-1);
    assert.notEqual(branchId, independentId);
    // Route change precedes the history read and the branch composer's draft reset.
    await branchDialog.waitFor({ state: "hidden" });
    await page.getByText("R7 原路径后续回答", { exact: true }).waitFor({ state: "hidden" });
    await page.getByText(ROOT_ANSWER, { exact: true }).waitFor();
    assert.equal(await send(BRANCH_QUESTION), branchId);
    await completed(branchId, BRANCH_QUESTION);
    let overview = await eventually(() => api("GET", "/api/message-canvas"),
      result => result.conversations.some(conversation => conversation.id === branchId && conversation.branch && conversation.status === "completed"),
      "created branch is persisted in the message canvas");
    const branch = overview.conversations.find(conversation => conversation.id === branchId);
    const independent = overview.conversations.find(conversation => conversation.id === independentId);
    assert.equal(overview.conversations.length, 3);
    assert.equal(branch.branch.parent_node_id, sourceAnswer.id);
    assert.equal(branch.nodes[0].parent_id, sourceAnswer.id);
    assert.equal(independent.nodes[0].parent_id, null);
    assert.deepEqual(facts.requests.find(request => request.question === BRANCH_QUESTION).userHistory, [ROOT_QUESTION, BRANCH_QUESTION]);
    assert.deepEqual((await api("GET", "/api/conversations/" + rootId)).messages, sourceBeforeBranch.messages);
    facts.checks.push("branch_dialog_forks_from_selected_answer_without_modifying_main_history");

    await openBranches.click();
    await branchDialog.getByRole("link", { name: "查看本组画布" }).click();
    await page.waitForURL(url => url.pathname === "/connectors" && url.searchParams.get("conversation") === branchId);
    const groupCanvas = page.getByRole("region", { name: "对话组消息画布", exact: true });
    const familyFilter = groupCanvas.getByRole("combobox", { name: "当前对话组", exact: true });
    const pathFilter = groupCanvas.getByRole("combobox", { name: "当前续聊路径", exact: true });
    const messageNodes = groupCanvas.locator(".react-flow__node-conversationSpeech");
    const messageNode = id => groupCanvas.locator('.react-flow__node-conversationSpeech[data-id="' + id + '"]');
    async function expectGroup(ids, label) {
      const expected = overview.conversations.filter(conversation => ids.includes(conversation.id))
        .flatMap(conversation => conversation.nodes.map(node => node.id)).sort();
      await eventually(() => messageNodes.evaluateAll(nodes => nodes.map(node => node.dataset.id).sort()),
        actual => JSON.stringify(actual) === JSON.stringify(expected), label);
    }
    await eventually(() => familyFilter.inputValue(), value => value === rootId, "branch deep link selects its root group");
    await eventually(() => pathFilter.inputValue(), value => value === branchId, "branch deep link selects the exact path");
    await expectGroup([rootId, branchId], "canvas shows only the selected group");
    await familyFilter.selectOption(independentId);
    await expectGroup([independentId], "independent group shows only its own messages");
    await familyFilter.selectOption(rootId);
    await expectGroup([rootId, branchId], "returning to the root group restores both paths");
    await pathFilter.selectOption(rootId);
    assert.equal(await pathFilter.inputValue(), rootId);
    await pathFilter.selectOption(branchId);
    await groupCanvas.getByText("另一条路径", { exact: true }).waitFor({ state: "attached" });
    await eventually(() => groupCanvas.locator('[data-id="message:' + branch.nodes[0].id + '"] .react-flow__edge-path').getAttribute("d"),
      value => value?.includes("C"), "the selected answer is connected to its fork by a curved edge");
    assert.equal(await groupCanvas.locator('.message-branch-node[data-role="process"]').count(), 0);
    assert.ok(overview.conversations.every(conversation => conversation.nodes.every(node => ["user", "assistant"].includes(node.role))));
    assert.ok(overview.conversations.every(conversation => conversation.nodes.every(node => !node.content.includes("正在调用实际文件工具。"))));
    facts.checks.push("canvas_deep_link_selects_group_and_path_and_excludes_unrelated_groups", "message_canvas_keeps_user_and_assistant_nodes_and_real_branch_edges");

    const canvasQuestion = "R7 从画布继续当前分支";
    const composer = groupCanvas.locator('[aria-label="当前路径续聊"]');
    await composer.getByRole("textbox", { name: "聊天输入", exact: true }).fill(canvasQuestion);
    await composer.getByRole("button", { name: "发送消息", exact: true }).click();
    await completed(branchId, canvasQuestion);
    overview = await eventually(() => api("GET", "/api/message-canvas"),
      result => result.conversations.find(conversation => conversation.id === branchId)?.nodes.some(node => node.content === canvasQuestion),
      "canvas continuation is stored on the selected branch");
    assert.deepEqual(facts.requests.find(request => request.question === canvasQuestion).userHistory, [ROOT_QUESTION, BRANCH_QUESTION, canvasQuestion]);
    await expectGroup([rootId, branchId], "group includes the persisted continuation");
    await screenshot("conversation-group-branch");
    facts.checks.push("canvas_continuation_writes_to_selected_path_and_keeps_its_history");

    const sourceNode = messageNode(sourceAnswer.id);
    const sourceHeader = sourceNode.locator(".message-branch-node > header");
    const isLayoutWrite = response => new URL(response.url()).pathname === "/api/message-canvas/layout" && response.request().method() === "PUT";
    const fitted = page.waitForResponse(isLayoutWrite);
    await groupCanvas.getByRole("button", { name: "适应画布", exact: true }).click();
    assert.ok((await fitted).ok(), "fitted viewport was saved");
    const header = await eventually(() => sourceHeader.boundingBox(),
      box => box && box.width > 20 && box.height > 4 && box.x > 80 && box.y > 70 && box.x + box.width < page.viewportSize().width && box.y + box.height < page.viewportSize().height,
      "source message header is inside the fitted canvas");
    const positionOf = locator => locator.evaluate(element => {
      const matrix = new DOMMatrixReadOnly(getComputedStyle(element).transform);
      return { x: matrix.m41, y: matrix.m42 };
    });
    const samePoint = (actual, expected) => actual && Math.abs(actual.x - expected.x) < 0.1 && Math.abs(actual.y - expected.y) < 0.1;
    const previousPoint = await positionOf(sourceNode);
    const dragX = header.x + header.width / 2;
    const dragY = header.y + header.height / 2;
    const dragSaved = page.waitForResponse(response => isLayoutWrite(response) &&
      response.request().postDataJSON().nodes.some(point => point.id === sourceAnswer.id &&
        (Math.abs(point.x - previousPoint.x) > 1 || Math.abs(point.y - previousPoint.y) > 1)));
    await page.mouse.move(dragX, dragY);
    await page.mouse.down();
    await page.mouse.move(dragX + 46, dragY + 28, { steps: 12 });
    await page.mouse.up();
    const dragResponse = await dragSaved;
    assert.ok(dragResponse.ok(), "dragged message coordinates were saved");
    const renderedAfterDrag = await positionOf(sourceNode);
    assert.ok(!samePoint(renderedAfterDrag, previousPoint), "dragging moves the source message in the canvas");
    const movedPoint = await eventually(async () => (await api("GET", "/api/message-canvas")).layout.nodes.find(node => node.id === sourceAnswer.id),
      point => samePoint(point, renderedAfterDrag), "SQLite layout matches the rendered position after dragging");
    const moved = await api("GET", "/api/message-canvas");
    const allMessageIds = new Set(overview.conversations.flatMap(conversation => conversation.nodes.map(node => node.id)));
    assert.ok(moved.layout.nodes.every(node => allMessageIds.has(node.id)));
    const reloadWrites = [];
    const trackReloadWrite = request => {
      if (request.method() === "PUT" && new URL(request.url()).pathname === "/api/message-canvas/layout") {
        reloadWrites.push(request.postDataJSON().nodes.find(node => node.id === sourceAnswer.id) ?? null);
      }
    };
    page.on("request", trackReloadWrite);
    const afterReload = page.waitForResponse(response => new URL(response.url()).pathname === "/api/message-canvas" && response.request().method() === "GET");
    try {
      await page.reload();
      const reloadedLayout = (await (await afterReload).json()).layout;
      facts.drag = { previousPoint, renderedAfterDrag, movedPoint, reloadedPoint: reloadedLayout.nodes.find(node => node.id === sourceAnswer.id), reloadWrites };
      assert.ok(samePoint(facts.drag.reloadedPoint, movedPoint), "reloaded page reads the dragged coordinates from the API");
      await eventually(() => positionOf(sourceNode),
        point => samePoint(point, movedPoint),
        "dragged message coordinates survive reload");
      for (const point of reloadWrites) assert.ok(samePoint(point, movedPoint), "initial viewport sync must preserve saved node coordinates");
    } finally {
      page.off("request", trackReloadWrite);
    }
    facts.checks.push("absolute_message_drag_coordinates_survive_reload");
    facts.conversations = { rootId, branchId, independentId, sourceAnswerId: sourceAnswer.id };

    const canvasAgent = await api("POST", "/api/agents", {
      name: "R7 画布验收助手", user_prompt: "使用中文回答。", allowed_tools: [], capabilities: [],
    });
    await navigate("/connectors?conversation=" + rootId);
    await page.getByRole("button", { name: "新建画布", exact: true }).click();
    const newCanvas = page.getByRole("dialog", { name: "新建画布", exact: true });
    await newCanvas.getByLabel("画布名称", { exact: true }).fill("R7 待管理画布");
    await newCanvas.getByRole("combobox", { name: "画布所属智能体", exact: true }).selectOption(canvasAgent.id);
    await newCanvas.getByRole("button", { name: "创建画布", exact: true }).click();
    await newCanvas.waitFor({ state: "hidden" });
    const canvasCatalog = await api("GET", "/api/connectors");
    const savedCanvas = canvasCatalog.canvases.find(canvas => canvas.name === "R7 待管理画布");
    assert.ok(savedCanvas);
    assert.equal(savedCanvas.agent_id, canvasAgent.id);
    const canvasSelector = page.getByRole("combobox", { name: "当前画布", exact: true });
    await eventually(() => canvasSelector.inputValue(), value => value === savedCanvas.id, "created canvas is selected");
    await page.getByRole("button", { name: "管理全部画布", exact: true }).click();
    const manager = page.getByRole("dialog", { name: "管理全部画布", exact: true });
    await manager.getByRole("article").filter({ hasText: "R7 待管理画布" }).getByRole("button", { name: "重命名", exact: true }).click();
    const rename = page.getByRole("dialog", { name: "重命名画布", exact: true });
    const renamed = "R7 已重命名画布";
    await rename.getByLabel("画布名称", { exact: true }).fill(renamed);
    await rename.getByRole("button", { name: "保存", exact: true }).click();
    const managedRow = manager.getByRole("article").filter({ hasText: renamed });
    await managedRow.waitFor();
    await eventually(() => api("GET", "/api/connectors"), catalog => catalog.canvases.some(canvas => canvas.id === savedCanvas.id && canvas.name === renamed), "canvas rename persists");
    await screenshot("canvas-manager-renamed");
    const deleteCanvas = managedRow.getByRole("button", { name: "删除此画布", exact: true });
    const confirmDelete = page.getByRole("dialog", { name: "删除所选画布", exact: true });
    await deleteCanvas.click();
    await confirmDelete.getByRole("button", { name: "取消", exact: true }).click();
    await managedRow.waitFor();
    assert.ok((await api("GET", "/api/connectors")).canvases.some(canvas => canvas.id === savedCanvas.id));
    await deleteCanvas.click();
    await confirmDelete.getByRole("button", { name: "确认删除", exact: true }).click();
    await manager.getByRole("status").filter({ hasText: "已删除 " + renamed }).waitFor();
    assert.ok(!(await api("GET", "/api/connectors")).canvases.some(canvas => canvas.id === savedCanvas.id));
    assert.equal((await api("GET", "/api/agents/" + canvasAgent.id)).id, canvasAgent.id);
    assert.deepEqual((await api("GET", "/api/conversations/" + rootId)).messages, sourceBeforeBranch.messages);
    await page.keyboard.press("Escape");
    await manager.waitFor({ state: "hidden" });
    assert.equal(await canvasSelector.inputValue(), "");
    await groupCanvas.waitFor();
    facts.checks.push("canvas_manager_creates_renames_and_deletes_with_confirmation_without_deleting_agent_or_chat");

    await navigate("/chat/" + branch.id);
    await page.getByRole("button", { name: "打开任务列表", exact: true }).click();
    await eventually(() => page.locator(".conversation-row").count(), count => count === 2, "sidebar lists two families rather than three execution conversations");
    assert.ok(!(await page.locator(".conversation-message-user").allTextContents()).some(value => value.includes(LATER_QUESTION)));
    await openBranches.click();
    const paths = branchDialog.getByRole("navigation", { name: "分支路径列表" });
    assert.equal(await paths.getByRole("button").count(), 2);
    await paths.getByRole("button", { name: /^主线/ }).click();
    await branchDialog.getByRole("button", { name: "打开路径继续", exact: true }).click();
    await page.waitForURL(`**/chat/${rootId}`);
    await page.getByText("R7 原路径后续回答", { exact: true }).waitFor();
    await screenshot("family-path-selector");
    facts.checks.push("sidebar_groups_real_alternatives_into_one_family", "path_selector_opens_correct_full_history");

    await navigate("/settings");
    const moreActions = page.getByText("更多操作", { exact: true });
    const settingsOverview = page.locator(".settings-overview");
    const modelDialog = page.getByRole("dialog", { name: "模型配置", exact: true });
    await settingsOverview.getByRole("button", { name: "权限审批", exact: true }).waitFor();
    assert.equal(await page.locator(".settings-canvas-stage").count(), 0);
    const canvasEntry = settingsOverview.getByRole("link", { name: "查看设置画布", exact: true });
    assert.equal(await settingsOverview.getByRole("button", { name: "高级模型池", exact: true }).count(), 0);
    await screenshot("settings-overview");
    await settingsOverview.getByRole("button", { name: "模型配置", exact: true }).click();
    await modelDialog.getByText("当前默认用途", { exact: true }).waitFor();
    assert.equal(await modelDialog.getByRole("textbox", { name: "手动模型 ID", exact: true }).count(), 0);
    await modelDialog.getByRole("button", { name: "添加厂商账户", exact: true }).click();
    const catalogDialog = page.getByRole("dialog", { name: "添加厂商账户", exact: true });
    await catalogDialog.getByLabel("厂商", { exact: true }).selectOption("custom");
    await catalogDialog.getByLabel("账户名称", { exact: true }).fill("R7 模型目录验收账户");
    await catalogDialog.getByLabel("API 地址", { exact: true }).fill(`http://127.0.0.1:${provider.address().port}/v1`);
    await catalogDialog.getByLabel("API Key", { exact: true }).fill("controlled-test-credential");
    const multi = catalogDialog.getByRole("group", { name: "r7-multimodal 的用途" });
    await multi.getByRole("checkbox", { name: "对话", exact: true }).check();
    await multi.getByRole("checkbox", { name: "图像理解", exact: true }).check();
    await catalogDialog.getByRole("group", { name: "r7-image 的用途" }).getByRole("checkbox", { name: "图像生成", exact: true }).check();
    await catalogDialog.getByRole("button", { name: "保存配置", exact: true }).click();
    await modelDialog.getByText("R7 模型目录验收账户", { exact: true }).waitFor();
    const savedModel = modelDialog.locator(".model-config-saved-row").filter({ hasText: "r7-multimodal" });
    assert.equal(await savedModel.count(), 1);
    assert.ok((await savedModel.innerText()).includes("图像理解"));
    assert.ok(catalogRequests.includes(true));
    assert.equal((await api("GET", "/api/llm/models")).find(item => item.active)?.id, model.id);
    assert.equal((await api("GET", "/api/service-defaults/vision")).id, null);
    await screenshot("model-configuration");
    facts.checks.push("supplier_catalog_saves_two_models_with_one_key_and_keeps_default_routes");
    await modelDialog.getByRole("button", { name: "关闭模型配置", exact: true }).click();
    await settingsOverview.getByText("更多设置", { exact: true }).click();
    await screenshot("settings-overview-expanded");
    await canvasEntry.click();
    await page.waitForURL("**/settings?view=canvas");
    facts.checks.push("global_settings_is_default_and_canvas_is_an_explicit_view");
    await eventually(() => page.locator(".settings-canvas-stage [data-settings-group]").count(), count => count === 6, "six collapsed category nodes");
    assert.equal(await page.locator('.settings-canvas-stage .react-flow__node[data-id^="setting:"]').count(), 0);
    assert.equal(await page.getByText("点击展开设置项", { exact: true }).count(), 0);
    const category = page.getByRole("button", { name: "编辑模型与服务", exact: true });
    assert.equal(await category.getAttribute("aria-expanded"), "false");
    await category.click();
    await page.getByRole("button", { name: "编辑模型配置", exact: true }).waitFor();
    assert.equal(await page.locator('.settings-canvas-stage .react-flow__node[data-id^="setting:"]').count(), 5);
    assert.equal(await page.getByRole("dialog").count(), 0);
    await eventually(() => page.locator('[data-id="derived:setting-child:model"] .react-flow__edge-path').getAttribute("d"), value => value?.includes("C"), "inline settings child curve");
    const focusFacts=await eventually(()=>page.locator(".settings-canvas-stage").evaluate((stage)=>{
      const boxes=[...stage.querySelectorAll(".react-flow__node")].map(node=>node.getBoundingClientRect());
      const area=stage.getBoundingClientRect();
      const left=Math.min(...boxes.map(box=>box.left)), right=Math.max(...boxes.map(box=>box.right));
      const top=Math.min(...boxes.map(box=>box.top)), bottom=Math.max(...boxes.map(box=>box.bottom));
      return {count:boxes.length,x:(left+right-area.left-area.right)/2,y:(top+bottom-area.top-area.bottom)/2,
        inFrame:left>=area.left&&right<=area.right&&top>=area.top&&bottom<=area.bottom};
    }),value=>value.count===12&&value.inFrame&&Math.abs(value.x)<35&&Math.abs(value.y)<35,"expanded settings retain the overview and branch in frame");
    facts.settingsFocus=focusFacts;facts.checks.push("expanded_settings_camera_keeps_overview_and_selected_branch_in_frame");
    await screenshot("settings-inline-expanded");
    await page.getByRole("button", { name: "编辑模型配置", exact: true }).click();
    await modelDialog.waitFor();
    assert.equal(await page.getByRole("dialog").count(), 1);
    await modelDialog.getByRole("button", { name: "关闭模型配置", exact: true }).click();
    assert.equal(await page.getByRole("dialog").count(), 0);
    assert.equal(new URL(page.url()).search, "?view=canvas");
    assert.equal(await category.getAttribute("aria-expanded"), "true");
    await category.click();
    await page.getByRole("button", { name: "编辑模型配置", exact: true }).waitFor({ state: "detached" });
    await category.click();
    await page.getByRole("button", { name: "编辑模型配置", exact: true }).waitFor();
    await page.reload();
    await category.waitFor();
    assert.equal(await category.getAttribute("aria-expanded"), "false");
    assert.equal(await page.locator('.settings-canvas-stage .react-flow__node[data-id^="setting:"]').count(), 0);
    const arrange = page.getByRole("button", { name: "整理布局", exact: true });
    assert.equal(await arrange.count(), 0);
    await moreActions.press("Enter");
    await arrange.press("Escape");
    assert.equal(await arrange.count(), 0);
    assert.equal(await moreActions.evaluate(element => element === document.activeElement), true);
    await moreActions.press("Enter");
    await arrange.click();
    await eventually(()=>page.locator('[data-id="setting-group:models"]').evaluate(node=>node.style.transform),value=>value.includes("436px")&&value.includes("146px"),"explicit layout reset uses balanced default coordinates");
    const returnToOverview = page.getByRole("button",{name:"返回收拢视图",exact:true});
    assert.equal(await returnToOverview.count(),0);
    await category.click();
    await page.getByRole("combobox",{name:"查看已展开分支"}).selectOption("models");
    await page.locator('.settings-canvas-stage .settings-outside-focus').first().waitFor();
    await moreActions.click();
    await screenshot("settings-more-actions");
    await returnToOverview.click();
    assert.equal(await category.getAttribute("aria-expanded"),"false");
    assert.equal(await page.locator('.settings-canvas-stage .settings-outside-focus').count(),0);
    facts.checks.push("explicit_relayout_and_overview_restore_default_geometry");
    await screenshot("settings-inline-collapsed");
    facts.checks.push("settings_categories_expand_actual_leaf_nodes_without_parent_modal", "settings_leaf_opens_existing_form", "settings_returns_to_collapsed_overview_on_reload");

    await moreActions.click();
    await page.getByRole("button", { name: "高级模型池", exact: true }).click();
    const poolDialog = page.getByRole("dialog", { name: "高级模型池", exact: true });
    const tabs = poolDialog.getByRole("tablist", { name: "模型池服务类型" });
    await tabs.waitFor();
    assert.deepEqual(await tabs.getByRole("tab").allTextContents(), ["对话", "识图", "生图"]);
    assert.equal(await poolDialog.getByRole("tabpanel").count(), 1);
    const chatTab = tabs.getByRole("tab", { name: "对话" });
    await chatTab.press("ArrowRight");
    await poolDialog.getByRole("tabpanel", { name: "识图" }).waitFor();
    await tabs.getByRole("tab", { name: "生图" }).click();
    await poolDialog.getByRole("tabpanel", { name: "生图" }).waitFor();
    assert.equal(await poolDialog.getByRole("button", { name: "新建模型池", exact: true }).count(), 1);
    await navigate("/settings?section=model-pools");
    await poolDialog.getByRole("tabpanel", { name: "对话" }).waitFor();
    facts.checks.push("settings_model_pools_secondary_entry_deep_link_and_service_tabs");

    const knowledgeName = "R7-知识全文.md";
    const tail = KNOWLEDGE_TAIL;
    const documentText = `# R7 知识资料\n${"这是一段实际导入的知识正文，用于验证完整文档分块与检索。\n".repeat(700)}${tail}\n`;
    assert.ok(documentText.length > 12_000);
    await navigate("/knowledge");
    await page.getByRole("button", { name: "导入资料", exact: true }).click();
    const importChooser = page.getByRole("dialog", { name: "导入资料", exact: true });
    await importChooser.getByRole("button", { name: "选择文件", exact: true }).waitFor();
    await importChooser.getByRole("button", { name: "选择文件夹", exact: true }).waitFor();
    await page.getByLabel("选择知识文档", { exact: true }).setInputFiles({ name: knowledgeName, mimeType: "text/markdown", buffer: Buffer.from(documentText, "utf8") });
    const knowledge = await eventually(() => api("GET", "/api/knowledge/documents"), result => result.documents.some(document => document.name === knowledgeName), "actual knowledge import");
    const document = knowledge.documents.find(entry => entry.name === knowledgeName);
    assert.equal(document.char_count, [...documentText].length);
    assert.ok(document.chunk_count > 1);
    const detail = await api("GET", `/api/knowledge/documents/${document.id}`);
    assert.equal(detail.text, documentText);
    assert.ok(detail.chunks.some(chunk => chunk.content.includes(tail)));
    await page.getByRole("dialog",{name:"导入结果",exact:true}).getByRole("button",{name:"关闭导入结果",exact:true}).click();
    await page.locator(".knowledge-document-open").filter({ hasText: knowledgeName }).click();
    await eventually(() => page.getByLabel("知识文档全文", { exact: true }).textContent(), value => value === documentText, "knowledge full text visible");
    await page.getByLabel("知识文档全文", { exact: true }).evaluate(element => element.scrollIntoView({ block: "end" }));
    await screenshot("knowledge-full-tail");
    await page.getByRole("button", { name: `关闭${knowledgeName}`, exact: true }).click();
    await page.getByRole("textbox", { name: "搜索知识内容", exact: true }).fill(tail);
    await page.locator(".knowledge-search-hit").filter({ hasText: tail }).waitFor();
    const searched = await api("GET", `/api/knowledge/search?q=${encodeURIComponent(tail)}`);
    assert.ok(searched.hits.some(hit => hit.document_id === document.id && hit.content.includes(tail) && hit.start_char > 12_000));
    await screenshot("knowledge-search-tail");
    await navigate(`/chat/${rootId}`);
    await send(KNOWLEDGE_QUESTION);
    await completed(rootId, KNOWLEDGE_QUESTION);
    assert.ok(facts.requests.some(request => request.question === KNOWLEDGE_QUESTION && request.knowledgeGrounded), "The actual provider system prompt must contain the imported document tail");
    await screenshot("knowledge-grounded-turn");
    await navigate("/knowledge");
    await page.getByRole("button", { name: `移出知识库 ${knowledgeName}`, exact: true }).click();
    await page.getByRole("dialog", { name: "移出资料库", exact: true }).getByRole("button", { name: "移出资料库", exact: true }).click();
    await eventually(() => api("GET", "/api/knowledge/documents"), result => !result.documents.some(entry => entry.id === document.id), "knowledge removal persisted");
    assert.equal((await api("GET", `/api/knowledge/search?q=${encodeURIComponent(tail)}`)).hits.length, 0);
    await page.reload();
    await page.getByText("导入第一份知识文档", { exact: true }).waitFor();
    facts.checks.push("knowledge_import_preserves_full_tail_beyond_preview_budget", "knowledge_tail_search_returns_canonical_excerpt", "knowledge_query_injects_full_tail_into_actual_provider_system_prompt", "knowledge_removal_deletes_document_and_search_chunks");

    const skillFixtures = [
      { directory: ".codex", name: "revision7-codex-real", sourceId: "workspace-codex", marker: "R7_CODEX_SKILL_FULL_TAIL" },
      { directory: ".claude", name: "revision7-claude-real", sourceId: "workspace-claude", marker: "R7_CLAUDE_SKILL_FULL_TAIL" },
    ];
    for (const fixture of skillFixtures) {
      const directory = join(workspacePath, fixture.directory, "skills", fixture.name);
      await mkdir(directory, { recursive: true });
      fixture.content = `---\nname: ${fixture.name}\n${fixture.directory===".codex"?"allowed-tools: Write\n":""}description: R7 actual workspace discovery fixture\n---\n\n${"Read the real workspace fixture and report its content.\n".repeat(35)}${fixture.marker}\n${fixture.directory===".codex"?"Use the Write tool to save skill-procedure-result.txt with the content R7_SKILL_PROCEDURE_EXECUTED.\n":""}`;
      fixture.path = join(directory, "SKILL.md");
      await writeFile(fixture.path, fixture.content);
    }
    const hostDirectory=join(workspacePath,".codex","skills",HOST_SKILL);await mkdir(hostDirectory,{recursive:true});
    await writeFile(join(hostDirectory,"SKILL.md"),`---\nname: ${HOST_SKILL}\ndescription: Requires an external Figma host function\n---\nCall tools.use_figma to edit the design.\n`);
    await navigate("/capabilities?category=skill");
    await page.getByRole("button", { name: "刷新发现", exact: true }).click();
    const skills = await eventually(() => api("GET", "/api/skills"), entries => skillFixtures.every(fixture => entries.some(skill => skill.source_id === fixture.sourceId && skill.display_name === fixture.name)), "actual Codex and Claude workspace skill discovery");
    const external=skills.find(skill=>skill.display_name===HOST_SKILL);assert.ok(external);assert.equal(external.compatibility.runtime_ready,false);assert.equal(external.compatibility.status,"external_host");
    const sourceFilter = page.getByRole("combobox", { name: "技能来源筛选", exact: true });
    assert.equal(await sourceFilter.inputValue(), "bundled");
    await page.getByRole("textbox", { name: "搜索技能", exact: true }).fill(HOST_SKILL);
    assert.equal(await page.getByRole("option").filter({ hasText: HOST_SKILL }).count(), 0);
    await sourceFilter.selectOption("other");
    await page.getByRole("option").filter({ hasText: HOST_SKILL }).click();
    await page.getByText("需要外部宿主能力", { exact: true }).waitFor();
    await screenshot("host-skill-dependencies");
    facts.checks.push("external_skills_hidden_by_default_and_host_dependencies_visible");
    for (const fixture of skillFixtures) {
      const skill = skills.find(entry => entry.source_id === fixture.sourceId && entry.display_name === fixture.name);
      assert.equal(skill.editable, false);
      assert.equal(resolve(skill.path), resolve(fixture.path));
      const loaded = await api("GET", `/api/skills/${encodeURIComponent(skill.name)}`);
      assert.equal(loaded.content, fixture.content);
      if (fixture.sourceId === "workspace-codex") { skillTarget = skill.name; skillRoot = loaded.root_dir; }
      await page.getByRole("textbox", { name: "搜索技能", exact: true }).fill(fixture.name);
      await page.getByRole("option").filter({ hasText: fixture.name }).click();
      const full = page.getByLabel("技能详情", { exact: true });
      await full.getByRole("button", { name: "展开完整使用说明", exact: true }).click();
      await eventually(() => full.locator(".skill-reading-content").textContent(), value => value === fixture.content, "actual discovered skill full text");
      await screenshot(`skill-${fixture.directory.slice(1)}-full-text`);
      await full.getByRole("button", { name: "收起全文", exact: true }).click();
    }
    facts.checks.push("real_codex_and_claude_workspace_skills_are_discovered_readonly", "discovered_skill_full_text_matches_actual_files");
    assert.ok(skillTarget && skillRoot);
    await navigate(`/chat/${rootId}`);
    await send(SKILL_QUESTION);
    const skillConversation = await completed(rootId, SKILL_QUESTION);
    assert.ok(skillConversation.execution_history.some(record => record.name === "load_skill" && record.executionStatus === "succeeded"));
    assert.ok(facts.requests.some(request => request.question === SKILL_QUESTION && request.hasTool && request.skillToolHasMarker && request.skillToolHasRoot), "The actual provider continuation must receive the SKILL.md tail and actual root directory from load_skill");
    await page.getByText("R7 技能工具读取结束。", { exact: true }).waitFor();
    await screenshot("actual-discovered-skill-tool");
    assert.equal(await readFile(join(workspacePath,"skill-procedure-result.txt"),"utf8"),SKILL_ARTIFACT);
    assert.ok(facts.requests.every(request=>!request.hostSkillAdvertised));
    assert.ok(skills.find(skill=>skill.display_name===skillFixtures[0].name).compatibility.tool_mappings.some(mapping=>mapping.from==="Write"&&mapping.to==="write_file"));
    facts.checks.push("actual_load_skill_returns_discovered_file_tail_and_root_to_provider","external_skill_procedure_runs_real_project_write_file");
    facts.skills = skillFixtures.map(({ name, sourceId, path }) => ({ name, sourceId, path }));
    await navigate("/capabilities?category=tool");
    const toolDialog=page.getByLabel("运行时工具",{exact:true});await toolDialog.getByLabel("工具列表",{exact:true}).waitFor();
    assert.equal(await page.getByRole("dialog").count(),0);
    await toolDialog.getByRole("textbox",{name:"搜索工具",exact:true}).fill("write_file");
    await toolDialog.locator(".tool-manager-row").filter({hasText:"写入文件"}).click();
    await toolDialog.getByLabel("工具详情",{exact:true}).getByRole("heading",{name:"写入文件",exact:true}).waitFor();
    await screenshot("compact-tool-detail");await toolDialog.getByRole("textbox",{name:"搜索工具",exact:true}).fill("");await screenshot("compact-tool-list");
    facts.checks.push("tools_use_persistent_list_and_detail_without_outer_modal");
    await navigate("/chat");
    const retryId=await send("R8_RETRY_PROTOCOL");
    await eventually(()=>api("GET",`/api/conversations/${retryId}`),conversation=>conversation.run_status==="failed","controlled first request failure");
    await page.getByRole("button",{name:"回退并重试",exact:true}).click();
    let retryConversation=await completed(retryId,"R8_RETRY_PROTOCOL");
    assert.equal(retryConversation.messages.filter(message=>message.role==="user").length,1);
    const retryUser=retryConversation.messages.find(message=>message.role==="user");
    const retryAnswer=retryConversation.messages.findLast(message=>message.role==="assistant"&&message.content);
    const answerArticle=page.locator(`article[data-message-id="${retryAnswer.id}"]`);
    await answerArticle.getByRole("button",{name:"复制回答",exact:true}).waitFor();
    await page.mouse.move(2,2);
    assert.equal(await answerArticle.getByRole("button",{name:"复制回答",exact:true}).evaluate(node=>getComputedStyle(node.parentElement).opacity),"1");
    await screenshot("persistent-message-actions");
    await answerArticle.getByRole("button",{name:"删除这条回答",exact:true}).click();
    await eventually(()=>api("GET",`/api/conversations/${retryId}`),conversation=>!conversation.messages.some(message=>message.id===retryAnswer.id),"single answer deletion persisted");
    retryConversation=await api("GET",`/api/conversations/${retryId}`);
    assert.ok(retryConversation.messages.some(message=>message.id===retryUser.id));
    await page.reload();
    await page.locator(`article[data-message-id="${retryUser.id}"]`).getByRole("button",{name:"回退这条消息",exact:true}).click();
    await eventually(()=>page.getByRole("textbox",{name:"聊天输入",exact:true}).inputValue(),value=>value==="R8_RETRY_PROTOCOL","rollback restores editable draft");
    assert.equal((await api("GET",`/api/conversations/${retryId}`)).messages.length,0);
    assert.ok((await api("GET",`/api/conversations/${rootId}`)).messages.length>0);
    await screenshot("rollback-restores-draft");
    facts.checks.push("failed_retry_replaces_original_user_without_duplicates","message_actions_are_always_visible_and_delete_only_selected_answer","user_rollback_restores_draft_and_preserves_other_conversations");
    facts.status = "PASS";
    return facts;
  } catch (error) {
    facts.status = "FAIL";
    facts.error = error.stack || String(error);
    throw error;
  } finally {
    await writeFile(join(evidenceDir, "revision7-facts.json"), JSON.stringify(facts, null, 2));
    await new Promise(resolveClose => provider.close(resolveClose));
  }
}

export const workbenchRevision7 = runWorkbenchRevision7;
