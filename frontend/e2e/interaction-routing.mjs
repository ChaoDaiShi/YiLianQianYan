import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { existsSync } from "node:fs";
import { mkdir, mkdtemp, readFile } from "node:fs/promises";
import { createServer } from "node:http";
import { once } from "node:events";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { chromium } from "playwright-core";

const root = resolve(import.meta.dirname, "../..");
const temp = await mkdtemp(join(tmpdir(), "yilian-interaction-"));
const evidence = resolve(process.env.YILIAN_INTERACTION_EVIDENCE || join(root, "target", "interaction-routing-e2e"));
await mkdir(evidence, { recursive: true });
const file = join(temp, "交付.txt");
const token = "interaction-routing-fixture-control-session-00000000";
const requests = [];
const mixed = "今天有点累，先别改代码，帮我想想记忆模块怎么做";
const work = "请把刚才的方案写入交付.txt";
const cancelledWork = "请写入另一份交付文件";
const steeringWork = "请写入需要修正的目标文件";
const steeringText = "先不要写文件，只说明要怎么做。";
const knowledgeQuestion = "帮我查一下资料中的铜鹤编号";
const knowledgeText = "项目试验记录。铜鹤编号为 YL-2407，实测工位记录已保存。";
const unexpected = join(temp, "禁止生成.txt");
const parallelA = "请把第一项任务写入并行甲.txt";
const parallelB = "请把第二项任务写入并行乙.txt";
const sideChat = "今天真的好累，我们随便聊聊吧";
const parallelFiles = new Map([[parallelA, join(temp, "并行甲.txt")], [parallelB, join(temp, "并行乙.txt")]]);
let releaseParallel;
const holdParallel = new Promise(resolve => { releaseParallel = resolve; });
let releaseSteering;
const holdSteering = new Promise(resolve => { releaseSteering = resolve; });
let releaseWork;
const holdWork = new Promise(resolve => { releaseWork = resolve; });
const children = [];
const logs = [];
const elapsedMs = {};

function stream(response, delta, reason = "stop") {
  response.writeHead(200, { "Content-Type": "text/event-stream" });
  response.write(`data: ${JSON.stringify({ choices: [{ index: 0, delta }] })}\n\n`);
  response.write(`data: ${JSON.stringify({ choices: [{ index: 0, delta: {}, finish_reason: reason }], usage: { prompt_tokens: 12, completion_tokens: 8, total_tokens: 20 } })}\n\n`);
  response.end("data: [DONE]\n\n");
}

const model = createServer(async (request, response) => {
  const chunks = [];
  for await (const chunk of request) chunks.push(chunk);
  const input = JSON.parse(Buffer.concat(chunks).toString());
  requests.push(input);
  const user = input.messages.findLast(message => message.role === "user");
  if (parallelFiles.has(user?.content)) {
    if (input.messages.at(-1)?.role === "user") {
      await holdParallel;
      stream(response, { tool_calls: [{ index: 0, id: `parallel-${user.content === parallelA ? "a" : "b"}`, type: "function", function: {
        name: "write_file", arguments: JSON.stringify({ path: parallelFiles.get(user.content), content: user.content }),
      } }] }, "tool_calls");
    } else {
      stream(response, { content: `${user.content === parallelA ? "甲" : "乙"}任务已完成。` });
    }
    return;
  }
  if (user?.content === sideChat) {
    stream(response, { content: "我们聊一会儿，两个任务可以继续处理。" });
    return;
  }
  if (user?.content === knowledgeQuestion) {
    const citation = input.messages.find(message => message.role === "system")?.content.match(/引用链接：(\/knowledge\?[^\s]+)/)?.[1];
    stream(response, { content: citation ? `铜鹤编号为 YL-2407。查看[资料原文](${citation})。` : "没有收到资料引用。" });
    return;
  }
  if (user?.content === cancelledWork) return;
  if (user?.content === steeringWork) {
    await holdSteering;
    stream(response, { tool_calls: [{ index: 0, id: "stale-write", type: "function", function: {
      name: "write_file", arguments: JSON.stringify({ path: unexpected, content: "过期操作" }),
    } }] }, "tool_calls");
    return;
  }
  if (user?.content === steeringText) {
    stream(response, { content: "已按补充要求改为说明方案。" });
    return;
  }
  if (user?.content === work && input.messages.at(-1)?.role === "user") {
    await holdWork;
    stream(response, { tool_calls: [{ index: 0, id: "fixture-write", type: "function", function: {
      name: "write_file", arguments: JSON.stringify({ path: file, content: "记忆模块按范围召回。" }),
    } }] }, "tool_calls");
  } else if (user?.content === work) {
    stream(response, { content: "方案已写入交付.txt。" });
  } else {
    stream(response, { content: "今天先缓一缓。记忆模块可以按适用范围召回。" });
  }
});

