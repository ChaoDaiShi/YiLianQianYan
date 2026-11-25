import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync } from "node:fs";
import { mkdir, mkdtemp, readFile, writeFile } from "node:fs/promises";
import { createServer } from "node:http";
import { createServer as createTcpServer } from "node:net";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { chromium } from "playwright-core";
import { pathToFileURL } from "node:url";

// All product HTTP, SQLite, execution, approval and file APIs below are real.
// Model planning and the remote MCP provider are explicit local controlled fixtures.
const root = resolve(import.meta.dirname, "../..");
const evidenceDir = process.env.YILIAN_SHARED_GOAL_EVIDENCE ?? "C:/Users/25113/.codex/visualizations/2026/10/29/01a1881b-f385-7b01-a3a7-270f9de6e401/shared-goal-state";
await mkdir(evidenceDir, { recursive: true });
const isolatedRoot = await mkdtemp(join(tmpdir(), "yilian-shared-goal-"));
const dataDir = join(isolatedRoot, "data"), workspacePath = join(isolatedRoot, "workspace");
await Promise.all([mkdir(dataDir), mkdir(workspacePath)]);
const backendPort = await freePort(), frontendPort = await freePort();
const base = `http://127.0.0.1:${backendPort}`, frontend = `http://127.0.0.1:${frontendPort}`;
const controlToken = "shared-goal-local-controlled-token-".padEnd(64, "x");
const backendBinary = process.env.YILIAN_E2E_BACKEND_BINARY ?? join(root, "target/debug/yilian-server.exe");
const logs = [], browserErrors = [], browserConsole = [], steps = [], providerCalls = [];
let backend, vite, browser, page, model, mcp;
let capabilities = [], providerFails = false;
let browserPhase = "normal";
const goalText = "三入口共享真实目标与报告成果";
const reportText = "# Shared goal controlled report\n\nVerified local fixture artifact.\n";
const children = [];

function launch(command, args, options, label) {
  const child = spawn(command, args, { ...options, windowsHide: true, stdio: ["ignore", "pipe", "pipe"] });
  for (const stream of [child.stdout, child.stderr]) stream.on("data", chunk => logs.push(`[${label}] ${chunk}`));
  children.push(child);
  return child;
}
function startBackend() {
  return launch(backendBinary, [], { cwd: root, env: { ...process.env, YILIAN_DATA_DIR: dataDir, YILIAN_WORKSPACE: workspacePath, YILIAN_HOST: `127.0.0.1:${backendPort}`, YILIAN_ALLOWED_ORIGINS: frontend, YILIAN_CONTROL_SESSION_TOKEN: controlToken } }, "backend");
}
async function api(method, path, body, expected = 200) {
  const response = await fetch(`${base}${path}`, { method, headers: { "Content-Type": "application/json", "X-Yilian-Control-Session": controlToken }, body: body === undefined ? undefined : JSON.stringify(body) });
  const text = await response.text();
  assert.equal(response.status, expected, `${method} ${path}: ${text}`);
  return text ? JSON.parse(text) : null;
}
async function until(read, predicate, label, timeout = 30000) {
  const deadline = Date.now() + timeout;
  let value;
  while (Date.now() < deadline) {
    value = await read();
    if (predicate(value)) return value;
    await new Promise(resolveWait => setTimeout(resolveWait, 100));
  }
  throw new Error(`${label}: ${JSON.stringify(value)}`);
}
async function ready(url) {
  await until(async () => { try { return (await fetch(url)).ok; } catch { return false; } }, Boolean, `service ${url}`);
}
async function phase(graphId, expected, target = page) {
  const summary = target.locator(`.goal-tracking-summary[data-graph-id="${graphId}"]`).first();
  await summary.waitFor();
  await until(() => summary.getAttribute("data-phase"), value => value === expected, `UI ${graphId} ${expected}`);
  return summary;
}
async function visit(path) { await page.goto(`${frontend}${path}`, { waitUntil: "domcontentloaded" }); }
async function graphDetail(id) { return (await api("GET", `/api/task-world/graphs/${id}/detail`)).detail; }
async function approval(id) { return until(async () => (await api("GET", "/api/approvals/pending")).approvals.find(item => item.task_graph_id === id), Boolean, "approval persisted"); }
async function approve(pending) {
  const response = await fetch(`${base}/api/approvals/${pending.approval_id}/approve`, { method: "POST", headers: { "Content-Type": "application/json", "X-Yilian-Control-Session": controlToken }, body: JSON.stringify({ conversation_id: pending.conversation_id }) });
  assert.equal(response.status, 200, await response.clone().text());
  await response.text();
}
async function runNode(id) {
  const detail = await graphDetail(id);
  await api("POST", `/api/task-world/graphs/${id}/nodes/report/executions`, { expected_revision: detail.revision }, 201);
  return approval(id);
}
async function makeGoal(workspaceId, title) {
  const draft = await api("POST", "/api/task-world/proofloop/drafts", { workspace_id: workspaceId, user_goal: title, allowed_capability_ids: capabilities.map(item => item.id) }, 201);
  assert.equal(draft.compilation.status, "ready", JSON.stringify(draft.compilation));
  const confirmed = await api("POST", `/api/task-world/proofloop/drafts/${draft.draft_id}/confirm`, { expected_revision: draft.revision });
  return { draft, ...confirmed };
}

