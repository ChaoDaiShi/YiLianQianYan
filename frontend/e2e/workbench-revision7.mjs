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
const SKILL_QUESTION = "按发现的 Codex 技能执行并保存结果";

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
    const input = page.getByRole("textbox", { name: "聊天输入", exact: true });
    await input.fill(text);
    await page.getByRole("button", { name: "发送消息", exact: true }).click();
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
  let skillTarget;
  let skillRoot;
  const provider = createServer(async (request, response) => {
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
    assert.equal(rootAfterWork.workspace_mode, "work");
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
    facts.checks.push("real_tool_file_and_provider_reasoning_survive_refresh", "assistant_has_no_avatar");

    const executionToggle = page.getByRole("button", { name: "执行与审批", exact: true });
    const gitToggle = page.getByRole("button", { name: "Git 文件变更", exact: true });
    assert.equal(await page.locator(".glass-header").getByRole("button", { name: "执行与审批", exact: true }).count(), 1);
    assert.equal(await page.locator(".glass-header").getByRole("button", { name: "Git 文件变更", exact: true }).count(), 1);
    assert.equal(await gitToggle.getAttribute("aria-pressed"),"false");
    assert.equal(await page.locator("#chat-execution-panel").count(),0);
    const layout=await page.locator(".ui-opt-chat").evaluate(el=>{const chat=el.querySelector(".chat-region");const parent=el.getBoundingClientRect(),box=chat.getBoundingClientRect();return {tracks:getComputedStyle(el).gridTemplateColumns,rightGap:parent.right-box.right,width:box.width};});
    assert.ok(layout.rightGap<=1,JSON.stringify(layout));assert.equal(layout.tracks.split(" ").length,2);
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
    await navigate(`/connectors?conversation=${rootId}`);
    let overview = await eventually(() => api("GET", "/api/message-canvas"), result => result.conversations.length === 2, "two actual independent conversations");
    await eventually(() => page.locator(".react-flow__node-conversationSpeech").count(), count => count === overview.conversations.reduce((total, conversation) => total + conversation.nodes.length, 0), "actual message forest nodes");
    assert.equal(await page.locator('.conversation-canvas-group, .react-flow__node-conversationGroup').count(), 0);
    assert.equal(await page.locator(".react-flow__node").count(), overview.conversations.flatMap(conversation => conversation.nodes).length);
    assert.equal((await api("GET", "/api/connectors")).canvases.length, 0);
    assert.equal(await page.getByRole("combobox", { name: "会话目标画布", exact: true }).count(), 0);
    assert.equal(await page.locator('.message-branch-node[data-role="process"]').count(),0);
    assert.ok(overview.conversations.every(conversation=>conversation.nodes.every(node=>node.role==="user"||node.role==="assistant")));
    assert.ok(overview.conversations.every(conversation=>conversation.nodes.every(node=>!node.content.includes("正在调用实际文件工具。"))));
    await page.getByRole("combobox", { name: "定位画布对话", exact: true }).selectOption(rootId);
    await page.locator(`[data-id="${sourceAnswer.id}"]`).getByRole("button", { name: "从此回答创建分支对话", exact: true }).click();
    await page.getByRole("textbox", { name: "分支对话提问", exact: true }).fill(BRANCH_QUESTION);
    await page.getByRole("button", { name: "发送到新分支", exact: true }).click();
    overview = await eventually(() => api("GET", "/api/message-canvas"), result => result.conversations.length === 3 && result.conversations.some(conversation => conversation.branch && conversation.status === "completed"), "actual alternative path completed");
    const branch = overview.conversations.find(conversation => conversation.branch);
    assert.equal(branch.branch.parent_node_id, sourceAnswer.id);
    assert.equal(branch.nodes[0].parent_id, sourceAnswer.id);
    const branchRequest = facts.requests.find(request => request.question === BRANCH_QUESTION);
    assert.deepEqual(branchRequest.userHistory, [ROOT_QUESTION, BRANCH_QUESTION]);
    assert.deepEqual((await api("GET", `/api/conversations/${rootId}`)).messages, sourceBeforeBranch.messages);
    await eventually(() => page.locator(`[data-id="message:${branch.nodes[0].id}"] .react-flow__edge-path`).getAttribute("d"), value => value?.includes("C"), "Bezier alternative-path connection");
    const independent = overview.conversations.find(conversation => conversation.id === independentId);
    assert.equal(independent.nodes[0].parent_id, null);
    assert.equal(await page.locator(`[data-id="message:${independent.nodes[0].id}"]`).count(), 0);
    await page.getByRole("button", { name: "取消", exact: true }).click();
    await page.getByRole("button", { name: "关闭消息详情", exact: true }).click();
    await page.getByRole("button", { name: "适应画布", exact: true }).click();
    await screenshot("message-forest-alternative-path");
    facts.checks.push("message_forest_contains_only_input_output_without_tool_or_backdrop", "alternative_path_uses_actual_ancestor_and_excludes_later_siblings", "independent_conversation_tree_has_no_cross_family_edge");

    await page.getByRole("combobox", { name: "定位画布对话", exact: true }).selectOption(rootId);
    const previous = await eventually(() => api("GET", "/api/message-canvas"), result => result.layout.nodes.some(node => node.id === sourceAnswer.id), "absolute message positions saved");
    await page.waitForTimeout(300);
    const previousPoint = previous.layout.nodes.find(node => node.id === sourceAnswer.id);
    const header = await page.locator(`[data-id="${sourceAnswer.id}"] .message-branch-node > header`).boundingBox();
    assert.ok(header);
    await page.mouse.move(header.x + 30, header.y + 10);
    await page.mouse.down();
    await page.mouse.move(header.x + 90, header.y + 45, { steps: 12 });
    await page.mouse.up();
    const moved = await eventually(() => api("GET", "/api/message-canvas"), result => { const point = result.layout.nodes.find(node => node.id === sourceAnswer.id); return point && (Math.abs(point.x - previousPoint.x) > 1 || Math.abs(point.y - previousPoint.y) > 1); }, "actual message drag persisted");
    const movedPoint = moved.layout.nodes.find(node => node.id === sourceAnswer.id);
    const allMessageIds = new Set(overview.conversations.flatMap(conversation => conversation.nodes.map(node => node.id)));
    assert.ok(moved.layout.nodes.every(node => allMessageIds.has(node.id)));
    await page.reload();
    await page.locator(`[data-id="${sourceAnswer.id}"]`).waitFor();
    const transform = await page.locator(`[data-id="${sourceAnswer.id}"]`).evaluate(element => element.style.transform);
    const position = /translate\(([-\d.]+)px,\s*([-\d.]+)px\)/.exec(transform);
    assert.ok(position, transform);
    assert.ok(Math.abs(Number(position[1]) - movedPoint.x) < 0.1);
    assert.ok(Math.abs(Number(position[2]) - movedPoint.y) < 0.1);
    assert.equal(await page.locator('.conversation-canvas-group, .react-flow__node-conversationGroup').count(), 0);
    facts.checks.push("absolute_message_drag_coordinates_survive_reload");
    facts.conversations = { rootId, branchId: branch.id, independentId, sourceAnswerId: sourceAnswer.id };

    await navigate(`/chat/${branch.id}`);
    await page.getByRole("combobox", { name: "对话路径", exact: true }).waitFor();
    assert.equal(await page.getByRole("combobox", { name: "对话路径", exact: true }).locator("option").count(), 2);
    await eventually(() => page.locator(".conversation-row").count(), count => count === 2, "sidebar lists two families rather than three execution conversations");
    assert.ok(!(await page.locator(".conversation-message-user").allTextContents()).some(value => value.includes(LATER_QUESTION)));
    await page.getByRole("combobox", { name: "对话路径", exact: true }).selectOption(rootId);
    await page.waitForURL(`**/chat/${rootId}`);
    await page.getByText("R7 原路径后续回答", { exact: true }).waitFor();
    await screenshot("family-path-selector");
    facts.checks.push("sidebar_groups_real_alternatives_into_one_family", "path_selector_opens_correct_full_history");

    await navigate("/settings");
    await eventually(() => page.locator(".settings-canvas-stage [data-settings-group]").count(), count => count === 6, "six collapsed category nodes");
    assert.equal(await page.locator('.settings-canvas-stage .react-flow__node[data-id^="setting:"]').count(), 0);
    const category = page.getByRole("button", { name: "编辑模型与服务", exact: true });
    assert.equal(await category.getAttribute("aria-expanded"), "false");
    await category.click();
    await eventually(() => api("GET", "/api/settings/canvas"), result => result.state.expanded_group_ids.includes("models"), "inline category expansion persisted");
    await page.getByRole("button", { name: "编辑对话模型", exact: true }).waitFor();
    assert.equal(await page.locator('.settings-canvas-stage .react-flow__node[data-id^="setting:"]').count(), 5);
    assert.equal(await page.getByRole("dialog").count(), 0);
    await eventually(() => page.locator('[data-id="derived:setting-child:model"] .react-flow__edge-path').getAttribute("d"), value => value?.includes("C"), "inline settings child curve");
    const focusedSelectors='.react-flow__node[data-id="setting-group:models"],.react-flow__node[data-id^="setting:"]';
    const focusFacts=await eventually(()=>page.locator(".settings-canvas-stage").evaluate((stage,selector)=>{const boxes=[...stage.querySelectorAll(selector)].map(node=>node.getBoundingClientRect());const area=stage.getBoundingClientRect();return {count:boxes.length,x:(Math.min(...boxes.map(box=>box.left))+Math.max(...boxes.map(box=>box.right)))/2-(area.left+area.width/2),y:(Math.min(...boxes.map(box=>box.top))+Math.max(...boxes.map(box=>box.bottom)))/2-(area.top+area.height/2)};},focusedSelectors),value=>value.count===6&&Math.abs(value.x)<35&&Math.abs(value.y)<35,"expanded setting branch is recentered");
    facts.settingsFocus=focusFacts;facts.checks.push("expanded_settings_camera_centers_selected_category_and_children");
    await screenshot("settings-inline-expanded");
    await page.getByRole("button", { name: "编辑对话模型", exact: true }).click();
    const modelDialog = page.getByRole("dialog", { name: "对话模型与模型池", exact: true });
    await modelDialog.waitFor();
    assert.equal(await page.getByRole("dialog").count(), 1);
    await modelDialog.getByRole("button", { name: "关闭对话模型与模型池", exact: true }).click();
    assert.equal(await page.getByRole("dialog").count(), 0);
    await page.reload();
    await page.getByRole("button", { name: "编辑对话模型", exact: true }).waitFor();
    assert.equal(await page.getByRole("button", { name: "编辑模型与服务", exact: true }).getAttribute("aria-expanded"), "true");
    await page.getByRole("button", { name: "编辑模型与服务", exact: true }).click();
    await eventually(() => api("GET", "/api/settings/canvas"), result => !result.state.expanded_group_ids.includes("models"), "inline collapse persisted");
    await page.reload();
    await page.getByRole("button", { name: "编辑模型与服务", exact: true }).waitFor();
    assert.equal(await page.locator('.settings-canvas-stage .react-flow__node[data-id^="setting:"]').count(), 0);
    await page.getByRole("button",{name:"整理布局",exact:true}).click();
    await eventually(()=>page.locator('[data-id="setting-group:models"]').evaluate(node=>node.style.transform),value=>value.includes("660px")&&value.includes("126px"),"explicit layout reset uses balanced default coordinates");
    await page.getByRole("button",{name:"居中",exact:true}).click();
    facts.checks.push("explicit_relayout_and_center_controls_keep_default_geometry");
    await screenshot("settings-inline-collapsed");
    facts.checks.push("settings_categories_expand_actual_leaf_nodes_without_parent_modal", "settings_leaf_opens_existing_form", "settings_expand_and_collapse_survive_reload");

    const knowledgeName = "R7-知识全文.md";
    const tail = KNOWLEDGE_TAIL;
    const documentText = `# R7 知识资料\n${"这是一段实际导入的知识正文，用于验证完整文档分块与检索。\n".repeat(700)}${tail}\n`;
    assert.ok(documentText.length > 12_000);
    await navigate("/knowledge");
    await page.getByLabel("选择知识文档", { exact: true }).setInputFiles({ name: knowledgeName, mimeType: "text/markdown", buffer: Buffer.from(documentText, "utf8") });
    const knowledge = await eventually(() => api("GET", "/api/knowledge/documents"), result => result.documents.some(document => document.name === knowledgeName), "actual knowledge import");
    const document = knowledge.documents.find(entry => entry.name === knowledgeName);
    assert.equal(document.char_count, [...documentText].length);
    assert.ok(document.chunk_count > 1);
    const detail = await api("GET", `/api/knowledge/documents/${document.id}`);
    assert.equal(detail.text, documentText);
    assert.ok(detail.chunks.some(chunk => chunk.content.includes(tail)));
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
    await page.getByRole("dialog", { name: "移出知识库", exact: true }).getByRole("button", { name: "移除", exact: true }).click();
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
    const hostToggle=page.getByRole("checkbox",{name:"显示宿主专用技能",exact:true});assert.equal(await hostToggle.isChecked(),false);
    await page.getByRole("textbox",{name:"搜索技能",exact:true}).fill(HOST_SKILL);assert.equal(await page.getByRole("option").filter({hasText:HOST_SKILL}).count(),0);
    await hostToggle.check();await page.getByRole("option").filter({hasText:HOST_SKILL}).click();await page.getByText("需要外部宿主能力",{exact:true}).waitFor();
    await screenshot("host-skill-dependencies");await hostToggle.uncheck();
    facts.checks.push("host_only_skills_are_hidden_by_default_and_show_actual_missing_tools");
    for (const fixture of skillFixtures) {
      const skill = skills.find(entry => entry.source_id === fixture.sourceId && entry.display_name === fixture.name);
      assert.equal(skill.editable, false);
      assert.equal(resolve(skill.path), resolve(fixture.path));
      const loaded = await api("GET", `/api/skills/${encodeURIComponent(skill.name)}`);
      assert.equal(loaded.content, fixture.content);
      if (fixture.sourceId === "workspace-codex") { skillTarget = skill.name; skillRoot = loaded.root_dir; }
      await page.getByRole("textbox", { name: "搜索技能", exact: true }).fill(fixture.name);
      await page.getByRole("option").filter({ hasText: fixture.name }).click();
      await page.getByRole("button", { name: "更多 · 查看全文", exact: true }).click();
      const full = page.getByRole("dialog", { name: "技能全文", exact: true });
      await eventually(() => full.locator("pre").textContent(), value => value === fixture.content, "actual discovered skill full text");
      await screenshot(`skill-${fixture.directory.slice(1)}-full-text`);
      await full.getByRole("button", { name: "关闭技能全文", exact: true }).click();
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
    const toolDialog=page.getByRole("dialog",{name:"工具管理",exact:true});await toolDialog.getByLabel("工具列表",{exact:true}).waitFor();
    assert.equal(await toolDialog.getByRole("combobox").count(),0);assert.equal(await toolDialog.locator(".capability-split-layout,.capability-detail-panel").count(),0);
    assert.equal(await toolDialog.getByText("新增能力来源",{exact:true}).count(),0);assert.equal(await toolDialog.getByText("能力概览",{exact:true}).count(),0);
    await toolDialog.getByRole("textbox",{name:"搜索工具",exact:true}).fill("write_file");
    await toolDialog.locator(".tool-manager-row").filter({hasText:"写入文件"}).click();await toolDialog.getByLabel("工具详情",{exact:true}).waitFor();
    await screenshot("compact-tool-detail");await toolDialog.getByRole("button",{name:"返回工具列表",exact:true}).click();await toolDialog.getByRole("textbox",{name:"搜索工具",exact:true}).fill("");await screenshot("compact-tool-list");
    facts.checks.push("tools_use_dedicated_compact_list_and_single_item_detail");
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