async function listen(server) {
  await new Promise(done => server.listen(0, "127.0.0.1", done));
  return server.address().port;
}
async function freePort() {
  const socket = createServer();
  const port = await listen(socket);
  await new Promise(done => socket.close(done));
  return port;
}
function launch(command, args, env, cwd = root) {
  const child = spawn(command, args, { cwd, env: { ...process.env, ...env }, windowsHide: true, stdio: ["ignore", "pipe", "pipe"] });
  for (const stream of [child.stdout, child.stderr]) stream.on("data", chunk => logs.push(String(chunk)));
  children.push(child);
  return child;
}
async function until(read, label) {
  const deadline = Date.now() + 30000;
  while (Date.now() < deadline) {
    try { const result = await read(); if (result) return result; } catch { /* startup polling */ }
    await new Promise(done => setTimeout(done, 120));
  }
  throw new Error(label);
}

const modelPort = await listen(model), backendPort = await freePort(), frontendPort = await freePort();
const base = `http://127.0.0.1:${backendPort}`, front = `http://127.0.0.1:${frontendPort}`;
async function api(method, path, body) {
  const result = await fetch(base + path, { method, headers: { "Content-Type": "application/json", "X-Yilian-Control-Session": token }, body: body === undefined ? undefined : JSON.stringify(body) });
  const data = await result.text();
  assert.ok(result.ok, `${path}: ${data}`);
  return JSON.parse(data);
}

