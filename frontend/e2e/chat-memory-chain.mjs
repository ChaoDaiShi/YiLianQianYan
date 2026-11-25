// Product UI, SQLite and execution gateway are real. Only the model is a local fixture.
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { existsSync } from "node:fs";
import { mkdir, mkdtemp, readFile, stat, writeFile } from "node:fs/promises";
import { createServer } from "node:http";
import { join, resolve } from "node:path";
import { tmpdir } from "node:os";
import { chromium } from "playwright-core";

const root = resolve(import.meta.dirname, "../..");
const isolated = await mkdtemp(join(tmpdir(), "yilian-chat-chain-"));
const workspace = join(isolated, "workspace"), data = join(isolated, "data");
const evidence = join(root, "target", "chat-memory-chain-clustered-timeline-review");
await Promise.all([mkdir(workspace), mkdir(data), mkdir(evidence, { recursive: true })]);
const token = "chat-memory-chain-controlled-session-000000000000";
const memoryText = "项目周报偏好：使用 UTF-8，按进度、风险、下一步三个栏目整理。";
const savedReply = "已记住这个工作空间的周报格式。以后整理周报时会参考这条偏好。";
const writtenReply = "已按记住的格式生成本周周报。文件已写入并核验，可在工作空间中查看。";
const failedReply = "周报写入失败，目标路径目前不可写。";
const report = "# 本周周报\n\n## 进度\n完成登录页联调。\n\n## 风险\n报表接口还在等待联调。\n\n## 下一步\n核对接口响应并补充回归。\n";
const file = join(workspace, "本周周报.md"), rejectedFile = join(workspace, "未批准周报.md");
const failedFile = join(workspace, "周报写入失败.md");
const children = [], logs = [];
let browser, memoryReachedModel = false;

async function listen(server) {
  await new Promise(done => server.listen(0, "127.0.0.1", done));
  return server.address().port;
}
async function freePort() {
  const server = createServer(); const port = await listen(server);
  await new Promise(done => server.close(done)); return port;
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
    const input = String(body.messages[index]?.content);
    const first = input.includes("记住这个工作空间");
    const rejected = input.includes("未批准周报");
    const failed = input.includes("周报写入失败");
    const toolMessages = body.messages.slice(index + 1).filter(message => message.role === "tool");
    if (!first && !toolMessages.length) memoryReachedModel ||= body.messages.some(message => message.role === "system" && String(message.content).includes(memoryText));
    const call = toolMessages.length === 0;
    const tool = first ? "save_memory" : "write_file";
    const args = first ? { content: memoryText, category: "preference", scope: "workspace" } : { path: failed ? failedFile : rejected ? rejectedFile : file, content: report };
    const delta = call ? { role: "assistant", tool_calls: [{ index: 0, id: `chain-${first ? "save" : rejected ? "reject" : failed ? "fail" : "write"}`, type: "function", function: { name: tool, arguments: JSON.stringify(args) } }] }
      : { content: first ? savedReply : failed ? failedReply : rejected ? "这次写入没有执行。" : writtenReply };
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
  for (const stream of [child.stdout, child.stderr]) stream.on("data", chunk => logs.push(String(chunk)));
  children.push(child);
}
async function until(read, label, timeout = 35000) {
  const end = Date.now() + timeout; let failure;
  while (Date.now() < end) {
    try { const value = await read(); if (value) return value; } catch (error) { failure = error; }
    await new Promise(done => setTimeout(done, 120));
  }
  throw new Error(`${label}: ${failure ?? "timed out"}`);
}
async function api(method, path, body) {
  const response = await fetch(base + path, { method, headers, body: body === undefined ? undefined : JSON.stringify(body), signal: AbortSignal.timeout(45000) });
  const text = await response.text(); assert.ok(response.ok, `${method} ${path}: ${text}`);
  return text ? JSON.parse(text) : null;
}

