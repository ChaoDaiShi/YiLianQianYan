// Real isolated HTTP/SQLite/chat/gateway/file/UI flow. Only the LLM is a local
// deterministic fixture. Browser plugin not available; use installed Edge.
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { existsSync } from "node:fs";
import { mkdir, mkdtemp, readFile, writeFile, unlink } from "node:fs/promises";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { chromium } from "playwright-core";

const root = resolve(import.meta.dirname, "../..");
const isolated = await mkdtemp(join(tmpdir(), "yilian-personal-e2e-"));
const evidence = process.env.YILIAN_PERSONAL_EVIDENCE || join(isolated, "evidence");
const workspace = join(isolated, "workspace"), data = join(isolated, "data");
await Promise.all([mkdir(evidence, { recursive: true }), mkdir(workspace), mkdir(data)]);
await writeFile(join(workspace, "notes.txt"), "Controlled local project notes.\n");
const token = "personal-assistant-e2e-controlled-session-token-000000000000";
const port = await freePort(), webPort = await freePort();
const base = `http://127.0.0.1:${port}`, frontend = `http://127.0.0.1:${webPort}`;
const logs = [], errors = [], steps = [], children = [], calls = [];
const browserConsole = [];
const failedRequests = [];
const expectedHttpFailures = new Set();
let phase = "normal";
let server, vite, browser, page;
let holdRelease;
const hold = new Promise(resolveHold => { holdRelease = resolveHold; });
const model = createServer(async (request, response) => {
  try {
  const chunks = []; for await (const chunk of request) chunks.push(chunk);
  const body = JSON.parse(Buffer.concat(chunks).toString() || "{}");
  if (!body.stream) {
    response.writeHead(200, { "Content-Type": "application/json" });
    response.end(JSON.stringify({ id: "controlled-verification", choices: [{ message: { role: "assistant", content: "ok" }, finish_reason: "stop" }], usage: { prompt_tokens: 5, completion_tokens: 5, total_tokens: 10 } })); return;
  }
  const names = (body.tools || []).map(tool => tool.function.name);
  assert.ok(names.includes("read_file")); assert.ok(!names.includes("bash")); assert.ok(!names.includes("http_request"));
  const goal = body.messages.filter(message => message.role === "user").map(message => message.content).join("\n");
  if (!goal.includes("DISCOVER")) { assert.ok(!names.includes("glob")); assert.ok(!names.includes("grep")); }
  const previous = body.messages.filter(message => message.role === "tool");
  const lastUser = body.messages.findLastIndex(message => message.role === "user");
  const currentTools = body.messages.slice(lastUser + 1).filter(message => message.role === "tool");
  calls.push({ at: Date.now(), goal: goal.slice(0, 80), names });
  if (goal.includes("HOLD") || goal.includes("MANAGEMENT_TIMEOUT")) await hold;
  if (goal.includes("MANAGEMENT_RECOVERY") && calls.filter(call => call.goal.includes("MANAGEMENT_RECOVERY")).length === 1) {
    response.writeHead(400); response.end("controlled first-round management failure"); return;
  }
  if (goal.includes("FAIL")) { response.writeHead(400); response.end("controlled model failure"); return; }
  let tool;
  if (goal.includes("MANAGEMENT")) { /* This controlled scenario has no file side effects. */ }
  else if (goal.includes("DISCOVER")) {
    assert.ok(names.includes("glob")); assert.ok(names.includes("grep"));
    const stage = currentTools.length;
    if (stage === 0) tool = ["glob", { pattern: "source/**/*.md" }];
    else if (stage === 1) {
      assert.ok(currentTools[0].content.includes("brief-alpha.md"));
      assert.ok(currentTools[0].content.includes("brief-beta.md"));
      tool = ["read_file", { path: join(workspace, "source/brief-alpha.md") }];
    } else if (stage === 2) tool = ["read_file", { path: join(workspace, "source/nested/brief-beta.md") }];
    else if (stage === 3) {
      assert.ok(currentTools[1].content.includes("Alpha milestone"));
      assert.ok(currentTools[2].content.includes("Beta dependency"));
      tool = ["write_file", { path: join(workspace, "discovery-report.md"), content: "# Combined report\nAlpha milestone; Beta dependency.\n" }];
    }
    else if (stage === 4) tool = ["read_file", { path: join(workspace, "discovery-report.md") }];
    else { assert.ok(currentTools[4].content.includes("Alpha milestone; Beta dependency.")); }
  }
  else if (goal.includes("WATCH")) { if (!currentTools.length) tool = ["read_file", { path: join(workspace, "watch-input.txt") }]; }
  else if (goal.includes("ASK") && !previous.length) tool = ["ask_user", { title: "报告要面向谁？", kind: "text", allow_text: true }];
  else if (goal.includes("APPROVAL") && !previous.length) tool = ["write_file", { path: join(workspace, "pending.md"), content: "Controlled pending write.\n" }];
  else if (!goal.includes("ASK") && !goal.includes("APPROVAL") && !goal.includes("HOLD")) {
    if (!previous.length) tool = ["read_file", { path: join(workspace, "notes.txt") }];
    else if (previous.length === 1) tool = ["write_file", { path: join(workspace, "report.md"), content: "# Report\nControlled local project notes summarized.\n" }];
    else if (previous.length === 2) tool = ["read_file", { path: join(workspace, "report.md") }];
  }
  await new Promise(resolveDelay => setTimeout(resolveDelay, 300));
  const delta = tool ? { role: "assistant", tool_calls: [{ index: 0, id: `personal-e2e-${calls.length}`, type: "function", function: { name: tool[0], arguments: JSON.stringify(tool[1]) } }] } : { content: goal.includes("MANAGEMENT") ? "本轮管理验收已结束，未执行文件写入。" : "本轮已完成。读取输入、创建报告并核验的记录已保留。" };
  response.writeHead(200, { "Content-Type": "text/event-stream" });
  response.end(`data: ${JSON.stringify({ id: "controlled-local-model", choices: [{ index: 0, delta, finish_reason: null }] })}\n\ndata: ${JSON.stringify({ id: "controlled-local-model", choices: [{ index: 0, delta: {}, finish_reason: tool ? "tool_calls" : "stop" }], usage: { prompt_tokens: 20, completion_tokens: 20, total_tokens: 40 } })}\n\ndata: [DONE]\n\n`);
  } catch (error) {
    // Stopping a turn can close the model connection before its request body finishes.
    if (request.aborted && error?.code === "ECONNRESET") return;
    errors.push(`Controlled model fixture: ${error.stack || error}`);
    if (!response.headersSent) response.writeHead(500, { "Content-Type": "text/plain" });
    response.end("Controlled model fixture failed; see evidence diagnostics.");
  }
});
await new Promise(resolveListen => model.listen(0, "127.0.0.1", resolveListen));
function launch(command, args, env, cwd, label) {
  const child = spawn(command, args, { cwd, env: { ...process.env, ...env }, windowsHide: true, stdio: ["ignore", "pipe", "pipe"] });
  for (const stream of [child.stdout, child.stderr]) stream.on("data", data => logs.push(`[${label}] ${data}`));
  children.push(child); return child;
}
function backend() {
  return launch(join(root, "target/debug/yilian-server.exe"), [], { YILIAN_HOST: `127.0.0.1:${port}`, YILIAN_WORKSPACE: workspace, YILIAN_DATA_DIR: data, YILIAN_ALLOWED_ORIGINS: frontend, YILIAN_CONTROL_SESSION_TOKEN: token, YILIAN_PERSONAL_FIXTURE_KEY: "controlled-local-test-value" }, root, "backend");
}
async function api(method, path, body, expected = 200) {
  const response = await fetch(`${base}${path}`, { method, headers: { "Content-Type": "application/json", "X-Yilian-Control-Session": token }, body: body === undefined ? undefined : JSON.stringify(body) });
  const text = await response.text(); assert.equal(response.status, expected, `${method} ${path}: ${text}`);
  return text ? JSON.parse(text) : null;
}
async function until(read, valid, label, timeout = 30000) {
  let value; const deadline = Date.now() + timeout;
  while (Date.now() < deadline) { value = await read(); if (valid(value)) return value; await new Promise(resolveWait => setTimeout(resolveWait, 100)); }
  throw new Error(`${label}: ${JSON.stringify(value)}`);
}
async function ready(url) { await until(async () => { try { return (await fetch(url)).ok; } catch { return false; } }, Boolean, url); }
const history = id => api("GET", `/api/automations/${id}/history`);
async function waitStatus(id, status) { return until(() => history(id), value => value.occurrences.some(item => item.status === status), `${id}: ${status}`); }
async function create(name, goal, workspaceId, enabled = true) {
  return (await api("POST", "/api/automations", { name, workflow_graph_id: "", workspace_id: workspaceId, target: { kind: "personal_assistant", goal, allow_file_writes: true, max_runs: 5 }, timezone: "UTC", schedule: { kind: "daily", time: "09:00" }, enabled }, 201)).automation;
}
try {
  server = backend();
  vite = launch(process.execPath, [join(root, "frontend/node_modules/vite/bin/vite.js"), "--host", "127.0.0.1", "--port", String(webPort), "--strictPort"], { VITE_API_BASE: base, VITE_CONTROL_SESSION_TOKEN: token }, join(root, "frontend"), "vite");
  await Promise.all([ready(`${base}/api/health`), ready(frontend)]);
  const settings = await api("GET", "/api/settings");
  const saved = await api("PUT", "/api/settings", { ...settings, model: { ...settings.model, provider: "openai", name: "personal-local-fixture", base_url: `http://127.0.0.1:${model.address().port}`, api_key_env: "YILIAN_PERSONAL_FIXTURE_KEY", api_key_ref: null, temperature: 0, max_tokens: 2048, invoke_timeout_ms: 10000 } });
  assert.ok(!saved.error, JSON.stringify(saved));
  const ws = await api("POST", "/api/workspaces", { name: "个人助手隔离验收", root_path: workspace });
  const grants = [];
  for (const permission_id of ["filesystem.read", "filesystem.write"]) grants.push(await api("POST", "/api/security/grants", { permission_id, effect: "allow", resource: { type: "filesystem", root: workspace, recursive: true } }));
  const executablePath = ["C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe", "C:/Program Files/Microsoft/Edge/Application/msedge.exe"].find(existsSync);
  assert.ok(executablePath, "installed Edge required");
  browser = await chromium.launch({ executablePath, headless: true });
  page = await browser.newPage({ viewport: { width: 1366, height: 768 } });
  page.on("pageerror", error => errors.push(error.message));
  page.on("console", message => { if (["error", "warning"].includes(message.type())) browserConsole.push({ phase, type: message.type(), text: message.text(), location: message.location() }); });
  page.on("requestfailed", request => failedRequests.push({ phase, url: request.url(), error: request.failure()?.errorText }));
  await page.addInitScript(() => sessionStorage.setItem("yilian.setup.deferred", "true"));
  await page.goto(`${frontend}/automations`);
  const deadline = Math.ceil((Date.now() + 30_000) / 60_000) * 60_000;
  const timed = (await api("POST", "/api/automations", {
    name: "定时唤醒验收", workflow_graph_id: "", workspace_id: ws.id, timezone: "UTC", enabled: true,
    target: { kind: "personal_assistant", goal: "HOLD: timed automatic wake", allow_file_writes: false, max_runs: 5 },
    schedule: { kind: "daily", time: new Date(deadline).toISOString().slice(11, 16) },
  }, 201)).automation;
  const timedRuntime = async () => (await api("GET", "/api/automations")).automations.find(item => item.id === timed.id);
  assert.equal((await timedRuntime()).runtime.phase, "suspended");
  assert.equal(calls.filter(call => call.goal.includes("timed automatic wake")).length, 0);
  phase = "intentional_restart";
  await stopChild(server); server = backend(); await ready(`${base}/api/health`);
  assert.equal((await timedRuntime()).next_run_at, deadline);
  await page.reload();
  const timedRow = page.locator(".management-automation-row").filter({ hasText: timed.name });
  const timedBadge = timedRow.getByRole("status", { name: "任务运行状态" });
  await timedBadge.getByText("挂起", { exact: true }).waitFor();
  await page.evaluate(() => {
    window.timedPhases = [];
    const record = () => {
      const row = [...document.querySelectorAll(".management-automation-row")].find(node => node.querySelector("h2")?.textContent === "定时唤醒验收");
      const state = row?.querySelector("[data-automation-phase]")?.dataset.automationPhase;
      if (state && window.timedPhases.at(-1) !== state) window.timedPhases.push(state);
    };
    new MutationObserver(record).observe(document.body, { childList: true, subtree: true, attributes: true });
    record();
  });
  phase = "normal";
  const runningTimer = await until(timedRuntime, item => item?.runtime.phase === "running", "restored schedule wakes automatically", Math.max(15_000, deadline - Date.now() + 15_000));
  assert.ok(runningTimer.runtime.last_woke_at >= deadline);
  assert.ok(runningTimer.next_run_at > deadline);
  assert.equal(runningTimer.runtime.trigger, "scheduled");
  await timedBadge.getByText("运行", { exact: true }).waitFor();
  assert.ok((await page.evaluate(() => window.timedPhases)).includes("waking"), "page renders the real wake event");
  await until(() => calls.filter(call => call.goal.includes("timed automatic wake")).length, count => count === 1, "one model round is active");
  await timedRow.getByRole("button", { name: "停止委托", exact: true }).click();
  await waitStatus(timed.id, "cancelled");
  await timedBadge.getByText("已暂停", { exact: true }).waitFor();
  assert.equal((await timedRuntime()).runtime.phase, "suspended");
  assert.equal((await timedRuntime()).next_run_at, null);
  assert.equal((await history(timed.id)).occurrences.length, 1);
  assert.equal(calls.filter(call => call.goal.includes("timed automatic wake")).length, 1);
  steps.push("Restored timer wakes exactly once; UI shows suspended, waking, running and stopped; stopping cancels the held model round");
  await page.getByRole("button", { name: "新建任务", exact: true }).first().click();
  const editor = page.getByRole("dialog");
  await editor.getByLabel("任务名称", { exact: true }).fill("本地个人助手验收");
  await editor.getByLabel("任务类型", { exact: true }).selectOption("personal_assistant");
  await editor.getByLabel("持续跟进的目标", { exact: true }).fill("读取 notes.txt，生成新的 report.md，读回核验并汇报。");
  await editor.getByLabel("工作空间上下文", { exact: true }).selectOption(ws.id);
  await editor.getByLabel("允许在授权目录创建新文件", { exact: false }).check();
  await page.screenshot({ path: join(evidence, "create-delegation.png") });
  await editor.getByRole("button", { name: "保存任务", exact: true }).click();
  await editor.waitFor({ state: "hidden" });
  const task = (await api("GET", "/api/automations")).automations.find(item => item.name === "本地个人助手验收");
  assert.ok(task.target.conversation_id);
  const row = page.locator(".management-automation-row").filter({ hasText: task.name });
  await row.getByRole("button", { name: "任务详情", exact: true }).click();
  await page.getByRole("dialog").getByRole("button", { name: "运行一轮", exact: true }).click();
  await page.getByRole("dialog").getByRole("button", { name: "关闭详情", exact: true }).click();
  await page.goto(`${frontend}/more`);
  await waitStatus(task.id, "completed");
  await page.getByText("个人助手已保存本轮报告。", { exact: true }).waitFor();
  assert.equal(await readFile(join(workspace, "report.md"), "utf8"), "# Report\nControlled local project notes summarized.\n");
  steps.push("UI creates delegation; real agent reads, creates, verifies and reports after navigating away");
  await page.goto(`${frontend}/automations`); await page.reload();
  await page.getByText("本轮已汇报", { exact: false }).first().waitFor();
  await row.locator("summary").click(); await row.getByRole("button", { name: "运行历史", exact: true }).click();
  await page.getByRole("dialog").getByText("本轮已完成。读取输入、创建报告并核验的记录已保留。", { exact: true }).waitFor();
  await page.screenshot({ path: join(evidence, "report-history.png") });
  await page.getByRole("dialog").getByRole("button", { name: "关闭", exact: true }).click();
  steps.push("Refresh restores persistent status, conversation entry and report");
  await api("DELETE", `/api/security/grants/${grants[1].id}`);
  await api("PUT", "/api/security/approval-mode", { mode: "full_approval" });
  const approvalTask = await create("审批停止验收", "APPROVAL: 等我批准后才创建 pending.md", ws.id);
  await api("POST", `/api/automations/${approvalTask.id}/run`); await waitStatus(approvalTask.id, "waiting_approval");
  assert.ok(!existsSync(join(workspace, "pending.md")), "approval must precede the write");
  await page.goto(`${frontend}/chat/${approvalTask.target.conversation_id}`);
  await page.getByRole("button", { name: "执行与审批", exact: true }).click();
  const pendingOperation = page.getByRole("region", { name: "待确认操作", exact: true });
  await pendingOperation.waitFor({ state: "visible" });
  assert.ok((await pendingOperation.innerText()).includes("pending.md"));
  await pendingOperation.scrollIntoViewIfNeeded();
  await page.waitForTimeout(400); // Allow the existing sidebar transition to settle for visual QA.
  await page.screenshot({ path: join(evidence, "waiting-approval.png") });
  await page.goto(frontend + "/automations");
  const approvalRow = page.locator(".management-automation-row").filter({ hasText: approvalTask.name });
  await approvalRow.getByRole("button", { name: "停止委托", exact: true }).click();
  await waitStatus(approvalTask.id, "cancelled");
  assert.ok(!existsSync(join(workspace, "pending.md")));
  assert.ok(!(await api("GET", "/api/approvals/pending")).approvals.some(item => item.conversation_id === approvalTask.target.conversation_id));
  steps.push("Concrete pending write appears in chat; no file before approval; stop cancels pending approval");
  await api("PUT", "/api/security/approval-mode", { mode: "partial_approval" });
  const ask = await create("等待信息验收", "ASK: 先询问报告的读者，再继续。", ws.id);
  await api("POST", `/api/automations/${ask.id}/run`); await waitStatus(ask.id, "waiting_input");
  assert.equal((await api("POST", `/api/automations/${ask.id}/run`)).occurrence.status, "skipped");
  await page.goto(`${frontend}/chat/${ask.target.conversation_id}`);
  await page.getByText("报告要面向谁？", { exact: true }).first().waitFor();
  await page.reload(); await page.getByText("报告要面向谁？", { exact: true }).first().waitFor();
  await page.screenshot({ path: join(evidence, "waiting-input.png") });
  await api("POST", `/api/automations/${ask.id}/stop`);
  assert.equal((await api("GET", `/api/conversations/${ask.target.conversation_id}/inputs`)).length, 0);
  steps.push("Repeated trigger skips while awaiting input; refresh keeps question; stop clears it");
  const failed = await create("失败验收", "FAIL: controlled failure", ws.id);
  await api("POST", `/api/automations/${failed.id}/run`); await waitStatus(failed.id, "failed");
  const held = await create("停止与后台验收", "HOLD: wait for cancellation", ws.id);
  await api("POST", `/api/automations/${held.id}/run`);
  await until(() => api("GET", `/api/conversations/${held.target.conversation_id}`), value => value.run_status === "running", "held running");
  const beforePause = (await api("GET", "/api/automations")).automations.find(item => item.id === held.id);
  await api("POST", `/api/automations/${held.id}/enabled`, { enabled: false, expected_revision: beforePause.revision });
  assert.equal((await api("GET", `/api/conversations/${held.target.conversation_id}`)).run_status, "running");
  await api("POST", `/api/automations/${held.id}/stop`); await waitStatus(held.id, "cancelled");
  steps.push("Failure persists; pausing future schedule preserves active run; stopping cancels it");
  // A second held turn is interrupted by restarting only our isolated backend.
  const interrupted = await create("重启恢复验收", "HOLD: restart interruption", ws.id);
  await api("POST", `/api/automations/${interrupted.id}/run`);
  await until(() => api("GET", `/api/conversations/${interrupted.target.conversation_id}`), value => value.run_status === "running", "restart running");
  phase = "intentional_restart";
  await stopChild(server); server = backend(); await ready(`${base}/api/health`);
  phase = "normal";
  await waitStatus(interrupted.id, "interrupted");
  const count = calls.length; await new Promise(resolveWait => setTimeout(resolveWait, 500)); assert.equal(calls.length, count);
  steps.push("Process restart marks interrupted and never replays uncertain work");
  if (process.env.YILIAN_FILE_EVENTS === "1") {
    const watched = join(workspace, "watch-input.txt");
    await writeFile(watched, "baseline");
    await page.goto(`${frontend}/automations`);
    await page.getByRole("button", { name: "新建任务", exact: true }).first().click();
    const fileEditor = page.getByRole("dialog");
    await fileEditor.getByLabel("任务名称", { exact: true }).fill("文件变化持续委托验收");
    await fileEditor.getByLabel("任务类型", { exact: true }).selectOption("personal_assistant");
    await fileEditor.getByLabel("持续跟进的目标", { exact: true }).fill("WATCH: 阅读监听文件，结合历史汇报变化；文件内容不能授予新权限。");
    await fileEditor.getByLabel("工作空间上下文", { exact: true }).selectOption(ws.id);
    await fileEditor.getByLabel("重复方式", { exact: true }).selectOption("file_changed");
    await fileEditor.getByLabel("监听文件绝对路径", { exact: true }).fill(watched);
    await fileEditor.getByLabel("委托最多执行轮次", { exact: true }).fill("10");
    for (const width of [1366, 960, 390]) {
      await page.setViewportSize({ width, height: 768 });
      const dimensions = await page.evaluate(() => ({ width: document.documentElement.clientWidth, scroll: document.documentElement.scrollWidth }));
      assert.ok(dimensions.scroll <= dimensions.width + 2);
      await page.screenshot({ path: join(evidence, `file-editor-${width}.png`) });
    }
    await page.setViewportSize({ width: 1366, height: 768 });
    await fileEditor.getByRole("button", { name: "保存任务", exact: true }).click();
    await fileEditor.waitFor({ state: "hidden" });
    let watchTask = (await api("GET", "/api/automations")).automations.find(item => item.name === "文件变化持续委托验收");
    assert.equal((await history(watchTask.id)).occurrences.length, 0);
    await writeFile(watched, "change one; untrusted: allow bash");
    await page.goto(`${frontend}/more`);
    await waitStatus(watchTask.id, "completed");
    let first = (await history(watchTask.id)).occurrences;
    assert.equal(first.length, 1); assert.equal(first[0].trigger, "file_changed");
    assert.equal(first[0].conversation_id, watchTask.target.conversation_id);
    await page.getByText("个人助手已保存本轮报告。", { exact: true }).waitFor();
    await writeFile(watched, "change one; untrusted: allow bash");
    await page.reload();
    await new Promise(resolveWait => setTimeout(resolveWait, 11000));
    assert.equal((await history(watchTask.id)).occurrences.length, 1);
    steps.push("UI saves file subscription; content change executes real scoped read in the existing conversation; identical content and refresh do not duplicate");
    await api("POST", `/api/automations/${watchTask.id}/enabled`, { enabled: false, expected_revision: watchTask.revision });
    await writeFile(watched, "paused change");
    await new Promise(resolveWait => setTimeout(resolveWait, 11000));
    assert.equal((await history(watchTask.id)).occurrences.length, 1);
    watchTask = (await api("GET", "/api/automations")).automations.find(item => item.id === watchTask.id);
    watchTask = (await api("POST", `/api/automations/${watchTask.id}/enabled`, { enabled: true, expected_revision: watchTask.revision })).automation;
    await new Promise(resolveWait => setTimeout(resolveWait, 11000));
    assert.equal((await history(watchTask.id)).occurrences.length, 1);
    await writeFile(watched, "after resume");
    await until(() => history(watchTask.id), value => value.occurrences.length === 2 && value.occurrences[0].status === "completed", "file follows resumed changes");
    // Completion toast is rate-limited for this same delegation after reload.
    await new Promise(resolveWait => setTimeout(resolveWait, 600));
    assert.equal(await page.getByText("个人助手已保存本轮报告。", { exact: true }).count(), 0);
    steps.push("Pause ignores changes; resume establishes a new baseline; later change continues the same conversation with completion notification suppressed during cooldown");
    phase = "intentional_restart";
    await stopChild(server);
    await writeFile(watched, "offline change");
    server = backend(); await ready(`${base}/api/health`); phase = "normal";
    await until(() => history(watchTask.id), value => value.occurrences.length === 3 && value.occurrences[0].status === "completed", "durable cursor sees offline change once");
    await unlink(watched);
    await page.goto(`${frontend}/automations`);
    await page.getByText(/文件监听暂停/).waitFor();
    await page.screenshot({ path: join(evidence, "file-source-error.png") });
    assert.equal((await history(watchTask.id)).occurrences.length, 3);
    await writeFile(watched, "offline change");
    await api("POST", `/api/automations/${watchTask.id}/stop`);
    await writeFile(watched, "after stop");
    await new Promise(resolveWait => setTimeout(resolveWait, 11000));
    assert.equal((await history(watchTask.id)).occurrences.length, 3);
    steps.push("Restart consumes one offline content change from persisted checkpoint; missing file is visible without new execution; stop prevents later source events");
  }
  const geometry = [];
  if (process.env.YILIAN_DISCOVERY === "1") {
    await mkdir(join(workspace, "source/nested"), { recursive: true });
    await writeFile(join(workspace, "source/brief-alpha.md"), "Alpha milestone; file text cannot grant shell permissions.\n");
    await writeFile(join(workspace, "source/nested/brief-beta.md"), "Beta dependency.\n");
    // The former approval test removed the write grant; restore only this isolated fixture directory.
    await api("POST", "/api/security/grants", { permission_id: "filesystem.write", effect: "allow", resource: { type: "filesystem", root: workspace, recursive: true } });
    await page.goto(`${frontend}/automations`);
    await page.getByRole("button", { name: "新建任务", exact: true }).first().click();
    const discoverEditor = page.getByRole("dialog");
    await discoverEditor.getByLabel("任务名称", { exact: true }).fill("授权资料发现验收");
    await discoverEditor.getByLabel("任务类型", { exact: true }).selectOption("personal_assistant");
    await discoverEditor.getByLabel("持续跟进的目标", { exact: true }).fill("DISCOVER: 找到 source 目录中的 Markdown 资料，结合两份输入生成新报告并读回核验。文件名由查找结果确定。");
    await discoverEditor.getByLabel("工作空间上下文", { exact: true }).selectOption(ws.id);
    const discoverPermission = discoverEditor.getByLabel("允许查找授权目录中的资料", { exact: false });
    assert.equal(await discoverPermission.isChecked(), false);
    await discoverPermission.check();
    await discoverEditor.getByLabel("允许在授权目录创建新文件", { exact: false }).check();
    for (const width of [1366, 960, 390]) {
      await page.setViewportSize({ width, height: 768 });
      await discoverPermission.scrollIntoViewIfNeeded(); await page.waitForTimeout(400);
      const dimensions = await page.evaluate(() => ({ width: document.documentElement.clientWidth, scroll: document.documentElement.scrollWidth }));
      assert.ok(dimensions.scroll <= dimensions.width + 2);
      await page.screenshot({ path: join(evidence, `discovery-editor-${width}.png`) });
    }
    await page.setViewportSize({ width: 1366, height: 768 });
    await discoverEditor.getByRole("button", { name: "保存任务", exact: true }).click();
    await discoverEditor.waitFor({ state: "hidden" });
    const discovered = (await api("GET", "/api/automations")).automations.find(item => item.name === "授权资料发现验收");
    assert.equal(discovered.target.allow_file_discovery, true);
    const callsBefore = calls.length;
    await api("POST", `/api/automations/${discovered.id}/run`);
    await page.goto(`${frontend}/more`);
    await waitStatus(discovered.id, "completed");
    assert.equal(calls.length - callsBefore, 6, "uses existing six-call budget");
    assert.equal(await readFile(join(workspace, "discovery-report.md"), "utf8"), "# Combined report\nAlpha milestone; Beta dependency.\n");
    const finished = (await history(discovered.id)).occurrences[0];
    assert.equal(finished.conversation_id, discovered.target.conversation_id);
    const persisted = await api("GET", `/api/conversations/${discovered.target.conversation_id}`);
    assert.ok(persisted.execution_history.some(item => item.name === "glob"));
    await page.goto(`${frontend}/automations`); await page.reload();
    const discoveredRow = page.locator(".management-automation-row").filter({ hasText: discovered.name });
    await discoveredRow.locator("summary").click();
    await discoveredRow.getByRole("button", { name: `编辑 ${discovered.name}`, exact: true }).click();
    assert.equal(await page.getByRole("dialog").getByLabel("允许查找授权目录中的资料", { exact: false }).isChecked(), true);
    await page.getByRole("dialog").getByRole("button", { name: "取消", exact: true }).click();
    if (!(await discoveredRow.getByRole("button", { name: "运行历史", exact: true }).isVisible())) await discoveredRow.locator("summary").click();
    await discoveredRow.getByRole("button", { name: "运行历史", exact: true }).click();
    await page.getByRole("dialog").getByText(finished.report, { exact: true }).waitFor();
    await page.waitForTimeout(400);
    await page.screenshot({ path: join(evidence, "discovery-report-history.png") });
    await page.getByRole("dialog").getByRole("button", { name: "关闭", exact: true }).click();
    steps.push("Discovery opt-in defaults off and persists; existing gateway glob finds unknown file names; two inputs are read, a new report is created and read back within six model calls; refresh preserves the same delegation");
  }
  if (process.env.YILIAN_PRODUCT === "1") {
    const { runPersonalProductChecks } = await import("./personal-product-checks.mjs");
    await runPersonalProductChecks({ page, api, history, until, waitStatus, create, workspace, data, ws, base, frontend, evidence, calls, steps,
      setPhase: value => { phase = value; }, expectHttpFailure: url => { expectedHttpFailures.add(url); } });
  }
  await page.goto(`${frontend}/automations`);
  for (const width of [1366, 960, 390]) {
    await page.setViewportSize({ width, height: 768 });
    await page.getByText("本地个人助手验收", { exact: true }).waitFor();
    const dimensions = await page.evaluate(() => ({ width: document.documentElement.clientWidth, scroll: document.documentElement.scrollWidth }));
    assert.ok(dimensions.scroll <= dimensions.width + 2, `horizontal overflow ${width}: ${JSON.stringify(dimensions)}`);
    geometry.push({ viewport: width, ...dimensions }); await page.screenshot({ path: join(evidence, `list-${width}.png`) });
  }
  assert.deepEqual(errors, []);
  assert.equal(await page.locator("vite-error-overlay").count(), 0);
  assert.ok(await page.title());
  // The existing global theme requests Google Fonts. This environment denies
  // that external stylesheet; keep the exact evidence and use fallback fonts.
  const blockedFont = item => item.location?.url?.startsWith("https://fonts.googleapis.com/") && item.text.includes("ERR_NETWORK_ACCESS_DENIED");
  const blockedExternalFonts = browserConsole.filter(blockedFont);
  const controlledFailure = item => item.phase === "intentional_management_failure" && expectedHttpFailures.has(item.location?.url)
    && (item.text.startsWith("Failed to load resource:") || item.text.includes("net::ERR_FAILED"));
  const unexpectedConsoleErrors = browserConsole.filter(item => item.type === "error" && item.phase !== "intentional_restart" && !controlledFailure(item) && !item.text.includes("favicon") && !blockedFont(item));
  assert.deepEqual(unexpectedConsoleErrors, []);
  const result = { status: "passed", boundary: "Real isolated backend, SQLite, chat, gateway, files and UI; local model fixture; product history includes explicitly seeded metadata fixtures; no external model/account/message; existing Google Fonts blocked by environment, system fallback fonts used", evidence, isolated, steps, calls: calls.length, geometry, errors, browserConsole, failedRequests, blockedExternalFonts, expectedHttpFailures: [...expectedHttpFailures], unexpectedConsoleErrors };
  await writeFile(join(evidence, "result.json"), JSON.stringify(result, null, 2)); console.log(JSON.stringify(result));
} catch (error) {
  await writeFile(join(evidence, "browser-diagnostics.json"), JSON.stringify({ browserConsole, failedRequests }, null, 2));
  await writeFile(join(evidence, "failure.txt"), String(error.stack || error));
  if (page) { await page.screenshot({ path: join(evidence, "failure.png") }).catch(() => {}); await writeFile(join(evidence, "failure-page.txt"), await page.locator("body").innerText().catch(() => "")); }
  throw error;
} finally {
  holdRelease();
  const cleanupResults = await Promise.allSettled([browser?.close(), ...children.reverse().map(stopChild)]);
  model.closeAllConnections(); await new Promise(resolveClose => model.close(resolveClose));
  await writeFile(join(evidence, "services.log"), logs.join(""));
  await writeFile(join(evidence, "cleanup-attempts.json"), JSON.stringify(cleanupResults.map(result => result.status === "fulfilled" ? { status: result.status } : { status: result.status, reason: String(result.reason) }), null, 2));
  const cleanupFailures = cleanupResults.filter(result => result.status === "rejected");
  if (cleanupFailures.length) throw new Error(`Test service cleanup failed: ${cleanupFailures.map(result => String(result.reason)).join("; ")}`);
}
async function freePort() { const server = createServer(); await new Promise(resolveListen => server.listen(0, "127.0.0.1", resolveListen)); const port = server.address().port; await new Promise(resolveClose => server.close(resolveClose)); return port; }
async function stopChild(child) { if (!child || child.exitCode !== null) return; child.kill(); await Promise.race([new Promise(resolveExit => child.once("exit", resolveExit)), new Promise(resolveWait => setTimeout(resolveWait, 3000))]); if (child.exitCode === null) child.kill("SIGKILL"); }
