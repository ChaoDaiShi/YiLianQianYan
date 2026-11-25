// Windows acceptance with real SQLite, chat and workflow gateways and a local model fixture.
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { existsSync } from "node:fs";
import { mkdir, mkdtemp, writeFile } from "node:fs/promises";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { chromium } from "playwright-core";

const root = resolve(import.meta.dirname, "../..");
const isolated = await mkdtemp(join(tmpdir(), "yilian-harness-e2e-"));
const workspace = join(isolated, "workspace"), data = join(isolated, "data");
await Promise.all([mkdir(workspace), mkdir(data)]);
const input = join(workspace, "fixture.txt");
await writeFile(input, "HARNESS_LOCAL_READ_PROOF");
const token = "harness-e2e-controlled-session-000000000000";
const targets = new Map(), receipts = new Map(), logs = [], children = [];
let browser;

async function listen(server) {
  await new Promise(resolve => server.listen(0, "127.0.0.1", resolve));
  return server.address().port;
}
async function freePort() {
  const server = createServer(); const port = await listen(server);
  await new Promise(resolve => server.close(resolve)); return port;
}
const model = createServer(async (request, response) => {
  try {
    const chunks = []; for await (const chunk of request) chunks.push(chunk);
    const body = JSON.parse(Buffer.concat(chunks).toString());
    if (!body.stream) {
      response.writeHead(200, { "Content-Type": "application/json" });
      response.end(JSON.stringify({ choices: [{ message: { role: "assistant", content: "ok" }, finish_reason: "stop" }] }));
      return;
    }
    const index = body.messages.findLastIndex(message => message.role === "user");
    const marker = [...targets.keys()].find(key => String(body.messages[index]?.content).includes(key));
    const target = targets.get(marker);
    const toolMessages = body.messages.slice(index + 1).filter(message => message.role === "tool");
    if (marker && !receipts.has(marker)) receipts.set(marker, { names: body.tools.map(tool => tool.function.name), prompt: body.messages.filter(message => message.role === "system").map(message => message.content).join("\n") });
    if (marker && toolMessages.length) receipts.get(marker).result = String(toolMessages.at(-1).content);
    const call = target && !toolMessages.length;
    const delta = call ? { role: "assistant", tool_calls: [{ index: 0, id: `call-${marker}`, type: "function", function: { name: target.name, arguments: JSON.stringify(target.args) } }] }
      : { content: "本地配置验收结束。" };
    response.writeHead(200, { "Content-Type": "text/event-stream" });
    for (const event of [{ choices: [{ index: 0, delta, finish_reason: null }] }, { choices: [{ index: 0, delta: {}, finish_reason: call ? "tool_calls" : "stop" }], usage: { prompt_tokens: 20, completion_tokens: 10, total_tokens: 30 } }]) response.write(`data: ${JSON.stringify(event)}\n\n`);
    response.end("data: [DONE]\n\n");
  } catch (error) { if (!response.headersSent) response.writeHead(500); response.end(String(error)); }
});
const modelPort = await listen(model), backendPort = await freePort(), frontendPort = await freePort();
const base = `http://127.0.0.1:${backendPort}`, front = `http://127.0.0.1:${frontendPort}`;
const headers = { "Content-Type": "application/json", "X-Yilian-Control-Session": token };