try {
  launch(join(root, "target/debug/yilian-server.exe"), [], { YILIAN_HOST: `127.0.0.1:${backendPort}`, YILIAN_WORKSPACE: workspace, YILIAN_DATA_DIR: data, YILIAN_ALLOWED_ORIGINS: front, YILIAN_CONTROL_SESSION_TOKEN: token, YILIAN_CHAIN_FIXTURE_KEY: "controlled-local-value" });
  launch(process.execPath, [join(root, "frontend/node_modules/vite/bin/vite.js"), "--host", "127.0.0.1", "--port", String(frontendPort), "--strictPort"], { VITE_API_BASE: base, VITE_CONTROL_SESSION_TOKEN: token }, join(root, "frontend"));
  await Promise.all([until(async () => (await fetch(base + "/api/health")).ok, "backend ready"), until(async () => (await fetch(front)).ok, "frontend ready")]);
  const settings = await api("GET", "/api/settings");
  await api("PUT", "/api/settings", { ...settings, model: { ...settings.model, provider: "openai", name: "chain-fixture", base_url: `http://127.0.0.1:${modelPort}`, api_key_env: "YILIAN_CHAIN_FIXTURE_KEY", api_key_ref: null, temperature: 0 } });
  const ws = await api("POST", "/api/workspaces", { name: "产品迭代演示", root_path: workspace });
  for (const permission_id of ["filesystem.read", "filesystem.write"]) await api("POST", "/api/security/grants", { permission_id, effect: "allow", resource: { type: "filesystem", root: workspace, recursive: true } });
  const executablePath = ["C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe", "C:/Program Files/Microsoft/Edge/Application/msedge.exe"].find(existsSync);
  assert.ok(executablePath, "installed Edge required");
  browser = await chromium.launch({ executablePath, headless: true });
  const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
  await page.addInitScript(() => sessionStorage.setItem("yilian.setup.deferred", "true"));
  async function chat(title, input) {
    const conversation = await api("POST", "/api/conversations", { title });
    await api("PUT", `/api/conversations/${conversation.id}/workspace`, { workspace_id: ws.id });
    await page.goto(`${front}/chat/${conversation.id}`);
    await page.getByRole("textbox", { name: "聊天输入" }).fill(input);
    await page.getByRole("button", { name: "发送消息", exact: true }).click();
    return conversation;
  }
  async function panel() {
    const toggle = page.getByRole("button", { name: "执行与审批", exact: true });
    if (await toggle.getAttribute("aria-pressed") !== "true") await toggle.click();
    return page.locator(".execution-sidebar");
  }
  async function captureChain(first, last, filename) {
    const top = await first.boundingBox(), bottom = await last.boundingBox();
    assert.ok(top && bottom);
    const x = Math.floor(Math.min(top.x, bottom.x)), y = Math.floor(top.y);
    await page.screenshot({ path: join(evidence, filename), animations: "disabled", clip: {
      x, y, width: Math.ceil(Math.max(top.x + top.width, bottom.x + bottom.width) - x), height: Math.ceil(bottom.y + bottom.height - y),
    } });
  }
  const source = await chat("周报偏好", `请记住这个工作空间的周报格式。${memoryText}`);
  await page.getByText(savedReply, { exact: true }).first().waitFor();
  const memory = await until(async () => (await api("GET", "/api/memories")).find(item => item.content === memoryText), "actual saved memory");
  assert.equal(memory.source_conversation_id, source.id);
  await api("PUT", "/api/security/approval-mode", { mode: "full_approval" });

  const current = await chat("生成周报", "按「项目周报偏好」生成本周周报。进度是登录页联调完成，风险是报表接口仍在等联调，下一步是核对接口并补回归。保存为本周周报.md。");
  await until(async () => (await api("GET", "/api/approvals/pending")).approvals.some(item => item.conversation_id === current.id), "actual write approval created");
  const execution = await panel();
  const chain = execution.getByRole("region", { name: "本轮工作链路" });
  const timeline = chain.getByRole("figure", { name: "本轮时间轴" });
  const openTimeline = timeline.getByRole("button", { name: "放大查看" });
  const timelineDialog = page.getByRole("dialog", { name: "本轮时间轴" });
  const graph = timelineDialog.getByRole("figure", { name: "可缩放时间轴" });
  const clusterFor = label => graph.getByRole("button", { name: /^放大 \d+ 个节点$/ }).and(graph.getByTitle(label));
  const sent = graph.getByRole("button", { name: "发送消息", exact: true });
  const memoryPoint = graph.getByRole("button", { name: "记忆引用", exact: true });
  const replyPoint = graph.getByRole("button", { name: "最终答复", exact: true });
  const durationAt = (chart, label) => chart.getByRole("group", { name: label, exact: true }).locator("[data-duration-ms]");
  const approval = execution.getByRole("region", { name: "待确认操作" });
  await approval.getByRole("button", { name: "允许此次工具操作" }).waitFor();
  await timeline.getByRole("img", { name: "记忆引用" }).waitFor();
  await chain.getByText(/1 项待审批/).waitFor();
  await timeline.getByRole("img", { name: /等待审批/ }).waitFor();
  const firstWait = Number(await durationAt(timeline, "审批等待").getAttribute("data-duration-ms"));
  await until(async () => Number(await durationAt(timeline, "审批等待").getAttribute("data-duration-ms")) > firstWait, "live approval clock advances");
  assert.equal(await timeline.getByRole("group", { name: "工具执行" }).count(), 0);
  assert.equal(await timeline.getByRole("img", { name: "最终答复" }).count(), 0);
  const manifest = await api("GET", `/api/conversations/${current.id}/context-manifest`);
  assert.ok(manifest.manifest.entries.some(entry => entry.kind === "memory" && entry.source_id === memory.id && ["resolved", "truncated"].includes(entry.status)));
  assert.ok(manifest.timeline.memory_sources.some(entry => entry.source_id === memory.id));
  assert.ok(manifest.timeline.request_received_at <= manifest.timeline.memory_resolved_at);
  assert.equal(memoryReachedModel, true);
  assert.equal(existsSync(file), false);
  await page.screenshot({ path: join(evidence, "01-approval.png"), animations: "disabled" });
  await captureChain(approval, chain, "01-approval-chain.png");
  await approval.getByRole("button", { name: "允许此次工具操作" }).click();
  await chain.getByText(/1 项已批准.*1 项已核验/).waitFor();
  await page.getByText(writtenReply, { exact: true }).first().waitFor();
  await timeline.getByRole("img", { name: "最终答复" }).waitFor();
  const audit = await api("GET", `/api/security/audit?conversation_id=${current.id}`);
  const at = eventType => audit.events.find(event => event.tool_call_id === "chain-write" && event.event_type === eventType).created_at;
  const approvalMs = at("approval_resolved") - at("approval_requested");
  const executionMs = at("execution_finished") - at("execution_started");
  assert.ok(manifest.timeline.memory_resolved_at <= at("approval_requested"));
  assert.equal(Number(await durationAt(timeline, "审批等待").getAttribute("data-duration-ms")), approvalMs);
  assert.equal(Number(await durationAt(timeline, "工具执行").getAttribute("data-duration-ms")), executionMs);
  const savedChat = await api("GET", `/api/conversations/${current.id}`);
  const finalMessage = savedChat.messages.find(message => message.content === writtenReply);
  assert.ok(finalMessage.created_at >= at("verification_finished"));
  assert.equal(await readFile(file, "utf8"), report);
  await writeFile(join(evidence, "本周周报.md"), await readFile(file));
  await page.screenshot({ path: join(evidence, "02-completed.png"), animations: "disabled" });
  await captureChain(chain, execution.getByRole("region", { name: "历史记录" }), "02-completed-chain.png");
  await chain.screenshot({ path: join(evidence, "06-arrow-overview.png"), animations: "disabled" });
  await openTimeline.click();
  await graph.waitFor();
  const approvalBar = await graph.locator('[data-stage="approval"].turn-timeline-span').boundingBox();
  const executionBar = await graph.locator('[data-stage="execution"].turn-timeline-span').boundingBox();
  assert.ok(approvalBar.width > executionBar.width);
  const fullWindow = Number(await graph.getAttribute("data-window-ms"));
  const executionCluster = clusterFor(/执行结束/);
  await executionCluster.waitFor();
  const groupedCount = Number(await executionCluster.getAttribute("data-node-count"));
  const groupHasReply = (await executionCluster.getAttribute("title")).includes("最终答复");
  assert.ok(groupedCount >= 2);
  assert.equal(await graph.getByRole("button", { name: "执行结束", exact: true }).count(), 0);
  await timelineDialog.screenshot({ path: join(evidence, "09-timeline-grouped.png"), animations: "disabled" });
  await timelineDialog.getByRole("button", { name: "放大时间轴" }).click();
  assert.equal(Number(await graph.getAttribute("data-window-ms")), fullWindow / 2);
  await timelineDialog.getByRole("button", { name: "查看更早" }).click();
  await timelineDialog.getByRole("button", { name: "查看更晚" }).click();
  await timelineDialog.getByRole("button", { name: "缩小时间轴" }).click();
  assert.equal(Number(await graph.getAttribute("data-window-ms")), fullWindow);
  await executionCluster.click();
  assert.ok(Number(await graph.getAttribute("data-window-ms")) < fullWindow / 2);
  const finishedPoint = graph.getByRole("button", { name: "执行结束", exact: true });
  const verifiedPoint = graph.getByRole("button", { name: "已核验", exact: true });
  await finishedPoint.waitFor();
  await until(async () => finishedPoint.evaluate(element => element === document.activeElement), "expanding group focuses the selected event");
  const expandedPoints = [[finishedPoint, at("execution_finished")], [verifiedPoint, at("verification_finished")]];
  if (groupHasReply) expandedPoints.push([replyPoint, finalMessage.created_at]);
  for (const [point, stamp] of expandedPoints) {
    await point.waitFor();
    assert.equal(Number(await point.getAttribute("data-at")), stamp);
  }
  const [runBox, checkBox, replyBox] = await Promise.all(expandedPoints.map(([point]) => point.boundingBox()));
  const overlaps = (a, b) => a.x < b.x + b.width && a.x + a.width > b.x && a.y < b.y + b.height && a.y + a.height > b.y;
  assert.ok(!overlaps(runBox, checkBox) && (!replyBox || !overlaps(runBox, replyBox) && !overlaps(checkBox, replyBox)));
  await finishedPoint.click();
  const nodeDetails = timelineDialog.getByRole("region", { name: "节点详情" });
  assert.equal(await nodeDetails.locator("time").getAttribute("datetime"), new Date(at("execution_finished")).toISOString());
  await nodeDetails.getByText(/耗时/).waitFor();
  await timelineDialog.screenshot({ path: join(evidence, "10-timeline-group-expanded.png"), animations: "disabled" });
  if (!groupHasReply) await timelineDialog.getByRole("button", { name: "聚焦执行" }).click();
  await replyPoint.click();
  await nodeDetails.getByText(writtenReply, { exact: true }).waitFor();
  await timelineDialog.screenshot({ path: join(evidence, "11-arrow-timeline-reply.png"), animations: "disabled" });
  await timelineDialog.getByRole("button", { name: "全程", exact: true }).click();
  assert.equal(Number(await graph.getAttribute("data-window-ms")), fullWindow);
  await executionCluster.waitFor();
  await page.keyboard.press("Escape");
  await timelineDialog.waitFor({ state: "hidden" });
  await until(async () => openTimeline.evaluate(element => element === document.activeElement), "closing restores timeline trigger focus");
  await page.reload();
  await panel();
  await timeline.getByRole("img", { name: "已批准" }).waitFor();
  await timeline.getByRole("img", { name: "已核验" }).waitFor();
  assert.equal(Number(await durationAt(timeline, "审批等待").getAttribute("data-duration-ms")), approvalMs);
  await openTimeline.click();
  await timelineDialog.getByRole("button", { name: "聚焦执行" }).click();
  assert.equal(Number(await replyPoint.getAttribute("data-at")), finalMessage.created_at);
  await timelineDialog.getByRole("button", { name: "全程", exact: true }).click();
  const memoryCluster = clusterFor(/记忆引用/);
  if (await memoryCluster.count()) await memoryCluster.click();
  assert.equal(Number(await sent.getAttribute("data-at")), manifest.timeline.request_received_at);
  assert.equal(Number(await memoryPoint.getAttribute("data-at")), manifest.timeline.memory_resolved_at);
  await memoryPoint.click();
  await nodeDetails.getByRole("link", { name: "查看引用记忆" }).click();
  const detail = page.getByRole("dialog", { name: "引用记忆与来源" });
  await detail.locator(".memory-detail-body").getByText(memoryText, { exact: true }).waitFor();
  await detail.getByText("工作空间 · 产品迭代演示", { exact: true }).waitFor();
  assert.equal(await detail.getByRole("link", { name: "打开来源对话" }).getAttribute("href"), `/chat/${source.id}`);
  await page.screenshot({ path: join(evidence, "03-memory-source.png"), animations: "disabled" });
  await detail.screenshot({ path: join(evidence, "03-memory-detail.png"), animations: "disabled" });
  await detail.getByRole("link", { name: "打开来源对话" }).click();
  await page.getByText(savedReply, { exact: true }).first().waitFor();

  const rejected = await chat("拒绝执行验证", "按项目周报偏好整理，并另存为未批准周报.md。");
  await until(async () => (await api("GET", "/api/approvals/pending")).approvals.some(item => item.conversation_id === rejected.id), "actual rejection scenario approval created");
  const rejectPanel = await panel();
  await rejectPanel.getByRole("region", { name: "待确认操作" }).getByRole("button", { name: "拒绝此次工具操作" }).click();
  await rejectPanel.getByText("用户拒绝，未执行", { exact: true }).first().waitFor();
  const rejectedChain = rejectPanel.getByRole("region", { name: "本轮工作链路" });
  const rejectedTimeline = rejectedChain.getByRole("figure", { name: "本轮时间轴" });
  await rejectedTimeline.getByRole("img", { name: "已拒绝" }).waitFor();
  assert.equal(await rejectedTimeline.getByRole("group", { name: "工具执行" }).count(), 0);
  await rejectedChain.getByText("本轮结束", { exact: true }).waitFor();
  assert.equal(existsSync(rejectedFile), false);
  await mkdir(failedFile);
  const failed = await chat("周报写入失败验证", "按项目周报偏好整理，保存为周报写入失败.md。");
  await until(async () => (await api("GET", "/api/approvals/pending")).approvals.some(item => item.conversation_id === failed.id), "approval for failing write");
  const failurePanel = await panel();
  await failurePanel.getByRole("button", { name: "允许此次工具操作" }).click();
  const failureChain = failurePanel.getByRole("region", { name: "本轮工作链路" });
  const failureChart = failureChain.getByRole("figure", { name: "本轮时间轴" });
  await page.getByText(failedReply, { exact: true }).first().waitFor();
  await failureChart.getByRole("img", { name: "最终答复" }).waitFor();
  const failureTrigger = failureChart.getByRole("button", { name: "放大查看" });
  await failureTrigger.click();
  const failureCluster = clusterFor(/执行失败/);
  if (await failureCluster.count()) await failureCluster.click();
  const failurePoint = graph.getByRole("button", { name: "执行失败", exact: true });
  await failurePoint.click();
  await nodeDetails.getByText(/无法写入文件.*周报写入失败\.md/).waitFor();
  await timelineDialog.screenshot({ path: join(evidence, "12-arrow-timeline-failure.png"), animations: "disabled" });
  assert.ok(await timelineDialog.evaluate(dialog => dialog.scrollWidth <= dialog.clientWidth + 1));
  await page.keyboard.press("Escape");
  await timelineDialog.waitFor({ state: "hidden" });
  await until(async () => failureTrigger.evaluate(element => element === document.activeElement), "closing failure timeline restores focus");
  assert.equal((await stat(failedFile)).isDirectory(), true);
  console.log("CHAT MEMORY CHAIN PASS", JSON.stringify({ memorySavedThroughChat: true, recallUsedByModel: true, noWriteBeforeApproval: true, verifiedOutput: true, sourceLinks: true, rejectPreventsWrite: true, timelineAfterReload: true, zoomAndPan: true, groupedCount, denseLabelsSeparated: true, approvalMs, executionMs, failureReasonOpened: true, evidence }));
} catch (error) {
  console.error(error);
  console.error(logs.slice(-7).join("").slice(-2600));
  process.exitCode = 1;
} finally {
  await browser?.close();
  for (const child of children.reverse()) if (child.exitCode === null && child.signalCode === null) child.kill();
  model.closeAllConnections();
  await new Promise(done => model.close(done));
}
