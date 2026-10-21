import assert from "node:assert/strict";
import { createServer } from "node:http";
import { writeFile } from "node:fs/promises";
import { join } from "node:path";

export async function settingsRevision3({ page, frontendPort, backendPort, controlToken, evidenceDir }) {
  const origin = `http://127.0.0.1:${frontendPort}`;
  const base = `http://127.0.0.1:${backendPort}`;
  const requests = [];
  const facts = { themes: [], settingsWrites: [], deleted: [] };
  const headers = { "Content-Type": "application/json", "X-Yilian-Control-Session": controlToken };
  async function api(method, path, body) {
    const response = await fetch(base + path, { method, headers, body: body === undefined ? undefined : JSON.stringify(body) });
    const text = await response.text();
    assert.ok(response.ok, `${method} ${path}: ${response.status} ${text.slice(0, 400)}`);
    return text ? JSON.parse(text) : null;
  }
  async function eventually(read, valid, label) {
    const deadline = Date.now() + 15000;
    let value;
    do {
      value = await read();
      if (valid(value)) return value;
      await new Promise(resolve => setTimeout(resolve, 60));
    } while (Date.now() < deadline);
    assert.fail(`${label}: ${JSON.stringify(value)}`);
  }
  const screenshot = name => page.screenshot({ path: join(evidenceDir, `revision3-${name}.png`) });
  const canvasPath = id => `/api/settings/canvas${id ? `?canvas_id=${encodeURIComponent(id)}` : ""}`;
  const settingNode = name => page.locator(".settings-canvas-stage").getByRole("button", { name: `编辑${name}`, exact: true });
  const manager = () => page.getByRole("complementary", { name: "节点管理器" });
  const closeEditor = async () => {
    await page.getByRole("dialog").getByRole("button", { name: "关闭", exact: true }).click();
    await page.getByRole("dialog").waitFor({ state: "detached" });
  };
  async function changeConnection(label, id, connect, scope = "") {
    const response = page.waitForResponse(result => {
      const url = new URL(result.url());
      return result.request().method() === "PUT" && url.pathname === "/api/settings/canvas" && (url.searchParams.get("canvas_id") ?? "") === scope;
    });
    if (connect) await manager().getByRole("button", { name: `连接${label}`, exact: true }).click();
    else await manager().locator("article").filter({ has: page.locator("strong", { hasText: new RegExp(`^${label}$`) }) }).getByRole("button", { name: "断开", exact: true }).click();
    const saved = await response;
    assert.equal(saved.status(), 200, `${label} connection persistence`);
    facts.settingsWrites.push(await saved.json());
    await eventually(() => api("GET", canvasPath(scope)), result => result.state.connected_node_ids.includes(id) === connect, `${id} persisted connection`);
    await settingNode(label).waitFor({ state: connect ? "visible" : "detached" });
  }
  const provider = createServer(async (req, res) => {
    let raw = "";
    for await (const chunk of req) raw += chunk;
    const body = JSON.parse(raw);
    requests.push(body);
    const question = body.messages.filter(message => message.role === "user").at(-1)?.content;
    const answer = question === "你好" ? "你好" : question === "告诉我你的名字" ? "我叫小涟" : question === "你认为什么是善" ? "善的意思是关心他人，并为自己的行为负责。" : "请继续提问。";
    res.writeHead(200, { "Content-Type": "text/event-stream" });
    res.end(`data: ${JSON.stringify({ choices: [{ index: 0, delta: { content: answer }, finish_reason: null }] })}\n\ndata: ${JSON.stringify({ choices: [{ index: 0, delta: {}, finish_reason: "stop" }], usage: { prompt_tokens: 12, completion_tokens: 7, total_tokens: 19 } })}\n\ndata: [DONE]\n\n`);
  });
  await new Promise(resolve => provider.listen(0, "127.0.0.1", resolve));
  try {
    const agent = await api("POST", "/api/agents", { name: "小涟", user_prompt: "使用中文回答问题。", allowed_tools: [], capabilities: [] });
    assert.equal(agent.source, "database");
    const root = await api("POST", "/api/conversations", { title: "问答分支" });
    const a = await api("POST", "/api/connectors", { name: "问答分支画布", agent_id: agent.id });
    const b = await api("POST", "/api/connectors", { name: "待删除画布", agent_id: agent.id });
    const c = await api("POST", "/api/connectors", { name: "最后的画布", agent_id: agent.id });
    await api("POST", `/api/connectors/${a.canvas_id}/bindings`, { kind: "conversation_agent", source_id: `conversation:${root.id}`, target_id: `agent:${agent.id}` });
    const model = await api("POST", "/api/llm/models", { provider: "local", label: "验收对话服务", model: "branch-provider", base_url: `http://127.0.0.1:${provider.address().port}/v1`, api_format: "openai", api_key: "controlled-test-credential", temperature: 0, max_tokens: 256, invoke_timeout_ms: 10000 });
    await api("POST", `/api/connectors/${a.canvas_id}/bindings`, { kind: "agent_model", source_id: `agent:${agent.id}`, target_id: `model:${model.id}`, model_source: "profile", service_kind: "chat" });
    const resourceIds = [`conversation:${root.id}`, `model:${model.id}`];
    for (const category of ["preference", "knowledge"]) {
      const memory = await api("POST", "/api/memories", { content: category === "knowledge" ? "工作区知识" : "偏好中文交流", category });
      const kind = category === "knowledge" ? "knowledge" : "memory";
      resourceIds.push(`${kind}:${memory.id}`);
      await api("POST", `/api/connectors/${a.canvas_id}/bindings`, { kind: `agent_${kind}`, source_id: `agent:${agent.id}`, target_id: `${kind}:${memory.id}` });
    }

    await page.goto(origin + "/connectors");
    await page.getByRole("combobox", { name: "当前连接器画布" }).selectOption(a.canvas_id);
    await page.getByRole("combobox", { name: "消息画布对话目录" }).waitFor();
    await page.getByRole("combobox", { name: "消息画布对话目录" }).selectOption(root.id);
    const dimensions = await page.locator(".message-branch-workspace").evaluate(pane => ({ width: pane.getBoundingClientRect().width, content: pane.closest("main").getBoundingClientRect().width }));
    assert.ok(dimensions.width >= dimensions.content - 2, "message canvas consumes the entire content width");
    assert.equal(await page.locator(".connector-resource-pane, .connector-split-workspace, .settings-canvas-stage").count(), 0);
    assert.equal(await page.getByRole("button", { name: "添加资源绑定", exact: true }).count(), 0);
    assert.equal(await page.getByRole("button", { name: "显示与布局", exact: true }).count(), 0);
    const composerBackground = await page.locator(".message-branch-composer textarea").evaluate(node => getComputedStyle(node).backgroundColor);
    assert.equal(composerBackground, "rgba(0, 0, 0, 0)", "message composer textarea has a transparent background");
    facts.connectorWidth = dimensions;
    facts.messageComposerBackground = composerBackground;
    const graphPath = `/api/connectors/${a.canvas_id}/messages/${root.id}`;
    async function turn(question, count) {
      await page.getByRole("textbox", { name: "消息画布提问" }).fill(question);
      await page.getByRole("button", { name: "发送提问", exact: true }).click();
      await page.waitForFunction(expected => document.querySelectorAll(".message-branch-stage .react-flow__node").length === expected && !document.querySelector('.message-branch-stage [data-id^="pending-"]'), count);
      await page.getByRole("button", { name: "发送提问", exact: true }).waitFor();
      return api("GET", graphPath);
    }
    await turn("你好", 2);
    const first = (await api("GET", graphPath)).nodes.find(node => node.role === "assistant");
    await turn("告诉我你的名字", 4);
    await page.locator(".message-branch-stage .react-flow__controls-fitview").click();
    await page.locator(`.message-branch-stage .react-flow__node[data-id="${first.id}"]`).getByRole("button", { name: "从此回答继续提问", exact: true }).click();
    const graph = await turn("你认为什么是善", 6);
    const siblings = graph.nodes.filter(node => node.parent_id === first.id);
    assert.equal(siblings.length, 2);
    assert.equal(requests.length, 3);
    assert.deepEqual(requests[1].messages.filter(message => message.role === "user").map(message => message.content), ["你好", "告诉我你的名字"]);
    assert.deepEqual(requests[2].messages.filter(message => message.role === "user").map(message => message.content), ["你好", "你认为什么是善"]);
    assert.equal((await api("GET", `/api/conversations/${root.id}`)).messages.length, 0);
    assert.equal((await api("GET", `/api/connectors/${a.canvas_id}/message-sources`)).conversations.length, 1);
    await page.locator(".message-branch-stage .react-flow__controls-fitview").click();
    await eventually(() => page.locator(".message-branch-stage").evaluate(stage => {
      const box = stage.getBoundingClientRect();
      return Array.from(stage.querySelectorAll(".react-flow__node")).every(node => {
        const n = node.getBoundingClientRect();
        return n.x >= box.x && n.y >= box.y && n.right <= box.right && n.bottom <= box.bottom;
      });
    }), value => value, "fit view includes all six nodes");
    const points = await page.locator(".message-branch-stage .react-flow__node").evaluateAll(nodes => nodes.map(node => ({ id: node.dataset.id, x: node.getBoundingClientRect().x, y: node.getBoundingClientRect().y })));
    const parentPoint = points.find(point => point.id === first.id);
    const children = siblings.map(node => points.find(point => point.id === node.id));
    assert.ok(children.every(point => point.y > parentPoint.y));
    assert.ok(Math.abs(children[0].x - children[1].x) > 50);
    await screenshot("connectors");
    assert.equal((await api("GET", graphPath)).selected_node_id, first.id);
    await page.reload();
    await page.waitForFunction(() => document.querySelectorAll(".message-branch-stage .react-flow__node").length === 6);
    assert.equal((await api("GET", graphPath)).nodes.length, 6);

    await page.goto(origin + "/settings");
    await settingNode("对话模型").waitFor();
    assert.equal(await page.getByRole("button", { name: "展开节点管理器", exact: true }).getAttribute("aria-expanded"), "false");
    assert.equal(Math.round((await manager().boundingBox()).width), 48);
    for (const label of ["对话模型", "权限审批", "界面外观"]) await settingNode(label).waitFor();
    assert.equal(await page.locator(".message-branch-workspace").count(), 0);
    const initial = await api("GET", canvasPath(""));
    assert.deepEqual(initial.state.connected_node_ids.slice().sort(), ["appearance", "model", "permissions"]);
    assert.deepEqual(initial.presets.map(preset => preset.id).sort(), ["model", "stt", "tts", "search", "vision", "image", "embedding", "agent", "compaction", "permissions", "sandbox", "extensions", "appearance", "navigation"].sort());
    await screenshot("settings-collapsed");
    await page.getByRole("button", { name: "展开节点管理器", exact: true }).click();
    assert.equal(await manager().locator("section").nth(0).locator("article").count(), 3);
    assert.equal(await manager().locator("section").nth(1).locator("article").count(), 11);
    await changeConnection("语音识别", "stt", true);
    const beforeSettings = await api("GET", "/api/settings");
    await settingNode("语音识别").click();
    assert.equal(await page.getByRole("dialog").count(), 1);
    const stt = page.getByRole("dialog");
    await eventually(() => stt.getByLabel("模型", { exact: true }).inputValue(), value => value === beforeSettings.voice.stt.model, "STT draft loaded from settings API");
    await stt.getByLabel("API 地址", { exact: true }).waitFor();
    await stt.getByLabel("模型", { exact: true }).fill("revision3-stt");
    const saving = page.waitForResponse(response => response.request().method() === "PUT" && new URL(response.url()).pathname === "/api/settings");
    await stt.getByRole("button", { name: "保存设置", exact: true }).click();
    assert.equal((await saving).status(), 200);
    await eventually(() => api("GET", "/api/settings"), value => value.voice.stt.model === "revision3-stt", "STT model saved");
    await screenshot("settings-stt-dialog");
    await closeEditor();
    await settingNode("语音识别").click();
    await eventually(() => page.getByRole("dialog").getByLabel("模型", { exact: true }).inputValue(), value => value === "revision3-stt", "STT model reopens with saved value");
    await closeEditor();
    await changeConnection("语音识别", "stt", false);
    assert.equal((await api("GET", "/api/settings")).voice.stt.model, "revision3-stt", "disconnect only changes presentation");

    // Nested model-account forms must keep keyboard focus in the topmost modal.
    await settingNode("对话模型").click();
    await page.getByRole("dialog").getByRole("button", { name: "添加厂商账户", exact: true }).waitFor();
    await page.getByRole("dialog").getByRole("button", { name: "添加厂商账户", exact: true }).click();
    assert.equal(await page.getByRole("dialog").count(), 2);
    const accountDialog = page.getByRole("dialog", { name: "添加厂商账户", exact: true });
    await accountDialog.getByRole("textbox", { name: "账户别名", exact: true }).focus();
    await page.keyboard.press("Tab");
    assert.equal(await page.evaluate(() => document.activeElement?.getAttribute("aria-label")), "厂商预设", "Tab advances inside the account dialog");
    await screenshot("settings-nested-account-dialog");
    await page.keyboard.press("Escape");
    await accountDialog.waitFor({ state: "detached" });
    assert.equal(await page.getByRole("dialog").count(), 1, "Escape closes only the topmost dialog");
    await closeEditor();

    await page.getByRole("combobox", { name: "设置配置对象" }).selectOption(a.canvas_id);
    await page.locator(`.settings-canvas-stage .react-flow__node[data-id="agent:${agent.id}"]`).waitFor();
    for (const id of resourceIds) await page.locator(`.settings-canvas-stage .react-flow__node[data-id="${id}"]`).waitFor();
    const ports = await page.locator(".settings-canvas-stage .connector-node.agent .react-flow__handle").evaluateAll(nodes => nodes.map(node => node.dataset.handlepos).sort());
    assert.deepEqual(ports, ["bottom", "left", "right", "top"]);
    const curves = await page.locator(".settings-canvas-stage .react-flow__edge-path").evaluateAll(nodes => nodes.map(node => node.getAttribute("d")));
    assert.ok(curves.length >= 4 && curves.every(path => path.includes("C")), "resource edges use actual Bezier paths in settings");
    await changeConnection("语音识别", "stt", true, a.canvas_id);
    await page.locator(".settings-canvas-stage .react-flow__controls-fitview").click();
    await page.waitForTimeout(250);
    await screenshot("settings-agent-resources");
    assert.equal((await api("GET", canvasPath(""))).state.connected_node_ids.includes("stt"), false);
    await page.getByRole("combobox", { name: "设置配置对象" }).selectOption("");
    await settingNode("对话模型").waitFor();
    await settingNode("语音识别").waitFor({ state: "detached" });
    assert.equal((await api("GET", canvasPath(a.canvas_id))).state.connected_node_ids.includes("stt"), true);
    await screenshot("settings-manager");

    for (const [mode, label] of [["light", "白天"], ["dark", "夜间"]]) {
      await page.goto(origin + "/settings?section=appearance");
      const radio = page.getByRole("radio", { name: new RegExp(label) });
      await radio.click();
      assert.equal(await radio.getAttribute("aria-checked"), "true");
      await screenshot(`settings-${mode}-dialog`);
      await closeEditor();
      await settingNode("对话模型").waitFor();
      await page.getByRole("button", { name: "展开节点管理器", exact: true }).click();
      await screenshot(`settings-${mode}-manager`);
      await page.goto(origin + `/chat/${graph.nodes.find(node => node.role === "assistant").conversation_id}`);
      await page.locator(".conversation-composer").waitFor();
      const expand = page.getByRole("button", { name: "展开执行轨迹", exact: true });
      if (await expand.count()) await expand.click();
      const themeFacts = await page.evaluate(() => [".workbench-grid", ".conversation-composer", ".composer-card", ".context-rail", ".execution-region", ".execution-sidebar"].flatMap(selector => Array.from(document.querySelectorAll(selector)).map(node => ({ selector, color: getComputedStyle(node).backgroundColor, image: getComputedStyle(node).backgroundImage }))));
      assert.ok(themeFacts.length >= 4);
      for (const fact of themeFacts) {
        assert.equal(fact.color, "rgba(0, 0, 0, 0)", `${mode} ${fact.selector}`);
        assert.equal(fact.image, "none", `${mode} ${fact.selector} background image`);
      }
      facts.themes.push({ mode, facts: themeFacts });
      await screenshot(`chat-${mode}`);
    }

    // Use an actual application action, which also works in the native webview.
    await page.goto(origin + "/agents");
    await page.getByRole("button", { name: "新建 Agent", exact: true }).first().click();
    await page.getByRole("dialog").getByPlaceholder("例如：research-assistant").fill("顶部通知验收");
    await page.getByRole("dialog").getByRole("button", { name: "创建", exact: true }).click();
    const toast = page.getByRole("region", { name: "系统轻通知" });
    await toast.waitFor();
    assert.ok((await toast.boundingBox()).y < 40);
    assert.ok((await api("GET", "/api/agents")).agents.some(item => item.name === "顶部通知验收" && item.source === "database"));
    await screenshot("toast-top");
    await toast.getByRole("button", { name: "关闭提示", exact: true }).click();

    async function deleted(id) {
      assert.ok(!(await api("GET", "/api/connectors")).canvases.some(canvas => canvas.id === id));
      // Negative checks run in Node to avoid polluting browser console/network errors.
      const response = await fetch(base + `/api/connectors/${id}`, { headers });
      assert.equal(response.status, 404);
      facts.deleted.push(id);
    }
    await page.goto(origin + "/connectors");
    await page.getByRole("combobox", { name: "当前连接器画布" }).selectOption(a.canvas_id);
    await page.getByRole("button", { name: "删除当前画布", exact: true }).click();
    await page.getByRole("button", { name: "确认删除", exact: true }).waitFor();
    assert.equal(await page.getByRole("dialog").count(), 1);
    await page.getByRole("button", { name: "取消", exact: true }).focus();
    await page.keyboard.press("Tab");
    assert.ok((await page.evaluate(() => document.activeElement?.textContent))?.startsWith("确认删除"));
    await page.keyboard.press("Enter");
    await page.getByRole("dialog").waitFor({ state: "detached" });
    await deleted(a.canvas_id);
    assert.equal(await page.getByRole("combobox", { name: "当前连接器画布" }).inputValue(), b.canvas_id);
    await page.getByRole("button", { name: "管理全部画布", exact: true }).click();
    const lastRow = page.getByRole("dialog").locator("article").filter({ hasText: "最后的画布" });
    await lastRow.getByRole("button", { name: "删除此画布", exact: true }).click();
    assert.equal(await page.getByRole("dialog").count(), 1);
    await page.getByRole("button", { name: "确认删除", exact: true }).click();
    await page.getByRole("heading", { name: "管理全部画布", exact: true }).waitFor();
    await lastRow.waitFor({ state: "detached" });
    await deleted(c.canvas_id);
    await page.getByRole("dialog").locator('button[aria-label^="关闭"]').click();
    assert.equal(await page.getByRole("combobox", { name: "当前连接器画布" }).inputValue(), b.canvas_id);
    await page.getByRole("button", { name: "删除当前画布", exact: true }).click();
    await page.getByRole("button", { name: "确认删除", exact: true }).click();
    await page.getByText("还没有连接器画布", { exact: true }).waitFor();
    await deleted(b.canvas_id);
    assert.equal((await api("GET", "/api/connectors")).canvases.length, 0);
    assert.equal(await page.getByRole("combobox", { name: "当前连接器画布" }).inputValue(), "");
    await screenshot("all-deleted");
    await writeFile(join(evidenceDir, "revision3-model-requests.json"), JSON.stringify(requests, null, 2));
    await writeFile(join(evidenceDir, "revision3-facts.json"), JSON.stringify(facts, null, 2));
    return { status: "passed", real_backend: true, controlled_provider: true, checks: { full_width_messages: true, no_connector_resource_pane: true, message_composer_transparent: true, message_nodes: true, vertical_chain: true, sibling_branches: true, ancestor_isolation: true, source_unchanged: true, fork_catalog: true, reopen: true, settings_graph: true, manager_collapsed: true, fourteen_presets: true, settings_connect_persistence: true, real_stt_editor: true, settings_disconnect_preserves_value: true, nested_keyboard: true, scope_isolation: true, bound_resources_preserved: true, four_ports: true, bezier: true, light_dark_transparent: true, toast_top: true, direct_current_delete: true, keyboard_confirmation: true, noncurrent_delete: true, last_delete: true }, evidence: evidenceDir };
  } finally {
    await new Promise(resolve => provider.close(resolve));
  }
}