function launch(command, args, env, cwd = root) {
  const child = spawn(command, args, { cwd, env: { ...process.env, ...env }, windowsHide: true, stdio: ["ignore", "pipe", "pipe"] });
  for (const stream of [child.stdout, child.stderr]) stream.on("data", data => logs.push(String(data)));
  children.push(child);
}
async function until(read, label, timeout = 35000) {
  const end = Date.now() + timeout; let failure;
  while (Date.now() < end) {
    try { const value = await read(); if (value) return value; } catch (error) { failure = error; }
    await new Promise(resolve => setTimeout(resolve, 120));
  }
  throw new Error(`${label}: ${failure ?? "timed out"}`);
}
async function api(method, path, body, status = 200) {
  const response = await fetch(base + path, { method, headers, body: body === undefined ? undefined : JSON.stringify(body), signal: AbortSignal.timeout(45000) });
  const text = await response.text();
  assert.equal(response.status, status, `${method} ${path}: ${text}`);
  return text && status < 400 ? JSON.parse(text) : text;
}
async function startChat(marker, ws, name, args) {
  targets.set(marker, { name, args });
  const chat = await api("POST", "/api/conversations", { title: marker });
  await api("PUT", `/api/conversations/${chat.id}/workspace`, { workspace_id: ws.id });
  const response = await fetch(base + "/api/chat", { method: "POST", headers, body: JSON.stringify({ conversation_id: chat.id, message: marker, workspace_mode: "work" }), signal: AbortSignal.timeout(45000) });
  const events = await response.text();
  assert.equal(response.status, 200, events);
  return { chat, events };
}

