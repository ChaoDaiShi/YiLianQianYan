import assert from "node:assert/strict";
import { createServer } from "node:http";
import { mkdir, readFile, writeFile, access } from "node:fs/promises";
import { join, relative, resolve } from "node:path";
import { execFileSync } from "node:child_process";

const ROOT_QUESTION = "R6 根对话你好";
const ROOT_ANSWER = "R6 真实问候回答";
const WORK_QUESTION = "R6 请执行实际文件写入";
const WORK_ANSWER = "R6 文件已经实际写入。";
const TOOL_REASONING = "R6_TOOL_REASONING：读取用户目标后，调用真实文件工具保存内容，再根据工具结果回复。";
const LATER_QUESTION = "R6 原路径后续问题";
const BRANCH_QUESTION = "R6 从问候回答展开另一条路径";
const INDEPENDENT_QUESTION = "R6 独立问题";
const FILE_CONTENT = "R6_ACTUAL_FILE_CONTENT";
const KNOWLEDGE_TAIL = "R6_KNOWLEDGE_TAIL_MARKER";
const KNOWLEDGE_QUESTION = `请查询 ${KNOWLEDGE_TAIL}`;
const SKILL_QUESTION = "R6 通过真实工具加载 Codex 技能";

/** Actual app/API/Git/filesystem flow with a local model protocol fixture. */
export async function runWorkbenchRevision6({ page, frontendPort, backendPort, controlToken, evidenceDir, workspace }) {
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
    const path = join(evidenceDir, `revision6-${name}.png`);
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
  await writeFile(join(workspacePath, "revision6-result.txt"), "Before work\n");
  await writeFile(join(workspacePath, "staged.txt"), "Before staged change\n");
  git("add", "--", "revision6-result.txt", "staged.txt");
  git("-c", "user.name=Workbench QA", "-c", "user.email=qa@local.invalid", "commit", "-m", "Create isolated R6 fixture");
  await writeFile(join(workspacePath, "staged.txt"), "Prepared staged change\n");
  git("add", "--", "staged.txt");
  const indexBefore = await readFile(join(workspacePath, ".git", "index"));

  let sequence = 0;
  let skillTarget;
  let skillRoot;
  const provider = createServer(async (request, response) => {
    let raw = "";
    for await (const chunk of request) raw += chunk;
    const body = JSON.parse(raw);
    if (!body.stream) {
      response.writeHead(200, { "Content-Type": "application/json" });
      response.end(JSON.stringify({ id: "r6-verify", choices: [{ index: 0, message: { role: "assistant", content: "pong" }, finish_reason: "stop" }] }));
      return;
    }
    const lastUser = body.messages.findLastIndex(message => message.role === "user");
    const question = body.messages[lastUser]?.content ?? "";
    const hasTool = body.messages.slice(lastUser + 1).some(message => message.role === "tool");
    const skillRequest = question === SKILL_QUESTION;
    const tool = (question === WORK_QUESTION || skillRequest) && !hasTool;
    const actualToolMessages = body.messages.slice(lastUser + 1).filter(message => message.role === "tool" && typeof message.content === "string");
    facts.requests.push({ question, model: body.model, hasTool, userHistory: body.messages.filter(message => message.role === "user").map(message => message.content), reasoningRoundTrip: body.messages.some(message => message.reasoning_content?.includes("R6_TOOL_REASONING")), knowledgeGrounded: body.messages.some(message => message.role === "system" && typeof message.content === "string" && message.content.includes(KNOWLEDGE_TAIL)), skillToolHasMarker: actualToolMessages.some(message => message.content.includes("R6_CODEX_SKILL_FULL_TAIL")), skillToolHasRoot: Boolean(skillRoot) && actualToolMessages.some(message => message.content.includes(skillRoot)) });
    if (hasTool) await new Promise(resolveWait => setTimeout(resolveWait, 1600));
    const delta = tool ? {
      reasoning_content: skillRequest ? "调用已发现技能的实际读取工具。" : TOOL_REASONING,
      content: skillRequest ? "正在读取实际技能文件。" : "正在调用实际文件工具。",
      tool_calls: [{ index: 0, id: `r6-tool-${++sequence}`, type: "function", function: { name: skillRequest ? "load_skill" : "write_file", arguments: JSON.stringify(skillRequest ? { name: skillTarget } : { path: "revision6-result.txt", content: FILE_CONTENT }) } }],
    } : { content: question === ROOT_QUESTION ? ROOT_ANSWER : question === WORK_QUESTION ? WORK_ANSWER : skillRequest ? "R6 技能工具读取结束。" : question === LATER_QUESTION ? "R6 原路径后续回答" : question === INDEPENDENT_QUESTION ? "R6 独立回答" : `R6 分支回答：${question}` };
    response.writeHead(200, { "Content-Type": "text/event-stream" });
    response.end(`data: ${JSON.stringify({ choices: [{ index: 0, delta, finish_reason: null }] })}\n\ndata: ${JSON.stringify({ choices: [{ index: 0, delta: {}, finish_reason: tool ? "tool_calls" : "stop" }], usage: { prompt_tokens: 25, completion_tokens: 15, total_tokens: 40 } })}\n\ndata: [DONE]\n\n`);
  });
  await new Promise(resolveListen => provider.listen(0, "127.0.0.1", resolveListen));

  try {
    const model = await api("POST", "/api/llm/models", { provider: "deepseek", label: "R6 协议验收模型", model: "deepseek-chat", base_url: `http://127.0.0.1:${provider.address().port}/v1`, api_format: "openai", api_key: "controlled-test-credential", temperature: 0, max_tokens: 2048, invoke_timeout_ms: 15_000 });
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
    assert.equal(await readFile(join(workspacePath, "revision6-result.txt"), "utf8"), FILE_CONTENT);
    assert.equal(rootAfterWork.workspace_mode, "work");
    const actualThoughts = [...Object.values(rootAfterWork.message_reasoning ?? {}), ...Object.values(rootAfterWork.turn_reasoning ?? {})].join("\n");
    assert.ok(actualThoughts.includes("R6_TOOL_REASONING"));
    const answer = page.locator(".conversation-message-assistant").filter({ hasText: WORK_ANSWER });
    const process = answer.locator('.message-process[aria-label="本轮处理过程"]');
    await process.locator("summary").first().click();
    await process.locator(".message-reasoning").filter({ hasText: "R6_TOOL_REASONING" }).waitFor();
    await process.locator('.conversation-tool-card[data-status="success"]').waitFor();
    assert.equal(await page.locator('.conversation-assistant-avatar, [class*="conversation-assistant-avatar"]').count(), 0);
    assert.equal(await answer.locator("img").count(), 0);
    await screenshot("actual-process-after-refresh");
    facts.checks.push("real_tool_file_and_provider_reasoning_survive_refresh", "assistant_has_no_avatar");

    const executionToggle = page.getByRole("button", { name: "执行与审批", exact: true });
    const gitToggle = page.getByRole("button", { name: "Git 文件变更", exact: true });
    assert.equal(await page.locator(".glass-header").getByRole("button", { name: "执行与审批", exact: true }).count(), 1);
    assert.equal(await page.locator(".glass-header").getByRole("button", { name: "Git 文件变更", exact: true }).count(), 1);
    if (await gitToggle.getAttribute("aria-pressed") !== "true") await gitToggle.click();
    await page.getByRole("complementary", { name: "Git 文件变更", exact: true }).waitFor();
    await page.getByRole("button", { name: "查看工作区差异 revision6-result.txt", exact: true }).click();
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
    assert.ok(await page.locator('.message-branch-node[data-role="process"]').count() >= 2);
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
    facts.checks.push("message_forest_contains_only_actual_message_nodes_without_backdrop", "alternative_path_uses_actual_ancestor_and_excludes_later_siblings", "independent_conversation_tree_has_no_cross_family_edge");

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
    await page.getByText("R6 原路径后续回答", { exact: true }).waitFor();
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
    await screenshot("settings-inline-collapsed");
    facts.checks.push("settings_categories_expand_actual_leaf_nodes_without_parent_modal", "settings_leaf_opens_existing_form", "settings_expand_and_collapse_survive_reload");

    const knowledgeName = "R6-知识全文.md";
    const tail = KNOWLEDGE_TAIL;
    const documentText = `# R6 知识资料\n${"这是一段实际导入的知识正文，用于验证完整文档分块与检索。\n".repeat(700)}${tail}\n`;
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
      { directory: ".codex", name: "revision6-codex-real", sourceId: "workspace-codex", marker: "R6_CODEX_SKILL_FULL_TAIL" },
      { directory: ".claude", name: "revision6-claude-real", sourceId: "workspace-claude", marker: "R6_CLAUDE_SKILL_FULL_TAIL" },
    ];
    for (const fixture of skillFixtures) {
      const directory = join(workspacePath, fixture.directory, "skills", fixture.name);
      await mkdir(directory, { recursive: true });
      fixture.content = `---\nname: ${fixture.name}\ndescription: R6 actual workspace discovery fixture\n---\n\n${"Read the real workspace fixture and report its content.\n".repeat(35)}${fixture.marker}\n`;
      fixture.path = join(directory, "SKILL.md");
      await writeFile(fixture.path, fixture.content);
    }
    await navigate("/capabilities?category=skill");
    await page.getByRole("button", { name: "刷新发现", exact: true }).click();
    const skills = await eventually(() => api("GET", "/api/skills"), entries => skillFixtures.every(fixture => entries.some(skill => skill.source_id === fixture.sourceId && skill.display_name === fixture.name)), "actual Codex and Claude workspace skill discovery");
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
    await page.getByText("R6 技能工具读取结束。", { exact: true }).waitFor();
    await screenshot("actual-discovered-skill-tool");
    facts.checks.push("actual_load_skill_returns_discovered_file_tail_and_root_to_provider");
    facts.skills = skillFixtures.map(({ name, sourceId, path }) => ({ name, sourceId, path }));
    facts.status = "PASS";
    return facts;
  } catch (error) {
    facts.status = "FAIL";
    facts.error = error.stack || String(error);
    throw error;
  } finally {
    await writeFile(join(evidenceDir, "revision6-facts.json"), JSON.stringify(facts, null, 2));
    await new Promise(resolveClose => provider.close(resolveClose));
  }
}

export const workbenchRevision6 = runWorkbenchRevision6;
