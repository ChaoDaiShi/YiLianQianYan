import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { existsSync } from "node:fs";
import { mkdir, mkdtemp, readFile, writeFile } from "node:fs/promises";
import { createServer } from "node:http";
import { createServer as createTcpServer } from "node:net";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { chromium } from "playwright-core";

// All product HTTP, SQLite, execution, approval and file APIs below are real.
// Model planning and the remote MCP provider are explicit local controlled fixtures.
const root = resolve(import.meta.dirname, "../..");
const isolatedRoot = await mkdtemp(join(tmpdir(), "yilian-integration-e2e-"));
const evidenceDir = process.env.YILIAN_INTEGRATION_EVIDENCE || join(isolatedRoot, "evidence");
await mkdir(evidenceDir, { recursive: true });
console.log(`[integration] evidence ${evidenceDir}`);
const dataDir = join(isolatedRoot, "data"), workspacePath = join(isolatedRoot, "workspace");
await Promise.all([mkdir(dataDir), mkdir(workspacePath)]);
const backendPort = await freePort(), frontendPort = await freePort();
const base = `http://127.0.0.1:${backendPort}`, frontend = `http://127.0.0.1:${frontendPort}`;
const controlToken = "shared-goal-local-controlled-token-".padEnd(64, "x");
const backendBinary = process.env.YILIAN_E2E_BACKEND_BINARY ?? join(root, "target/debug/yilian-server.exe");
const logs = [], browserErrors = [], browserConsole = [], steps = [], providerCalls = [];
let backend, vite, browser, page, model, mcp;
const failedRequests = [], geometry = [], failedResponses = [];
const expectedFailures = new Map();
let modelCalls = 0;
let capabilities = [], providerFails = false;
let browserPhase = "normal";
const goalText = "三入口共享真实目标与报告成果";
const reportText = "# Shared goal controlled report\n\nVerified local fixture artifact.\n";
const children = [];
function completedStep(...messages) { steps.push(...messages); for (const message of messages) console.log(`[integration] ${message}`); }