try {
  launch(join(root, "target/debug/yilian-server.exe"), [], { YILIAN_HOST: `127.0.0.1:${backendPort}`, YILIAN_WORKSPACE: workspace, YILIAN_DATA_DIR: data, YILIAN_ALLOWED_ORIGINS: front, YILIAN_CONTROL_SESSION_TOKEN: token, YILIAN_HARNESS_FIXTURE_KEY: "controlled-local-value" });
  launch(process.execPath, [join(root, "frontend/node_modules/vite/bin/vite.js"), "--host", "127.0.0.1", "--port", String(frontendPort), "--strictPort"], { VITE_API_BASE: base, VITE_CONTROL_SESSION_TOKEN: token }, join(root, "frontend"));
  await Promise.all([until(async () => (await fetch(base + "/api/health")).ok, "backend ready"), until(async () => (await fetch(front)).ok, "frontend ready")]);
  const settings = await api("GET", "/api/settings");
  await api("PUT", "/api/settings", { ...settings, model: { ...settings.model, provider: "openai", name: "harness-fixture", base_url: `http://127.0.0.1:${modelPort}`, api_key_env: "YILIAN_HARNESS_FIXTURE_KEY", api_key_ref: null, temperature: 0 } });
  const ws = await api("POST", "/api/workspaces", { name: "Harness fixture", root_path: workspace });
  await api("POST", "/api/security/grants", { permission_id: "filesystem.read", effect: "allow", resource: { type: "filesystem", root: workspace, recursive: true } });
  const graph = async (name, tool, args) => api("POST", "/api/workflow-graphs", { name, definition: { schema_version: 1, entry_node_id: "tool", nodes: [{ id: "tool", kind: "tool", config: { type: "tool", tool_name: tool, arguments: args } }], edges: [] } });
  const a = await graph("工作流 A", "read_file", { path: input }), b = await graph("工作流 B", "glob", { pattern: "*.txt", path: workspace });
  const agentA = await api("POST", "/api/agents", { name: "智能体 A", user_prompt: "HARNESS_AGENT_A_PROOF", allowed_tools: ["read_file", "load_skill"] });
  const agentB = await api("POST", "/api/agents", { name: "智能体 B", user_prompt: "HARNESS_AGENT_B_PROOF", allowed_tools: ["glob", "load_skill"] });
  const pluginId = "harness-fixture-plugin";
  const pluginFiles = {
    "plugin.json": JSON.stringify({ schema_version: 1, id: pluginId, name: "Harness 试验插件", version: "1.0.0", contributes: { skills: ["skills/SKILL.md"], features: [{ id: "test-skill", title: "Harness fixture skill", target: { kind: "skill", contribution: "skills/SKILL.md" } }] } }),
    "skills/SKILL.md": "---\nname: harness-fixture-skill\ndescription: Local harness fixture\n---\n# HARNESS_PLUGIN_PROOF\n",
  };
  const packagePreview = await api("POST", "/api/capabilities/imports/inspect", { format: "plugin_source", files: Object.fromEntries(Object.entries(pluginFiles).map(([path, content]) => [path, Buffer.from(content).toString("base64")])) });
  const installed = await api("POST", "/api/capabilities/imports/confirm", { preview_id: packagePreview.preview_id });
  await api("PUT", `/api/capabilities/imports/${pluginId}`, { revision: installed.revision, action: "enable" });
  const skill = (await api("GET", "/api/plugins/features")).features.find(item => item.plugin_id === pluginId).resource_id;
  assert.ok(skill);

  const executablePath = ["C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe", "C:/Program Files/Microsoft/Edge/Application/msedge.exe"].find(existsSync);
  assert.ok(executablePath, "installed Edge required");
  browser = await chromium.launch({ executablePath, headless: true });
  const page = await browser.newPage({ viewport: { width: 1366, height: 768 } });
  await page.addInitScript(() => sessionStorage.setItem("yilian.setup.deferred", "true"));
  await page.goto(front + "/capabilities?category=harness");
  const panel = page.getByRole("region", { name: "Harness 配置" });
  await panel.getByRole("tab", { name: "工具组" }).click();
  async function createGroup(name, level, tools) {
    await panel.getByRole("button", { name: "新建工具组" }).click();
    const dialog = page.getByRole("dialog", { name: "配置工具组" });
    await dialog.getByLabel("名称").fill(name);
    await dialog.getByLabel("级别").selectOption(level);
    for (const tool of tools) await dialog.getByRole("checkbox", { name: new RegExp(` · ${tool}$`) }).check();
    await dialog.getByRole("button", { name: "保存", exact: true }).click();
    await dialog.waitFor({ state: "hidden" });
  }
  await createGroup("系统文件组 A", "system", ["read_file", "load_skill"]);
  await createGroup("用户搜索组 B", "user", ["glob", "load_skill"]);
  await panel.getByRole("tab", { name: "能力组合", exact: true }).click();
  async function createProfile(name, level, group, workflow, agent, plugin, skillId) {
    await panel.getByRole("button", { name: "新建 Harness" }).click();
    const dialog = page.getByRole("dialog", { name: "配置 Harness" });
    await dialog.getByLabel("名称").fill(name);
    await dialog.getByLabel("级别").selectOption(level);
    await dialog.getByRole("checkbox", { name: new RegExp(group) }).check();
    await dialog.getByRole("checkbox", { name: workflow, exact: true }).check();
    if (plugin) await dialog.getByRole("checkbox", { name: plugin, exact: true }).check();
    await dialog.getByLabel("技能范围").selectOption("selected");
    if (skillId) await dialog.getByRole("group", { name: "技能", exact: true }).getByRole("checkbox", { name: new RegExp(skillId + "$") }).check();
    await dialog.getByLabel("对话智能体").selectOption(agent);
    await dialog.getByRole("button", { name: "保存", exact: true }).click();
    await dialog.waitFor({ state: "hidden" });
  }
  await createProfile("配置 A", "system", "系统文件组 A", "工作流 A", agentA.id, "Harness 试验插件", skill);
  await createProfile("配置 B", "user", "用户搜索组 B", "工作流 B", agentB.id, "Harness 试验插件");
  async function activate(name) {
    await panel.locator("article").filter({ has: page.getByRole("heading", { name, exact: true }) }).getByRole("button", { name: "设为当前" }).click();
    await panel.getByText(`当前使用 ${name}。`, { exact: false }).waitFor();
    return (await api("GET", "/api/harnesses")).settings;
  }
  async function run(marker, tool, args) {
    const { chat } = await startChat(marker, ws, tool, args);
    const receipt = await until(() => receipts.get(marker)?.result && receipts.get(marker), "real tool response " + marker);
    const manifest = await api("GET", `/api/conversations/${chat.id}/context-manifest`);
    return { ...receipt, binding: manifest.manifest.entries.find(entry => entry.kind === "harness") };
  }
  const settingsA = await activate("配置 A");
  assert.deepEqual(settingsA.profiles.find(profile => profile.name === "配置 A").skill_ids, [skill]);
  const read = await run("HARNESS_A_READ", "read_file", { path: input });
  assert.ok(read.names.includes("read_file") && !read.names.includes("glob"));
  assert.ok(read.prompt.includes("HARNESS_AGENT_A_PROOF") && read.result.includes("HARNESS_LOCAL_READ_PROOF"));
  assert.equal(read.binding.version, String(settingsA.revision));
  assert.ok((await run("HARNESS_A_PLUGIN", "load_skill", { name: skill })).result.includes("HARNESS_PLUGIN_PROOF"));
  const runA = await api("POST", `/api/workflow-graphs/${a.id}/run`);
  await until(async () => (await api("GET", `/api/workflow-runs/${runA.run_id}`)).status === "completed", "workflow A completes");

  await api("PUT", "/api/security/approval-mode", { mode: "full_approval" });
  const { chat: pendingChat } = await startChat("HARNESS_A_PENDING", ws, "read_file", { path: input });
  const pending = await until(async () => (await api("GET", "/api/approvals/pending")).approvals.find(item => item.conversation_id === pendingChat.id), "configuration A waits for approval");

  const settingsB = await activate("配置 B");
  assert.deepEqual(settingsB.profiles.find(profile => profile.name === "配置 B").skill_ids, []);
  const approved = await fetch(`${base}/api/approvals/${pending.approval_id}/approve`, { method: "POST", headers, body: JSON.stringify({ conversation_id: pendingChat.id }), signal: AbortSignal.timeout(45000) });
  assert.equal(approved.status, 200);
  await approved.text();
  assert.ok((await until(() => receipts.get("HARNESS_A_PENDING")?.result, "approval retains configuration A")).includes("HARNESS_LOCAL_READ_PROOF"));
  assert.equal((await api("GET", `/api/conversations/${pendingChat.id}/context-manifest`)).manifest.entries.find(entry => entry.kind === "harness").version, String(settingsA.revision));
  await api("PUT", "/api/security/approval-mode", { mode: "partial_approval" });
  const glob = await run("HARNESS_B_GLOB", "glob", { pattern: "*.txt", path: workspace });
  assert.ok(glob.names.includes("glob") && !glob.names.includes("read_file"));
  assert.ok(glob.prompt.includes("HARNESS_AGENT_B_PROOF") && glob.result.includes("fixture.txt"));
  assert.equal(glob.binding.version, String(settingsB.revision));
  const denied = await run("HARNESS_B_DENIED", "read_file", { path: input });
  assert.ok(!denied.result.includes("HARNESS_LOCAL_READ_PROOF"));
  assert.match((await run("HARNESS_B_PLUGIN_DENIED", "load_skill", { name: skill })).result, /未配置此技能/);
  await api("POST", `/api/workflow-graphs/${a.id}/run`, undefined, 400);
  const runB = await api("POST", `/api/workflow-graphs/${b.id}/run`);
  await until(async () => (await api("GET", `/api/workflow-runs/${runB.run_id}`)).status === "completed", "workflow B completes");
  assert.equal((await api("GET", `/api/workflow-runs/${runA.run_id}/context-manifest`)).manifest.entries.find(entry => entry.kind === "harness").version, String(settingsA.revision));
  await page.reload();
  await page.getByText("当前使用 配置 B。", { exact: false }).waitFor();
  console.log("HARNESS PROFILES PASS", JSON.stringify({ uiCreatesBothLevels: true, activeSwitchPersists: true, toolsEnforced: true, selectedAgentsUsed: true, workflowToolsExecuted: true, pluginAccessScoped: true, historicalRevisionRetained: true, approvalResumesOriginalRevision: true }));
} catch (error) {
  console.error(error);
  console.error(logs.slice(-8).join("").slice(-3000));
  process.exitCode = 1;
} finally {
  await browser?.close();
  for (const child of children.reverse()) if (child.exitCode === null && child.signalCode === null) child.kill();
  model.closeAllConnections();
  await new Promise(resolve => model.close(resolve));
}