try {
  const schema = { type: "object", properties: { report: { type: "string" } }, required: ["report"] };
  mcp = createServer(async (request, response) => {
    const rpc = JSON.parse(await body(request) || "{}");
    let result = {};
    if (rpc.method === "server/discover") result = { supportedVersions: ["2026-07-28", "2025-11-25"], serverInfo: { name: "SHARED GOAL CONTROLLED MCP", version: "1" }, capabilities: { tools: true } };
    if (rpc.method === "tools/list") result = { tools: [{ name: "shared_report", description: "CONTROLLED report fixture", inputSchema: { type: "object", properties: {}, additionalProperties: false }, outputSchema: schema }] };
    if (rpc.method === "tools/call") {
      providerCalls.push({ at: Date.now(), fail: providerFails });
      result = providerFails ? { content: [{ type: "text", text: "CONTROLLED provider failure for recovery" }], isError: true } : { content: [{ type: "text", text: reportText }], structuredContent: { report: reportText }, isError: false, resultType: "complete" };
    }
    response.writeHead(200, { "Content-Type": "application/json" }); response.end(JSON.stringify({ jsonrpc: "2.0", id: rpc.id, result }));
  });
  await listen(mcp);
  model = createServer(async (request, response) => {
    await body(request);
    const content = {
      deliverables: [{ id: "report-file", title: "shared-goal-report.md", mime_type: "text/markdown" }],
      criteria: [{ id: "exists", description: "实际产物存在", required: true, check: { kind: "artifact_exists", deliverable_id: "report-file" } }, { id: "digest", description: "实际产物摘要一致", required: true, check: { kind: "artifact_digest_matches", deliverable_id: "report-file" } }],
      candidate: { nodes: [{ id: "report", title: "生成共享报告", instruction: "Produce a controlled report", capability_id: capabilities[0]?.id, literal_input: {}, bindings: [] }], edges: [], deliverable_nodes: [{ deliverable_id: "report-file", node_id: "report" }] },
    };
    response.writeHead(200, { "Content-Type": "application/json" });
    response.end(JSON.stringify({ id: "shared-goal-local-model", choices: [{ index: 0, finish_reason: "stop", message: { role: "assistant", content: JSON.stringify(content) } }], usage: { prompt_tokens: 20, completion_tokens: 20, total_tokens: 40 } }));
  });
  await listen(model);
  backend = startBackend();
  const viteArgs = process.env.YILIAN_SHARED_GOAL_BEHAVIOR === "1"
    ? ["--input-type=module", "-e", `import {createServer} from ${JSON.stringify(pathToFileURL(join(root, "frontend/node_modules/vite/dist/node/index.js")).href)}; const server=await createServer({root:${JSON.stringify(join(root, "frontend"))},server:{host:'127.0.0.1',port:${frontendPort},strictPort:true,watch:{ignored:['**/*']}}}); await server.listen();`]
    : [join(root, "frontend/node_modules/vite/bin/vite.js"), "--host", "127.0.0.1", "--port", String(frontendPort), "--strictPort"];
  vite = launch(process.execPath, viteArgs, { cwd: join(root, "frontend"), env: { ...process.env, VITE_API_BASE: base, VITE_CONTROL_SESSION_TOKEN: controlToken } }, "vite");
  await Promise.all([ready(`${base}/api/health`), ready(frontend)]);
  const workspace = await api("POST", "/api/workspaces", { name: "共享目标隔离验收", root_path: workspacePath });
  await api("POST", "/api/plugins/mcp", { name: "SHARED GOAL CONTROLLED MCP", transport: "streamable_http", url: `http://127.0.0.1:${mcp.address().port}/mcp` });
  await api("POST", "/api/capabilities/refresh");
  capabilities = (await api("GET", "/api/capabilities?provider=mcp&limit=200")).capabilities.filter(item => item.name === "shared_report");
  assert.equal(capabilities.length, 1);
  const profile = await api("POST", "/api/llm/models", { provider: "custom", label: "SHARED GOAL CONTROLLED MODEL", model: "shared-goal-loopback", base_url: `http://127.0.0.1:${model.address().port}`, api_format: "openai", api_key: "local-test-placeholder", api_key_env: "", temperature: 0, max_tokens: 2048, invoke_timeout_ms: 10000 }, 201);
  await api("POST", `/api/llm/models/${profile.id}/verify`); await api("POST", `/api/llm/models/${profile.id}/activate`);
  const goal = await makeGoal(workspace.id, goalText);
  const executablePath = ["C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe", "C:/Program Files/Microsoft/Edge/Application/msedge.exe", "C:/Program Files/Google/Chrome/Application/chrome.exe"].find(existsSync);
  assert.ok(executablePath, "system browser required");
  browser = await chromium.launch({ executablePath, headless: true });
  page = await browser.newPage({ viewport: { width: 1366, height: 768 }, acceptDownloads: true });
  page.on("pageerror", error => browserErrors.push(error.message));
  page.on("console", message => { if (["error", "warning", "warn"].includes(message.type())) browserConsole.push({ phase: browserPhase, type: message.type(), text: message.text() }); });
  await page.addInitScript(() => sessionStorage.setItem("yilian.setup.deferred", "true"));
  const conversation = await api("POST", "/api/conversations", { title: "共享目标任务对话" });
  await api("PUT", `/api/conversations/${conversation.id}/workspace`, { workspace_id: workspace.id });
  await visit(`/chat/${conversation.id}`);
  await page.getByRole("button", { name: "关联已有目标", exact: true }).click();
  await page.getByLabel("目标任务", { exact: true }).selectOption(goal.graph_id);
  await page.getByRole("button", { name: "确认关联", exact: true }).click();
  await phase(goal.graph_id, "ready");
  const persisted = await api("GET", `/api/conversations/${conversation.id}/goal`);
  assert.equal(persisted.binding.graph_id, goal.graph_id);
  await page.reload({ waitUntil: "domcontentloaded" }); await phase(goal.graph_id, "ready");
  steps.push("chat_ui_binding_and_refresh_use_persisted_goal");

  if (process.env.YILIAN_SHARED_GOAL_BEHAVIOR === "1") {
    const { runSharedGoalBehavior } = await import("./shared-goal-behavior-checks.mjs");
    const behavior = await runSharedGoalBehavior({ page, browser, api, until, phase, visit, graphDetail, makeGoal, runNode, approval, approve, goal, conversation, workspace, providerCalls, base, frontend, controlToken, workspacePath, evidenceDir, setProviderFailure(value) { providerFails = value; }, async stopBackend() { browserPhase = "intentional_backend_restart"; await stop(backend); }, async restartBackend() { backend = startBackend(); await ready(`${base}/api/health`); browserPhase = "normal"; } });
    const result = { status: "passed", boundary: "Real isolated backend/SQLite/commands/approvals/artifacts and application events; local controlled model/MCP; document visibility override and CDP offline are explicit browser lifecycle fixtures", backendBinary: { path: backendBinary, sha256: createHash("sha256").update(await readFile(backendBinary)).digest("hex") }, isolatedRoot, goal_id: goal.goal_id, graph_id: goal.graph_id, conversation_id: conversation.id, workspace_id: workspace.id, behavior, providerCalls, browserErrors, browserConsole };
    assert.deepEqual(browserErrors, []);
    await writeFile(join(evidenceDir, "result.json"), JSON.stringify(result, null, 2));
    console.log(JSON.stringify({ status: result.status, evidenceDir, backendBinary: result.backendBinary, steps: behavior.steps, providerCalls, noticeCount: behavior.notices.length, browserErrors }));
  } else {
  providerFails = true;
  await approve(await runNode(goal.graph_id));
  await until(() => graphDetail(goal.graph_id), detail => detail.nodes[0].latest_execution?.status === "failed", "failed provider recorded");
  const failedSummary = await phase(goal.graph_id, "failed");
  await failedSummary.locator("dd a").first().click();
  await page.waitForURL(/\/task-world\/.*\?node=report/);
  await phase(goal.graph_id, "failed");
  await page.getByRole("button", { name: "专家视图", exact: true }).click();
  await page.locator("summary").filter({ hasText: "状态与执行记录" }).click();
  await page.screenshot({ path: join(evidenceDir, "canvas-failed-recovery.png"), fullPage: true });
  await page.getByRole("button", { name: "从此节点重跑", exact: true }).click();
  await until(() => graphDetail(goal.graph_id), detail => ["pending", "runnable", "invalidated"].includes(detail.nodes[0].status), "node prepared for recovery");
  await page.getByRole("button", { name: "运行选中节点", exact: true }).click();
  providerFails = false;
  await approve(await approval(goal.graph_id));
  const succeeded = await until(() => graphDetail(goal.graph_id), detail => detail.nodes[0].latest_execution?.status === "succeeded", "failed node recovery");
  assert.equal(succeeded.nodes[0].latest_execution.attempt, 2);
  steps.push("failed_execution_located_in_canvas_and_recovered_with_history");
  const summaryBeforeArtifact = page.locator(`.goal-tracking-summary[data-graph-id="${goal.graph_id}"]`).first();
  await until(() => summaryBeforeArtifact.getAttribute("data-phase"), value => ["failed", "verifying"].includes(value), "settled execution without artifact");
  assert.notEqual(await summaryBeforeArtifact.getAttribute("data-phase"), "completed", "no artifact must never complete");
  await api("POST", "/api/artifacts/from-execution", { source: { kind: "node", graph_id: goal.graph_id, node_id: "report", execution_id: succeeded.nodes[0].latest_execution.execution_id, workspace_id: workspace.id }, name: "shared-goal-report.md" }, 201);
  await page.locator(`.goal-tracking-summary[data-graph-id="${goal.graph_id}"]`).first().getByRole("button", { name: "刷新状态", exact: true }).click();
  await phase(goal.graph_id, "completed");
  steps.push("completion_requires_registered_actual_artifact");

  await visit(`/chat/${conversation.id}`);
  const completedChat = await phase(goal.graph_id, "completed");
  await completedChat.getByRole("button", { name: "任务详情与成果", exact: true }).click();
  const downloadEvent = page.waitForEvent("download");
  await page.getByRole("dialog", { name: "任务详情与成果", exact: true }).getByRole("button", { name: "下载真实文件", exact: true }).click();
  const download = await downloadEvent;
  const downloaded = join(evidenceDir, "shared-goal-downloaded-artifact.md"); await download.saveAs(downloaded);
  assert.ok((await readFile(downloaded, "utf8")).includes("Verified local fixture artifact"));
  await page.getByRole("button", { name: "关闭任务详情与成果", exact: true }).click();
  await completedChat.getByRole("link", { name: "工作空间", exact: true }).click();
  await phase(goal.graph_id, "completed");
  assert.equal(await page.getByLabel("目标任务", { exact: true }).inputValue(), goal.graph_id);
  steps.push("chat_canvas_workspace_share_graph_and_download_real_result");

  const cancelledGoal = await makeGoal(workspace.id, "取消后不会调用受控提供方");
  const pending = await runNode(cancelledGoal.graph_id);
  const callsBeforeCancel = providerCalls.length;
  await visit(`/task-world/${cancelledGoal.graph_id}`);
  const pendingSummary = await phase(cancelledGoal.graph_id, "waiting_approval");
  assert.ok((await pendingSummary.innerText()).includes("需要决定"));
  await pendingSummary.getByRole("button", { name: "取消任务", exact: true }).click();
  await phase(cancelledGoal.graph_id, "cancelled");
  const cancelled = await graphDetail(cancelledGoal.graph_id);
  assert.equal(cancelled.control_state, "cancelled");
  const approvalAfterCancel = await fetch(`${base}/api/approvals/${pending.approval_id}/approve`, { method: "POST", headers: { "Content-Type": "application/json", "X-Yilian-Control-Session": controlToken }, body: "{}" });
  assert.notEqual(approvalAfterCancel.status, 200);
  assert.equal(providerCalls.length, callsBeforeCancel);
  steps.push("graph_cancellation_revokes_pending_approval_without_provider_call");

  await visit(`/workspaces/${workspace.id}?tab=tasks&graph=${goal.graph_id}`); await phase(goal.graph_id, "completed");
  await page.getByLabel("目标任务", { exact: true }).selectOption(cancelledGoal.graph_id);
  await phase(cancelledGoal.graph_id, "cancelled");
  await page.reload({ waitUntil: "domcontentloaded" }); await phase(cancelledGoal.graph_id, "cancelled");
  assert.equal(new URL(page.url()).searchParams.get("graph"), cancelledGoal.graph_id);
  steps.push("workspace_task_selection_survives_refresh_through_url");

  browserPhase = "intentional_backend_restart";
  await stop(backend); backend = startBackend(); await ready(`${base}/api/health`);
  await visit(`/chat/${conversation.id}`); await phase(goal.graph_id, "completed");
  browserPhase = "normal";
  assert.equal((await api("GET", `/api/conversations/${conversation.id}/goal`)).binding.graph_id, goal.graph_id);
  await visit(`/task-world/${cancelledGoal.graph_id}`); await phase(cancelledGoal.graph_id, "cancelled");
  assert.equal(providerCalls.length, callsBeforeCancel);
  steps.push("backend_restart_preserves_binding_result_and_cancelled_control");

  const slowGoal = await makeGoal(workspace.id, "慢接口不会阻断正在执行任务的追踪");
  await runNode(slowGoal.graph_id);
  const delayedGoalPattern = `**/api/task-world/proofloop/graphs/${slowGoal.graph_id}/goal`;
  let delayedGoalRequests = 0, delayedGoalResponses = 0, abortedGoalRequests = 0;
  const onRequestFailed = request => {
    if (new URL(request.url()).pathname === `/api/task-world/proofloop/graphs/${slowGoal.graph_id}/goal`) abortedGoalRequests++;
  };
  const delayedGoalHandler = async route => {
    delayedGoalRequests++;
    const realResponse = await route.fetch();
    await new Promise(resolveDelay => setTimeout(resolveDelay, 3500));
    await route.fulfill({ response: realResponse });
    delayedGoalResponses++;
  };
  page.on("requestfailed", onRequestFailed);
  await page.route(delayedGoalPattern, delayedGoalHandler);
  await visit(`/task-world/${slowGoal.graph_id}`);
  const slowSummary = await phase(slowGoal.graph_id, "waiting_approval");
  assert.ok(delayedGoalResponses > 0, "the slow goal response must finish");
  assert.ok(abortedGoalRequests <= 1, `slow real goal API was repeatedly aborted: ${abortedGoalRequests}`);
  await page.unroute(delayedGoalPattern, delayedGoalHandler);
  page.off("requestfailed", onRequestFailed);
  await slowSummary.getByRole("button", { name: "取消任务", exact: true }).click();
  await phase(slowGoal.graph_id, "cancelled");
  await visit(`/task-world/${goal.graph_id}`); await phase(goal.graph_id, "completed");
  await page.screenshot({ path: join(evidenceDir, "canvas-shared-goal.png"), fullPage: true });
  steps.push("slow_real_goal_response_over_poll_interval_finishes_without_repeated_abort");

  const geometry = [];
  for (const width of [1366, 960, 390]) {
    await page.setViewportSize({ width, height: 768 });
    await visit(`/chat/${conversation.id}`); await phase(goal.graph_id, "completed");
    const dimensions = await page.evaluate(() => ({ scrollWidth: document.documentElement.scrollWidth, clientWidth: document.documentElement.clientWidth, scrollHeight: document.documentElement.scrollHeight, clientHeight: document.documentElement.clientHeight }));
    assert.ok(dimensions.scrollWidth <= dimensions.clientWidth + 2, `chat horizontal overflow ${width}: ${JSON.stringify(dimensions)}`);
    geometry.push({ width, ...dimensions });
    await page.screenshot({ path: join(evidenceDir, `chat-${width}.png`), fullPage: true });
  }
  await page.setViewportSize({ width: 1366, height: 768 });
  await visit(`/workspaces/${workspace.id}?tab=tasks&graph=${goal.graph_id}`); await phase(goal.graph_id, "completed");
  await page.screenshot({ path: join(evidenceDir, "workspace-shared-goal.png"), fullPage: true });
  assert.deepEqual(browserErrors, []);
  const unexpectedConsoleErrors = browserConsole.filter(item => item.type === "error" && !(item.phase === "intentional_backend_restart" && /ERR_CONNECTION_(RESET|REFUSED)|Failed to fetch/.test(item.text)));
  assert.deepEqual(unexpectedConsoleErrors, [], "unexpected browser console errors");
  const result = { status: "passed", boundary: "Real isolated backend/SQLite/browser/artifacts; local controlled model and MCP fixtures, no external model claims; one real goal response delayed 3500ms without changing its data", backendBinary: { path: backendBinary, sha256: createHash("sha256").update(await readFile(backendBinary)).digest("hex") }, isolatedRoot, goal_id: goal.goal_id, graph_id: goal.graph_id, conversation_id: conversation.id, workspace_id: workspace.id, steps, providerCalls, slowGoal: { delayedGoalRequests, delayedGoalResponses, abortedGoalRequests }, geometry, browserErrors, browserConsole, unexpectedConsoleErrors };
  await writeFile(join(evidenceDir, "result.json"), JSON.stringify(result, null, 2));
  console.log(JSON.stringify(result));
  }
} catch (error) {
  await writeFile(join(evidenceDir, "browser-diagnostics.json"), JSON.stringify({ browserErrors, browserConsole }, null, 2));
  await writeFile(join(evidenceDir, "failure.txt"), String(error.stack ?? error));
  if (page) { await page.screenshot({ path: join(evidenceDir, "failure.png"), fullPage: true }).catch(() => {}); await writeFile(join(evidenceDir, "failure-page.txt"), `${page.url()}\n${await page.locator("body").innerText().catch(() => "")}`); }
  throw error;
} finally {
  await browser?.close();
  for (const child of children.reverse()) await stop(child);
  for (const server of [mcp, model]) if (server) { server.closeAllConnections(); await new Promise(resolveClose => server.close(resolveClose)); }
  await writeFile(join(evidenceDir, "services.log"), logs.join(""));
}

async function freePort() { const server = createTcpServer(); await new Promise(resolveListen => server.listen(0, "127.0.0.1", resolveListen)); const port = server.address().port; await new Promise(resolveClose => server.close(resolveClose)); return port; }
async function listen(server) { await new Promise(resolveListen => server.listen(0, "127.0.0.1", resolveListen)); }
async function body(request) { let content = ""; for await (const chunk of request) content += chunk; return content; }
async function stop(child) { if (!child || child.exitCode !== null) return; child.kill(); await Promise.race([new Promise(resolveExit => child.once("exit", resolveExit)), new Promise(resolveWait => setTimeout(resolveWait, 3000))]); if (child.exitCode === null) child.kill("SIGKILL"); }