function launch(command, args, options, label) {
  const child = spawn(command, args, { ...options, windowsHide: true, stdio: ["ignore", "pipe", "pipe"] });
  for (const stream of [child.stdout, child.stderr]) stream.on("data", chunk => logs.push(`[${label}] ${chunk}`));
  children.push(child);
  return child;
}
function startBackend() {
  return launch(backendBinary, [], { cwd: root, env: { ...process.env, YILIAN_DATA_DIR: dataDir, YILIAN_WORKSPACE: workspacePath, YILIAN_HOST: `127.0.0.1:${backendPort}`, YILIAN_ALLOWED_ORIGINS: frontend, YILIAN_CONTROL_SESSION_TOKEN: controlToken, YILIAN_INTEGRATION_FIXTURE_KEY: "controlled-local-test-value" } }, "backend");
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
    try {
      const input = JSON.parse(await body(request) || "{}"); modelCalls++;
      if (input.stream) {
        const tools = input.tools || [];
        const goals = input.messages.filter(item => item.role === "user").map(item => item.content).join("\n");
        const personal = goals.includes("INTEGRATION_PERSONAL");
        const lastUser = input.messages.findLastIndex(item => item.role === "user");
        const prior = input.messages.slice(lastUser + 1).filter(item => item.role === "tool");
        let tool;
        if (personal) {
          assert.ok(!tools.some(item => item.function.name === "bash"));
          if (!prior.length) tool = ["write_file", { path: join(workspacePath, "personal-report.md"), content: "# Personal integration report\nControlled new file.\n" }];
          else if (prior.length === 1) tool = ["read_file", { path: join(workspacePath, "personal-report.md") }];
        }
        const delta = tool ? { role: "assistant", tool_calls: [{ index: 0, id: `integration-tool-${modelCalls}`, type: "function", function: { name: tool[0], arguments: JSON.stringify(tool[1]) } }] } : { content: personal ? "个人报告已创建并读回核验。" : "```text\nControlled conversation output.\n```" };
        response.writeHead(200, { "Content-Type": "text/event-stream" });
        response.end(`data: ${JSON.stringify({ choices: [{ index: 0, delta, finish_reason: null }] })}\n\ndata: ${JSON.stringify({ choices: [{ index: 0, delta: {}, finish_reason: tool ? "tool_calls" : "stop" }], usage: { prompt_tokens: 20, completion_tokens: 20, total_tokens: 40 } })}\n\ndata: [DONE]\n\n`); return;
      }
      const content = { deliverables: [{ id: "report-file", title: "shared-goal-report.md", mime_type: "text/markdown" }], criteria: [{ id: "exists", description: "actual artifact exists", required: true, check: { kind: "artifact_exists", deliverable_id: "report-file" } }, { id: "digest", description: "actual digest matches", required: true, check: { kind: "artifact_digest_matches", deliverable_id: "report-file" } }], candidate: { nodes: [{ id: "report", title: "controlled integration report", instruction: "Produce a controlled report", capability_id: capabilities[0]?.id, literal_input: {}, bindings: [] }], edges: [], deliverable_nodes: [{ deliverable_id: "report-file", node_id: "report" }] } };
      response.writeHead(200, { "Content-Type": "application/json" }); response.end(JSON.stringify({ id: "integration-local-model", choices: [{ index: 0, finish_reason: "stop", message: { role: "assistant", content: JSON.stringify(content) } }], usage: { prompt_tokens: 20, completion_tokens: 20, total_tokens: 40 } }));
    } catch (error) { browserErrors.push(`model fixture: ${error.stack || error}`); response.writeHead(500); response.end("controlled fixture failed"); }
  });
  await listen(model);
  backend = startBackend();
  vite = launch(process.execPath, [join(root, "frontend/node_modules/vite/bin/vite.js"), "--host", "127.0.0.1", "--port", String(frontendPort), "--strictPort"], { cwd: join(root, "frontend"), env: { ...process.env, VITE_API_BASE: base, VITE_CONTROL_SESSION_TOKEN: controlToken } }, "vite");
  await Promise.all([ready(`${base}/api/health`), ready(frontend)]);
  const settings = await api("GET", "/api/settings");
  await api("PUT", "/api/settings", { ...settings, model: { ...settings.model, provider: "openai", name: "integration-local-fixture", base_url: `http://127.0.0.1:${model.address().port}`, api_key_env: "YILIAN_INTEGRATION_FIXTURE_KEY", api_key_ref: null, temperature: 0, max_tokens: 2048, invoke_timeout_ms: 10000 } });
  const workspace = await api("POST", "/api/workspaces", { name: "内部一体化验收", root_path: workspacePath });
  const otherPath = join(workspacePath, "other-space"); await mkdir(otherPath);
  const otherSpace = await api("POST", "/api/workspaces", { name: "隔离的另一空间", root_path: otherPath });
  await api("POST", "/api/plugins/mcp", { name: "SHARED GOAL CONTROLLED MCP", transport: "streamable_http", url: `http://127.0.0.1:${mcp.address().port}/mcp` });
  await api("POST", "/api/capabilities/refresh");
  capabilities = (await api("GET", "/api/capabilities?provider=mcp&limit=200")).capabilities.filter(item => item.name === "shared_report");
  assert.equal(capabilities.length, 1);
  assert.equal(capabilities[0].availability.personal.executable, false);
  const goal = await makeGoal(workspace.id, "INTEGRATION_GOAL no frontend observer");
  // No browser or frontend observer exists while execution and artifact acceptance settle.
  const goalPending = await runNode(goal.graph_id); await approve(goalPending);
  const goalDetail = await until(() => graphDetail(goal.graph_id), value => value.nodes[0].latest_execution?.status === "succeeded", "goal succeeds with browser closed");
  const goalExecution = goalDetail.nodes[0].latest_execution.execution_id;
  await api("POST", "/api/artifacts/from-execution", { source: { kind: "node", graph_id: goal.graph_id, node_id: "report", execution_id: goalExecution, workspace_id: workspace.id }, name: "shared-goal-report.md" }, 201);
  const reportsBefore = await until(() => api("GET", `/api/task-world/proofloop/goals/${goal.goal_id}/evaluations`), value => value.reports.some(item => item.status === "pass"), "backend acceptance without frontend");
  await api("POST", "/api/work-index/reconcile", {}); await api("POST", "/api/work-index/reconcile", {});
  const reportsAfter = await api("GET", `/api/task-world/proofloop/goals/${goal.goal_id}/evaluations`);
  assert.equal(reportsAfter.reports.length, reportsBefore.reports.length, "repeated reconciliation must be idempotent");
  completedStep("backend produces persisted acceptance without frontend; repeated reconciliation does not duplicate reports");

  const workflow = await api("POST", "/api/workflow-graphs", { name: "审批父子关联验收", definition: { schema_version: 1, entry_node_id: "read", nodes: [{ id: "read", kind: "tool", config: { type: "tool", tool_name: capabilities[0].metadata.source_id, arguments: {} } }, { id: "out", kind: "output", config: { type: "output", template: "workflow controlled completed" } }], edges: [{ from: "read", to: "out" }] } });
  async function makeWorkflowParent(id) {
    await api("POST", "/api/task-world/graphs", { id, nodes: [{ id: "child", kind: "work", title: "工作流父任务", input: { executor_ref: `workflow://${workflow.id}` }, retry_policy: { max_attempts: 1 } }], edges: [] }, 201);
    const detail = await graphDetail(id);
    const started = await api("POST", `/api/task-world/graphs/${id}/nodes/child/executions`, { expected_revision: detail.revision }, 201);
    const waiting = await until(() => graphDetail(id), value => value.nodes[0].latest_execution?.status === "waiting_approval", "parent waits for child approval");
    const runs = (await api("GET", `/api/workflow-runs?workflow_graph_id=${workflow.id}`)).runs;
    const pending = await until(() => api("GET", "/api/approvals/pending"), value => value.approvals.some(item => runs.some(run => run.run_id === item.workflow_run_id)), "workflow pending approval");
    return { execution: started.execution, waiting, pending: pending.approvals.find(item => runs.some(run => run.run_id === item.workflow_run_id)) };
  }
  const parentId = `integration-parent-${Date.now()}`;
  const parent = await makeWorkflowParent(parentId);
  const workflowCalls = providerCalls.length;
  await approve(parent.pending);
  await until(() => graphDetail(parentId), value => value.nodes[0].latest_execution?.status === "succeeded", "approval resumes original parent");
  assert.equal(providerCalls.length, workflowCalls + 1);
  const workflowManifest = await api("GET", `/api/workflow-runs/${parent.pending.workflow_run_id}/context-manifest`);
  assert.equal(workflowManifest.owner.run_id, parent.pending.workflow_run_id);
  assert.equal(workflowManifest.manifest.schema_version, 1);
  assert.ok(Array.isArray(workflowManifest.manifest.entries));
  const replay = await fetch(`${base}/api/approvals/${parent.pending.approval_id}/approve`, { method: "POST", headers: { "Content-Type": "application/json", "X-Yilian-Control-Session": controlToken }, body: JSON.stringify({ conversation_id: parent.pending.conversation_id }) });
  const replayBody = await replay.text();
  if (replay.status === 200) {
    assert.ok(replayBody.includes("workflow_approval_error"), "SSE must report the already consumed approval");
    assert.ok(!replayBody.includes('"type":"approval_resolved"'), "replay must not claim a new decision");
  } else assert.equal(replay.status, 409);
  assert.equal(providerCalls.length, workflowCalls + 1);
  const cancelledParentId = `integration-cancel-${Date.now()}`;
  const cancelledParent = await makeWorkflowParent(cancelledParentId);
  await api("POST", `/api/task-world/graphs/${cancelledParentId}/executions/${cancelledParent.execution.execution_id}/cancel`, { expected_revision: cancelledParent.waiting.revision });
  const afterCancel = await fetch(`${base}/api/approvals/${cancelledParent.pending.approval_id}/approve`, { method: "POST", headers: { "Content-Type": "application/json", "X-Yilian-Control-Session": controlToken }, body: "{}" });
  const cancelledApprovalBody = await afterCancel.text();
  if (afterCancel.status === 200) {
    assert.ok(cancelledApprovalBody.includes("workflow_approval_error"));
    assert.ok(!cancelledApprovalBody.includes('"type":"approval_resolved"'));
  } else assert.ok([404, 409].includes(afterCancel.status));
  assert.equal(providerCalls.length, workflowCalls + 1);
  completedStep("workflow child approval preserves waiting parent, resumes the same execution once, and parent cancellation revokes pending child approval");

  const grants = [];
  for (const permission_id of ["filesystem.read", "filesystem.write"]) grants.push(await api("POST", "/api/security/grants", { permission_id, effect: "allow", resource: { type: "filesystem", root: workspacePath, recursive: true } }));
  const personal = (await api("POST", "/api/automations", { name: "一体化个人委托", workflow_graph_id: "", workspace_id: workspace.id, target: { kind: "personal_assistant", goal: "INTEGRATION_PERSONAL create and verify personal-report.md", allow_file_writes: true, max_runs: 5 }, timezone: "UTC", schedule: { kind: "daily", time: "09:00" }, enabled: false }, 201)).automation;
  await api("POST", `/api/automations/${personal.id}/personal/start`, { expected_revision: personal.revision, request_id: crypto.randomUUID() });
  await until(() => api("GET", `/api/automations/${personal.id}/personal`), value => value.activity?.latest?.status === "completed", "personal report completed");
  const conversation = await api("POST", "/api/conversations", { title: "一体化对话" });
  await api("PUT", `/api/conversations/${conversation.id}/workspace`, { workspace_id: workspace.id });
  const chat = await fetch(`${base}/api/chat`, { method: "POST", headers: { "Content-Type": "application/json", "X-Yilian-Control-Session": controlToken }, body: JSON.stringify({ conversation_id: conversation.id, message: "INTEGRATION_CHAT produce controlled code block" }) });
  assert.equal(chat.status, 200); await chat.text();
  await until(() => api("GET", `/api/conversations/${conversation.id}`), value => value.run_status === "completed", "chat completed");
  const candidates = await api("GET", `/api/conversations/${conversation.id}/outputs/code-candidates`); assert.ok(candidates.candidates.length);
  await api("POST", `/api/conversations/${conversation.id}/outputs/from-message`, { message_id: candidates.candidates[0].message_id, block_index: candidates.candidates[0].block_index, previous_output_id: null });
  const legacy = await api("POST", "/api/tasks", { workspace_id: workspace.id, title: "保留原任务管理" });
  const legacyId = legacy.task?.id || legacy.id;
  const index = await api("GET", "/api/work-index?limit=200");
  for (const kind of ["conversation", "legacy_task", "task_graph", "personal_delegation"]) assert.ok(index.items.some(item => item.kind === kind), kind);
  assert.ok(!index.items.some(item => item.kind === "conversation" && item.id === personal.target.conversation_id));
  const outputs = await api("GET", `/api/outputs?workspace_id=${workspace.id}&limit=100`);
  for (const kind of ["artifact", "conversation", "personal_file"]) assert.ok(outputs.outputs.some(item => item.kind === kind), kind);
  const personalOutput = outputs.outputs.find(item => item.kind === "personal_file");
  const previewPath = `/api/outputs/personal_file/${encodeURIComponent(personalOutput.id)}/preview?source_id=${personal.id}&workspace_id=${workspace.id}`;
  assert.ok((await api("GET", previewPath)).text.includes("Controlled new file"));
  const scopedDenied = await fetch(`${base}/api/outputs/personal_file/${encodeURIComponent(personalOutput.id)}/preview?source_id=${personal.id}&workspace_id=${otherSpace.id}`, { headers: { "X-Yilian-Control-Session": controlToken } }); assert.ok(!scopedDenied.ok); await scopedDenied.text();
  completedStep("shared index covers all four work sources without personal-chat duplication; catalogue includes all three real output chains and rejects another workspace");

  const executablePath = ["C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe", "C:/Program Files/Microsoft/Edge/Application/msedge.exe"].find(existsSync); assert.ok(executablePath);
  browser = await chromium.launch({ executablePath, headless: true });
  page = await browser.newPage({ viewport: { width: 1366, height: 768 } });
  page.on("pageerror", error => browserErrors.push(error.message));
  page.on("console", message => { if (["error", "warning", "warn"].includes(message.type())) browserConsole.push({ phase: browserPhase, type: message.type(), text: message.text(), location: message.location() }); });
  page.on("requestfailed", request => {
    const record = { phase: browserPhase, url: request.url(), error: request.failure()?.errorText }; failedRequests.push(record);
    const allowedRestartPaths = ["/api/events", "/api/work-index?limit=200", "/api/presence", "/api/voice/session"];
    if (browserPhase === "intentional_backend_restart" && allowedRestartPaths.some(path => record.url === `${base}${path}`) && /^net::ERR_CONNECTION_(RESET|REFUSED|CLOSED)$/.test(record.error || "")) expectedFailures.set(record.url, { phase: browserPhase, error: record.error });
  });
  page.on("response", response => { if (response.status() >= 400) failedResponses.push({ phase: browserPhase, url: response.url(), status: response.status() }); });
  await page.addInitScript(() => sessionStorage.setItem("yilian.setup.deferred", "true"));
  await visit("/tasks"); await page.getByRole("region", { name: "统一工作索引" }).waitFor();
  for (const kind of ["conversation", "legacy_task", "task_graph", "personal_delegation"]) await page.locator(`[data-work-kind="${kind}"]`).first().waitFor();
  await page.getByRole("button", { name: "任务计划", exact: true }).click();
  const planningDialog = page.getByRole("dialog", { name: /^任务计划/ });
  await planningDialog.waitFor();
  assert.equal(await page.getByRole("dialog").count(), 1);
  await page.screenshot({ animations: "disabled", path: join(evidenceDir, "planning-entry.png") });
  await planningDialog.getByRole("button", { name: "新建任务画布", exact: true }).click();
  await page.waitForURL(/\/task-world\/[^/?]+$/);
  const createdGraphId = new URL(page.url()).pathname.split("/").pop();
  expectedFailures.set(`${base}/api/task-world/proofloop/graphs/${createdGraphId}/goal`, { phase: "normal", status: 404 });
  assert.equal((await graphDetail(createdGraphId)).nodes.length, 0);
  await visit("/tasks");
  await page.locator(`[data-work-kind="conversation"][data-work-id="${conversation.id}"]`).waitFor();
  completedStep("original planning entry uses one dialog and creates a real empty task graph without calling a model");
  const chatRow = page.locator(`[data-work-kind="conversation"][data-work-id="${conversation.id}"]`); await chatRow.getByRole("button", { name: "上下文来源" }).click();
  await page.getByRole("dialog", { name: "上下文来源" }).getByRole("region", { name: "执行上下文来源" }).waitFor(); await page.screenshot({ animations: "disabled", path: join(evidenceDir, "context-manifest.png") });
  await page.getByRole("dialog").getByRole("button", { name: /^关闭/ }).click();
  await visit(`/tasks?task=${legacyId}`); await page.getByRole("dialog").getByText("保留原任务管理", { exact: true }).first().waitFor();
  await page.screenshot({ animations: "disabled", path: join(evidenceDir, "legacy-detail.png") });
  await visit(`/automations?assistant=${personal.id}`); await page.getByRole("dialog", { name: /个人助手详情/ }).waitFor();
  completedStep("task centre reads unified sources; original task and personal assistant deep links preserve management; context source manifest is visible");
  await visit(`/tasks?workspace=${workspace.id}`); await page.getByRole("button", { name: "全部成果", exact: true }).click();
  const outputDialog = page.getByRole("dialog", { name: "统一成果" });
  for (const kind of ["artifact", "conversation", "personal_file"]) await outputDialog.locator(`[data-output-kind="${kind}"]`).first().waitFor();
  await outputDialog.locator('[data-output-kind="personal_file"]').getByRole("button", { name: "查看成果" }).click();
  await outputDialog.getByTestId("unified-output-preview").getByText(/Controlled new file/).waitFor();
  assert.equal(await page.getByRole("dialog").count(), 1, "preview reuses parent dialog");
  for (const width of [1366, 960, 390]) {
    await page.setViewportSize({ width, height: 768 });
    const dimensions = await page.evaluate(() => ({ width: document.documentElement.clientWidth, scroll: document.documentElement.scrollWidth, dialogs: [...document.querySelectorAll('[role="dialog"]')].map(item => ({ width: item.clientWidth, scroll: item.scrollWidth })) }));
    assert.ok(dimensions.scroll <= dimensions.width + 2); for (const dialog of dimensions.dialogs) assert.ok(dialog.scroll <= dialog.width + 2);
    geometry.push({ viewport: width, ...dimensions }); await page.screenshot({ animations: "disabled", path: join(evidenceDir, `unified-preview-${width}.png`) });
  }
  browserPhase = "intentional_preview_denial";
  expectedFailures.set(`${base}${previewPath}`, { phase: browserPhase, status: 400 });
  // The isolated workspace also receives a migration grant at bootstrap.
  // Revoke every read allowance for this fixture root, rather than one duplicate.
  const readGrants = (await api("GET", "/api/security/grants")).grants.filter(grant => grant.permission === "filesystem.read" && grant.effect === "allow" && grant.resource.type === "filesystem" && grant.resource.root === workspacePath);
  assert.ok(readGrants.length > 0);
  for (const grant of readGrants) await api("DELETE", `/api/security/grants/${grant.id}`);
  const revokedPreview = await fetch(`${base}${previewPath}`, { headers: { "X-Yilian-Control-Session": controlToken } });
  assert.equal(revokedPreview.status, 400, await revokedPreview.text());
  await outputDialog.getByRole("button", { name: "返回成果列表" }).click(); await outputDialog.locator('[data-output-kind="personal_file"]').getByRole("button", { name: "查看成果" }).click();
  await outputDialog.getByRole("alert").getByText("成果暂时无法预览", { exact: true }).waitFor(); await page.screenshot({ animations: "disabled", path: join(evidenceDir, "revoked-preview.png") });
  await api("POST", "/api/security/grants", { permission_id: "filesystem.read", effect: "allow", resource: { type: "filesystem", root: workspacePath, recursive: true } });
  await outputDialog.getByRole("button", { name: "重新检查权限与文件" }).click(); await outputDialog.getByTestId("unified-output-preview").getByText(/Controlled new file/).waitFor();
  browserPhase = "normal";
  completedStep("one unified output dialog displays real personal file across three widths and rejects revoked read authority before recovering");
  await page.setViewportSize({ width: 1366, height: 768 }); await visit(`/workspaces/${workspace.id}`); await page.getByRole("tab", { name: "成果", exact: true }).click(); await page.locator('[data-output-kind="personal_file"]').waitFor();
  await page.screenshot({ animations: "disabled", path: join(evidenceDir, "workspace-unified-outputs.png") });
  await visit("/capabilities?category=catalog"); await page.getByRole("region", { name: "按执行环境可用性" }).waitFor(); await page.screenshot({ animations: "disabled", path: join(evidenceDir, "capability-availability.png") });
  completedStep("workspace uses the same output catalogue and capability detail displays runtime-context availability without granting authority");
  await visit("/tasks"); browserPhase = "intentional_index_failure";
  await page.route("**/api/work-index?*", route => { expectedFailures.set(route.request().url(), { phase: browserPhase, error: "net::ERR_FAILED" }); return route.abort("failed"); }); await page.getByRole("button", { name: "刷新统一工作" }).click(); await page.getByText(/工作索引读取失败/).waitFor();
  await page.screenshot({ animations: "disabled", path: join(evidenceDir, "work-index-error.png") }); await page.unroute("**/api/work-index?*"); await page.getByRole("button", { name: "重试工作索引" }).click(); await page.locator('[data-work-kind="personal_delegation"]').waitFor(); browserPhase = "normal";
  const callsBeforeRestart = { model: modelCalls, provider: providerCalls.length };
  browserPhase = "intentional_backend_restart"; await stop(backend); backend = startBackend(); await ready(`${base}/api/health`); await api("POST", "/api/work-index/reconcile", {}); await visit("/tasks"); await page.locator('[data-work-kind="personal_delegation"]').waitFor(); browserPhase = "normal";
  assert.equal(modelCalls, callsBeforeRestart.model); assert.equal(providerCalls.length, callsBeforeRestart.provider);
  assert.equal((await api("GET", `/api/task-world/proofloop/goals/${goal.goal_id}/evaluations`)).reports.length, reportsAfter.reports.length);
  completedStep("index failures stay explicit and retry; restart reconstructs shared state without replaying model or provider side effects");
  const pageIdentity = { url: page.url(), title: await page.title(), bodyChars: (await page.locator("body").innerText()).trim().length, frameworkOverlays: await page.locator("vite-error-overlay").count() };
  assert.equal(new URL(pageIdentity.url).pathname, "/tasks");
  assert.ok(pageIdentity.title.trim() && pageIdentity.bodyChars > 100);
  assert.equal(pageIdentity.frameworkOverlays, 0);
  assert.deepEqual(browserErrors, []);
  const unexpectedConsoleErrors = browserConsole.filter(item => {
    if (item.type !== "error") return false;
    const url = item.location?.url || "";
    const deniedFont = url.startsWith("https://fonts.googleapis.com/") && /net::ERR_NETWORK_ACCESS_DENIED/.test(item.text) && failedRequests.some(request => request.url === url && request.error === "net::ERR_NETWORK_ACCESS_DENIED");
    if (deniedFont) return false;
    const expected = expectedFailures.get(url);
    if (!expected || expected.phase !== item.phase) return true;
    if (expected.error) return !item.text.includes(expected.error);
    if (expected.status) return !new RegExp(`status of ${expected.status}\\b`).test(item.text);
    return true;
  });
  assert.deepEqual(unexpectedConsoleErrors, []);
  const result = { status: "passed", boundary: "Real isolated HTTP/SQLite/Gateway/approvals/files/UI; local deterministic model and MCP fixtures; no real accounts, external model, credentials or test metadata SQL inserts", isolatedRoot, evidenceDir, steps, modelCalls, providerCalls, geometry, pageIdentity, browserAvailability: "Browser plugin not available; repository Playwright", browserErrors, browserConsole, failedRequests, failedResponses, expectedFailures: [...expectedFailures.entries()], unexpectedConsoleErrors };
  await writeFile(join(evidenceDir, "result.json"), JSON.stringify(result, null, 2)); console.log(JSON.stringify({ status: result.status, evidenceDir, steps, modelCalls, providerCalls: providerCalls.length, browserErrors }));
} catch (error) {
  await writeFile(join(evidenceDir, "failure.txt"), String(error.stack || error));
  if (page) { await page.screenshot({ animations: "disabled", path: join(evidenceDir, "failure.png") }).catch(() => {}); await writeFile(join(evidenceDir, "failure-page.txt"), `${page.url()}\n${await page.locator("body").innerText().catch(() => "")}`); }
  await writeFile(join(evidenceDir, "browser-diagnostics.json"), JSON.stringify({ browserErrors, browserConsole, failedRequests, failedResponses, expectedFailures: [...expectedFailures.entries()] }, null, 2)); throw error;
} finally {
  const cleanup = await Promise.allSettled([browser?.close(), ...children.slice().reverse().map(child => stop(child))]);
  for (const server of [mcp, model]) if (server) { server.closeAllConnections(); await new Promise(resolveClose => server.close(resolveClose)); }
  await writeFile(join(evidenceDir, "cleanup.json"), JSON.stringify({ attempts: cleanup.map(result => ({ status: result.status, reason: result.status === "rejected" ? String(result.reason) : null })), backendPort, frontendPort }, null, 2));
  await writeFile(join(evidenceDir, "services.log"), logs.join(""));
  if (cleanup.some(result => result.status === "rejected")) throw new Error("Test service cleanup failed; see evidence");
}
async function freePort() { const server = createTcpServer(); await new Promise(resolveListen => server.listen(0, "127.0.0.1", resolveListen)); const port = server.address().port; await new Promise(resolveClose => server.close(resolveClose)); return port; }
async function listen(server) { await new Promise(resolveListen => server.listen(0, "127.0.0.1", resolveListen)); }
async function body(request) { let content = ""; for await (const chunk of request) content += chunk; return content; }
async function stop(child) { if (!child || child.exitCode !== null || child.signalCode !== null) return; child.kill(); await Promise.race([new Promise(resolveExit => child.once("exit", resolveExit)), new Promise(resolveWait => setTimeout(resolveWait, 3000))]); if (child.exitCode === null && child.signalCode === null) { child.kill("SIGKILL"); await Promise.race([new Promise(resolveExit => child.once("exit", resolveExit)), new Promise((_, reject) => setTimeout(() => reject(new Error(`Own child ${child.pid} did not exit`)), 3000))]); } }
