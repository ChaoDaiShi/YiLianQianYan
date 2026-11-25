import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { existsSync } from "node:fs";
import { mkdir, mkdtemp } from "node:fs/promises";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { chromium } from "playwright-core";

const root = resolve(import.meta.dirname, "../..");
const temp = await mkdtemp(join(tmpdir(), "yilian-memory-review-"));
const evidence = join(root, "target", "memory-candidate-review");
await mkdir(evidence, { recursive: true });
const token = "memory-candidate-review-local-fixture-000000000";
const children = [];
const logs = [];
const userText = "我一直用 UTF-8 导出 CSV，报告统一使用中文。";
const accepted = "CSV 导出使用 UTF-8 编码";
const dismissed = "报告使用中文";
const unsupported = "助手推荐了另一个字符集";

const provider = createServer(async (request, response) => {
  try {
    const chunks = [];
    for await (const chunk of request) chunks.push(chunk);
    const body = JSON.parse(Buffer.concat(chunks).toString());
    if (body.stream) {
      response.writeHead(200, { "Content-Type": "text/event-stream" });
      response.end(`data: ${JSON.stringify({ choices: [{ index: 0, delta: { content: unsupported }, finish_reason: "stop" }] })}\n\ndata: [DONE]\n\n`);
      return;
    }
    const messages = JSON.parse(body.messages.at(-1).content);
    assert.equal(messages.length, 1);
    assert.equal(messages[0].text, userText);
    const id = messages[0].message_id;
    response.writeHead(200, { "Content-Type": "application/json" });
    response.end(JSON.stringify({ id: "review-fixture", choices: [{ index: 0, message: { role: "assistant", content: null, tool_calls: [{
      id: "memory-proposals", type: "function", function: { name: "extract_memories", arguments: JSON.stringify({ memories: [
        { content: accepted, category: "preference", message_id: id },
        { content: dismissed, category: "preference", message_id: id },
        { content: unsupported, category: "fact", message_id: "unknown-message" },
      ] }) },
    }] }, finish_reason: "tool_calls" }] }));
  } catch (error) {
    logs.push(String(error));
    response.writeHead(500).end();
  }
});

async function listen(server) {
  await new Promise(done => server.listen(0, "127.0.0.1", done));
  return server.address().port;
}
async function freePort() {
  const server = createServer();
  const port = await listen(server);
  await new Promise(done => server.close(done));
  return port;
}
function launch(command, args, env, cwd = root) {
  const child = spawn(command, args, { cwd, env: { ...process.env, ...env }, windowsHide: true, stdio: ["ignore", "pipe", "pipe"] });
  for (const stream of [child.stdout, child.stderr]) stream.on("data", chunk => logs.push(String(chunk)));
  children.push(child);
}
async function until(read, label) {
  const deadline = Date.now() + 30000;
  while (Date.now() < deadline) {
    try { if (await read()) return; } catch { /* startup polling */ }
    await new Promise(done => setTimeout(done, 150));
  }
  throw new Error(label);
}

const providerPort = await listen(provider), backendPort = await freePort(), frontendPort = await freePort();
const base = `http://127.0.0.1:${backendPort}`, front = `http://127.0.0.1:${frontendPort}`;
async function api(method, path, body, expected = 200) {
  const response = await fetch(base + path, { method, headers: { "Content-Type": "application/json", "X-Yilian-Control-Session": token }, body: body === undefined ? undefined : JSON.stringify(body) });
  const data = await response.text();
  assert.equal(response.status, expected, `${method} ${path}: ${data}`);
  return JSON.parse(data);
}