let browser;
try {
  const backend = process.env.YILIAN_E2E_SERVER || join(root, "target/debug/yilian-server.exe");
  const backendEnv = {
    YILIAN_HOST: `127.0.0.1:${backendPort}`, YILIAN_WORKSPACE: temp, YILIAN_DATA_DIR: join(temp, "state"),
    YILIAN_ALLOWED_ORIGINS: front, YILIAN_CONTROL_SESSION_TOKEN: token, YILIAN_ROUTING_TEST_KEY: "local-fixture",
  };
  const serverProcess = launch(backend, [], backendEnv);
  launch(process.execPath, [join(root, "frontend/node_modules/vite/bin/vite.js"), "--host", "127.0.0.1", "--port", String(frontendPort), "--strictPort"], { VITE_API_BASE: base, VITE_CONTROL_SESSION_TOKEN: token }, join(root, "frontend"));
  await Promise.all([until(async () => (await fetch(base + "/api/health")).ok, "backend startup"), until(async () => (await fetch(front)).ok, "frontend startup")]);
  const settings = await api("GET", "/api/settings");
  await api("PUT", "/api/settings", { ...settings, model: { ...settings.model, provider: "openai", name: "interaction-fixture", base_url: `http://127.0.0.1:${modelPort}`, api_key_env: "YILIAN_ROUTING_TEST_KEY", api_key_ref: null } });
  await api("POST", "/api/security/grants", { permission_id: "filesystem.write", effect: "allow", resource: { type: "filesystem", root: temp, recursive: true } });
  await api("PUT", "/api/security/approval-mode", { mode: "full_approval" });
  const { id } = await api("POST", "/api/conversations", { title: "交互验收" });
  const agents = (await api("GET", "/api/agents")).agents;
  assert.equal(agents.filter(agent => agent.id.startsWith("interaction-")).length, 5);
  const profilePath = "/api/agents/interaction-daily/profile";
  const profile = await api("GET", profilePath);
  await api("PUT", profilePath, { ...profile, persona_enabled: true, persona: { ...profile.persona, tone: "清晰、温和" } });
  assert.equal((await api("GET", profilePath)).persona.tone, "清晰、温和");
  assert.equal((await api("DELETE", profilePath)).persona.tone, "");
  const executablePath = ["C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe", "C:/Program Files/Microsoft/Edge/Application/msedge.exe"].find(existsSync);
  assert.ok(executablePath);
  browser = await chromium.launch({ executablePath, headless: true });
  const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
  const errors = [];
  page.on("pageerror", error => errors.push(error.message));
  await page.addInitScript(() => sessionStorage.setItem("yilian.setup.deferred", "true"));
  await page.goto(`${front}/chat/${id}`);
  const input = page.getByRole("textbox", { name: "聊天输入" });
  async function send(text) {
    await input.fill(text);
    await page.getByRole("button", { name: "发送消息", exact: true }).click();
  }
  const modes = page.getByRole("button", { name: "切换工作模式" });
  await modes.click();
  await page.getByRole("button", { name: /聊天.*交流/ }).waitFor();
  await page.screenshot({ path: join(evidence, "01-mode-selection.png"), animations: "disabled" });
  await page.getByRole("button", { name: /聊天.*交流/ }).click();
  await until(async () => (await api("GET", `/api/conversations/${id}`)).workspace_mode === "chat", "manual mode persisted");
  await page.reload();
  await modes.getByText("聊天", { exact: true }).waitFor();
  await modes.click();
  await page.getByRole("button", { name: /自动.*本轮/ }).click();
  await until(async () => (await api("GET", `/api/conversations/${id}`)).workspace_mode === "auto", "auto mode restored");
  const mixedStarted = performance.now();
  await send(mixed);
  await page.getByText("今天先缓一缓。记忆模块可以按适用范围召回。", { exact: true }).first().waitFor();
  await until(async () => (await api("GET", `/api/conversations/${id}`)).run_status === "completed", "mixed turn committed");
  assert.ok(!requests[0].tools?.some(tool => tool.function.name === "write_file"));
  assert.equal((await api("GET", "/api/memories")).length, 0);
  elapsedMs.mixedReply = Math.round(performance.now() - mixedStarted);
  await send(work);
  await page.getByText("正在处理工作", { exact: true }).waitFor();
  const statusStarted = performance.now();
  await send("任务进度");
  await page.getByText("当前对话仍在处理本轮请求。", { exact: true }).first().waitFor();
  elapsedMs.progress = Math.round(performance.now() - statusStarted);
  await page.screenshot({ path: join(evidence, "02-work-progress.png"), animations: "disabled" });
  assert.equal(existsSync(file), false);
  releaseWork();
  const approval = await until(async () => (await api("GET", "/api/approvals/pending")).approvals.find(item => item.conversation_id === id), "real approval");
  assert.equal(approval.tool_name, "write_file");
  assert.equal(existsSync(file), false);
  const toggle = page.getByRole("button", { name: "执行与审批", exact: true });
  if (await toggle.getAttribute("aria-pressed") !== "true") await toggle.click();
  const approvalStarted = performance.now();
  await page.getByRole("button", { name: "允许此次工具操作" }).click();
  await page.getByText("方案已写入交付.txt。", { exact: true }).first().waitFor();
  assert.equal(await readFile(file, "utf8"), "记忆模块按范围召回。");
  elapsedMs.approvedDelivery = Math.round(performance.now() - approvalStarted);
  const completed = await api("GET", `/api/conversations/${id}`);
  assert.equal(completed.workspace_mode, "auto");
  assert.equal(completed.messages.filter(message => message.role === "user").length, 2);
  await page.reload();
  await page.locator("article[data-message-id]").getByText("方案已写入交付.txt。", { exact: true }).first().waitFor();
  await page.screenshot({ path: join(evidence, "03-verified-delivery.png"), animations: "disabled" });
  await send(cancelledWork);
  await until(() => requests.some(request => request.messages.some(message => message.role === "user" && message.content === cancelledWork)), "cancellation fixture entered");
  await send("取消当前任务");
  await page.getByText("已向当前轮次发出取消请求。", { exact: true }).first().waitFor();
  await until(async () => (await api("GET", `/api/conversations/${id}`)).run_status === "cancelled", "cancelled turn committed");
  const afterCancel = await api("GET", `/api/conversations/${id}`);
  assert.equal(afterCancel.messages.filter(message => message.role === "user").length, 3);
  await send(steeringWork);
  await until(() => requests.some(request => request.messages.at(-1)?.content === steeringWork), "steering fixture entered");
  await send(`补充给当前任务：${steeringText}`);
  await page.getByText("已收到补充，当前步骤结束后纳入处理。", { exact: true }).first().waitFor();
  const steeringStarted = performance.now();
  releaseSteering();
  await page.getByText("已按补充要求改为说明方案。", { exact: true }).first().waitFor();
  assert.equal(existsSync(unexpected), false);
  const steeredRequest = requests.find(request => request.messages.at(-1)?.content === steeringText);
  assert.ok(steeredRequest, "supplement reached the next model request");
  assert.ok(!steeredRequest.tools?.some(tool => tool.function.name === "write_file"));
  const afterSteering = await api("GET", `/api/conversations/${id}`);
  assert.equal(afterSteering.messages.filter(message => message.role === "user" && message.content === steeringText).length, 1);
  elapsedMs.steeredReply = Math.round(performance.now() - steeringStarted);
  await page.goto(`${front}/settings?section=appearance`);
  await page.getByRole("radio", { name: "夜间" }).check();
  await page.setViewportSize({ width: 960, height: 768 });
  await page.goto(`${front}/chat/${id}`);
  await page.locator("article[data-message-id]").getByText("方案已写入交付.txt。", { exact: true }).first().waitFor();
  await modes.click();
  await page.getByRole("button", { name: /工作.*交付/ }).waitFor();
  assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1));
  await page.screenshot({ path: join(evidence, "04-dark-compact.png"), animations: "disabled" });
  const imported = await fetch(`${base}/api/resources/ingest?${new URLSearchParams({ name: "铜鹤试验.txt" })}`, {
    method: "POST", headers: { "Content-Type": "text/plain", "X-Yilian-Control-Session": token }, body: knowledgeText,
  });
  assert.ok(imported.ok);
  const resource = await imported.json();
  const { document } = await api("POST", "/api/knowledge/documents", { resource_id: resource.id });
  await page.keyboard.press("Escape");
  await send(knowledgeQuestion);
  const sourceLink = page.getByRole("link", { name: "资料原文" });
  await sourceLink.waitFor();
  const citation = await sourceLink.getAttribute("href");
  assert.ok(citation.includes(`document=${document.id}`));
  await sourceLink.click();
  await page.getByText(/已定位原文第/).waitFor();
  assert.ok((await page.locator("mark.knowledge-source-highlight").textContent()).includes("YL-2407"));
  await page.screenshot({ path: join(evidence, "05-knowledge-citation.png"), animations: "disabled" });
  await api("DELETE", `/api/knowledge/documents/${document.id}`);
  await page.goto(front + citation);
  await page.getByText("知识文档不存在", { exact: true }).waitFor();
  await page.goto(`${front}/chat/${id}`);
  await page.locator("article[data-message-id]").getByText("铜鹤编号为 YL-2407。", { exact: false }).first().waitFor();
  await input.waitFor();
  const laneToggle = page.getByRole("button", { name: "并行运行", exact: true });
  const laneDialog = page.getByRole("dialog", { name: "当前会话的并行运行" });
  await laneToggle.click();
  await laneDialog.getByRole("button", { name: "发起新任务", exact: true }).click();
  await until(async () => (await input.inputValue()) === "新任务：", "new task action prepares composer");
  await send(`新任务：${parallelA}`);
  await until(() => requests.some(request => request.messages.at(-1)?.content === parallelA), "first parallel task entered");
  await send(parallelB);
  await until(() => requests.some(request => request.messages.at(-1)?.content === parallelB), "second parallel task entered");
  await send(sideChat);
  await page.getByText("我们聊一会儿，两个任务可以继续处理。", { exact: true }).first().waitFor();
  const lanesPath = `/api/conversations/${id}/lanes`;
  const concurrent = await api("GET", lanesPath);
  assert.equal(concurrent.filter(lane => lane.kind === "work" && lane.run_status === "running").length, 2);
  assert.ok(concurrent.some(lane => lane.kind === "chat"));
  assert.ok(!requests.find(request => request.messages.at(-1)?.content === sideChat)?.tools?.some(tool => tool.function.name === "write_file"));
  assert.equal(new URL(page.url()).pathname, `/chat/${id}`);
  const ambiguous = await fetch(`${base}${lanesPath}`, { method: "POST", headers: { "Content-Type": "application/json", "X-Yilian-Control-Session": token }, body: JSON.stringify({ kind: "auto", title: "把刚才的文件名改成最终版" }) });
  assert.equal(ambiguous.status, 409);
  assert.equal((await ambiguous.json()).code, "conversation_lane_target_required");
  const deletingWhileActive = await fetch(`${base}/api/conversations/${id}`, { method: "DELETE", headers: { "X-Yilian-Control-Session": token } });
  assert.equal(deletingWhileActive.status, 409);
  await laneToggle.click();
  await laneDialog.screenshot({ path: join(evidence, "06-parallel-runs.png"), animations: "disabled" });
  await laneDialog.getByRole("button", { name: /并行甲\.txt.*处理中/ }).waitFor();
  await laneDialog.getByRole("button", { name: new RegExp(parallelB.replace(".", "\\.")) }).click();
  await send("取消当前任务");
  await until(async () => (await api("GET", lanesPath)).some(lane => lane.title === parallelB && lane.run_status === "cancelled"), "only second lane cancelled");
  assert.ok((await api("GET", lanesPath)).some(lane => lane.title === parallelA && lane.run_status === "running"));
  releaseParallel();
  const firstLane = concurrent.find(lane => lane.title === parallelA);
  const approvalA = await until(async () => (await api("GET", "/api/approvals/pending")).approvals.find(item => item.conversation_id === firstLane.id), "first lane approval");
  assert.equal(existsSync(parallelFiles.get(parallelA)), false);
  const approvedA = await fetch(`${base}/api/approvals/${approvalA.approval_id}/approve`, { method: "POST", headers: { "Content-Type": "application/json", "X-Yilian-Control-Session": token }, body: JSON.stringify({ conversation_id: firstLane.id }) });
  assert.ok(approvedA.ok);
  await approvedA.text();
  assert.equal(await readFile(parallelFiles.get(parallelA), "utf8"), parallelA);
  assert.equal(existsSync(parallelFiles.get(parallelB)), false);
  const stopped = once(serverProcess, "exit");
  serverProcess.kill();
  await stopped;
  launch(backend, [], backendEnv);
  const restoredLanes = await until(async () => {
    const rows = await api("GET", lanesPath);
    return rows.length === 3 ? rows : null;
  }, "parallel lanes recovered after server restart");
  assert.deepEqual(Object.fromEntries(restoredLanes.map(lane => [lane.title, lane.run_status])), {
    [parallelA]: "completed", [parallelB]: "cancelled", [sideChat]: "completed",
  });
  await page.reload();
  await laneToggle.click();
  await laneDialog.getByRole("button", { name: /并行甲\.txt.*已结束/ }).click();
  await page.locator("article[data-message-id]").getByText("甲任务已完成。", { exact: true }).first().waitFor();
  assert.equal((await api("DELETE", `/api/conversations/${id}`)).status, "deleted");
  for (const lane of restoredLanes) {
    const removed = await fetch(`${base}/api/conversations/${lane.id}`, { headers: { "X-Yilian-Control-Session": token } });
    assert.equal(removed.status, 404);
  }
  assert.deepEqual(await api("GET", lanesPath), []);
  assert.deepEqual(errors, []);
  console.log(JSON.stringify({ mixedReply: true, realFileVerified: true, presetReset: true, cancelled: true, steering: true, knowledgeCitation: true, parallelTasksAndChat: true, restoredAfterRestart: true, providerCalls: requests.length, elapsedMs, evidence }));
} catch (error) {
  console.error(error, logs.slice(-5).join("").slice(-2600));
  process.exitCode = 1;
} finally {
  releaseWork();
  releaseSteering();
  releaseParallel();
  await browser?.close();
  for (const child of children.reverse()) child.kill();
  model.closeAllConnections();
  await new Promise(done => model.close(done));
}