let browser;
try {
  launch(process.env.YILIAN_E2E_SERVER || join(root, "target/debug/yilian-server.exe"), [], { YILIAN_HOST: `127.0.0.1:${backendPort}`, YILIAN_WORKSPACE: temp,
    YILIAN_DATA_DIR: join(temp, "state"), YILIAN_ALLOWED_ORIGINS: front, YILIAN_CONTROL_SESSION_TOKEN: token, YILIAN_MEMORY_TEST_KEY: "loopback-fixture" });
  launch(process.execPath, [join(root, "frontend/node_modules/vite/bin/vite.js"), "--host", "127.0.0.1", "--port", String(frontendPort), "--strictPort"], { VITE_API_BASE: base, VITE_CONTROL_SESSION_TOKEN: token }, join(root, "frontend"));
  await Promise.all([until(async () => (await fetch(`${base}/api/health`)).ok, "backend startup"), until(async () => (await fetch(front)).ok, "frontend startup")]);
  const settings = await api("GET", "/api/settings");
  await api("PUT", "/api/settings", { ...settings, model: { ...settings.model, provider: "openai", name: "review-fixture", base_url: `http://127.0.0.1:${providerPort}`, api_key_env: "YILIAN_MEMORY_TEST_KEY", api_key_ref: null } });
  const conversation = await api("POST", "/api/conversations", { title: "记忆候选验收" });
  const chat = await fetch(`${base}/api/chat`, { method: "POST", headers: { "Content-Type": "application/json", "X-Yilian-Control-Session": token },
    body: JSON.stringify({ conversation_id: conversation.id, message: userText, workspace_mode: "chat" }) });
  assert.equal(chat.status, 200);
  assert.match(await chat.text(), /助手推荐了另一个字符集/);
  assert.ok([401, 403].includes((await fetch(`${base}/api/memories/candidates`)).status));

  const edge = ["C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe", "C:/Program Files/Microsoft/Edge/Application/msedge.exe"].find(existsSync);
  assert.ok(edge);
  browser = await chromium.launch({ executablePath: edge, headless: true });
  const page = await browser.newPage({ viewport: { width: 1366, height: 768 } });
  await page.addInitScript(() => sessionStorage.setItem("yilian.setup.deferred", "true"));
  await page.goto(`${front}/memory`);
  await page.getByRole("tab", { name: "待审阅" }).click();
  await page.getByRole("button", { name: "从对话提取" }).click();
  await page.getByLabel("来源对话", { exact: true }).selectOption(conversation.id);
  await page.getByRole("button", { name: "生成候选" }).click();
  await page.getByText("已生成 2 条待审阅候选").waitFor();
  assert.equal((await api("GET", "/api/memories")).length, 0);
  const pending = (await api("GET", "/api/memories/candidates")).entries;
  assert.equal(pending.length, 2);
  assert.ok(pending.every(item => item.source_message_id && item.source_excerpt === userText));
  assert.ok(pending.every(item => item.content !== unsupported));
  await page.getByRole("button", { name: new RegExp(accepted) }).click();
  const excerpt = page.getByRole("dialog", { name: "审阅记忆候选" }).locator("blockquote");
  await excerpt.waitFor();
  assert.equal(await excerpt.textContent(), userText);
  await page.screenshot({ path: join(evidence, "review.png"), animations: "disabled" });
  await page.getByLabel("确认后的记忆内容").fill("确认 CSV 统一用 UTF-8");
  await page.getByLabel("适用范围").selectOption("conversation");
  await page.getByRole("button", { name: "确认并保存" }).click();
  await page.getByText("已保存为长期记忆", { exact: true }).waitFor();
  const memory = (await api("GET", "/api/memories"))[0];
  assert.equal(memory.content, "确认 CSV 统一用 UTF-8");
  assert.equal(memory.source, "manual");
  assert.equal((await api("GET", `/api/memories/${memory.id}/governance`)).scope, `conversation:${conversation.id}`);
  assert.equal(JSON.parse(memory.metadata).source_message_id, pending[0].source_message_id);
  await page.getByRole("button", { name: new RegExp(dismissed) }).click();
  await page.getByRole("button", { name: "忽略候选" }).click();
  await page.getByText("暂无待审阅候选").waitFor();
  const reextract = await api("POST", "/api/memories/extract", { conversation_id: conversation.id });
  assert.deepEqual(reextract.candidate_ids, []);
  assert.equal((await api("GET", "/api/memories")).length, 1);
  await api("DELETE", `/api/conversations/${conversation.id}/messages/${pending[0].source_message_id}`);
  const remaining = (await api("GET", "/api/memories")).find(row => row.id === memory.id);
  assert.ok(remaining.source_removed);
  assert.equal(remaining.source_conversation_id ?? null, null);
  assert.equal(remaining.content, "确认 CSV 统一用 UTF-8");
  await api("DELETE", `/api/memories/${memory.id}`);
  assert.equal((await api("GET", "/api/memories")).length, 0);
  console.log(`Memory review passed: ${evidence}`);
} catch (error) {
  console.error(error, logs.slice(-6).join("").slice(-2000));
  process.exitCode = 1;
} finally {
  await browser?.close();
  for (const child of children.reverse()) child.kill();
  provider.closeAllConnections();
  await new Promise(done => provider.close(done));
}
