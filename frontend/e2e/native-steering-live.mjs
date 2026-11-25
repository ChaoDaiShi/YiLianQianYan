import assert from "node:assert/strict";
import { randomUUID } from "node:crypto";
import { existsSync } from "node:fs";
import { mkdir, readFile, stat, writeFile } from "node:fs/promises";
import { isAbsolute, join, resolve } from "node:path";
import { chromium } from "playwright-core";

const cdpi = loopback(process.env.YILIAN_LIVE_CDP_URL);
const app = loopback(process.env.YILIAN_LIVE_APP_URL);
const backend = loopback(process.env.YILIAN_LIVE_API_URL);
const workspace = process.env.YILIAN_LIVE_WORKSPACE;
assert.ok(workspace && isAbsolute(workspace), "set YILIAN_LIVE_WORKSPACE to the isolated writable workspace");
assert.ok((await stat(workspace)).isDirectory(), "isolated workspace is not a directory");
const stamp = randomUUID().slice(0, 8);
const evidence = resolve(process.env.YILIAN_LIVE_STEERING_EVIDENCE || join(import.meta.dirname, "../../target/native-steering-live"));
await mkdir(evidence, { recursive: true });

function loopback(raw) {
  const value = new URL(raw);
  assert.ok(value.protocol === "http:" && ["127.0.0.1", "localhost", "tauri.localhost"].includes(value.hostname) && !value.username && !value.password, "expected a loopback HTTP origin");
  return value.origin;
}

const report = {
  status: "running",
  boundary: "Native Tauri WebView2 via loopback CDP; configured real model; real local API, SQLite, approval and scratch file; test authored prompts only.",
  app,
  backend,
  scenarios: [],
};
let page;
let phase = "setup";
let conversation;
const faults = [];
const timeout = Number(process.env.YILIAN_LIVE_STEERING_TIMEOUT_MS || 90000);

async function api(method, path, body) {
  return page.evaluate(async ({ backend, method, path, body }) => {
    const invoke = window.__TAURI_INTERNALS__?.invoke;
    if (typeof invoke !== "function") throw new Error("native Tauri IPC unavailable");
    const token = await invoke("get_control_session_token");
    const response = await fetch(backend + path, {
      method,
      headers: { "Content-Type": "application/json", "X-Yilian-Control-Session": token },
      body: body === undefined ? undefined : JSON.stringify(body),
      signal: AbortSignal.timeout(12000),
    });
    if (!response.ok) throw new Error(`${method} ${path}: HTTP ${response.status}`);
    return response.json();
  }, { backend, method, path, body });
}

async function until(action, label, duration = timeout) {
  const stop = Date.now() + duration;
  while (Date.now() < stop) {
    const result = await action();
    if (result) return result;
    await new Promise(done => setTimeout(done, 160));
  }
  throw new Error(label);
}

async function screenshot(name) {
  const file = `${stamp}-${name}.png`;
  await page.screenshot({ path: join(evidence, file), animations: "disabled" });
  return file;
}

async function newConversation(title) {
  conversation = (await api("POST", "/api/conversations", { title })).id;
  await api("PUT", `/api/conversations/${conversation}/workspace`, { workspace_id: report.workspaceId });
  await api("PUT", `/api/conversations/${conversation}/mode`, { mode: "work" });
  await page.goto(`${app}/chat/${conversation}`);
  await page.getByRole("textbox", { name: "聊天输入" }).waitFor();
  await page.locator(".home-composer[data-workspace-mode='work']").waitFor();
  return conversation;
}

function composer() { return page.getByRole("textbox", { name: "聊天输入" }); }
function send() { return page.getByRole("button", { name: "发送消息", exact: true }).click(); }
function route(path) { return `${backend}${path}`; }
function responseFor(path) {
  return page.waitForResponse(response => response.request().method() === "POST" && response.url() === route(path), { timeout });
}

async function getTurns(id) {
  return (await api("GET", `/api/conversations/${id}/turns`)).turns;
}

function normalizedPath(path) { return path.replaceAll("\\", "/").toLowerCase(); }

async function steerAfterResume({ id, turn, correction, trigger, path, record }) {
  const text = `补充给当前任务：${correction}`;
  await composer().fill(text);
  const resume = responseFor(path);
  const resumedAt = performance.now();
  const canSteer = page.getByPlaceholder("补充给当前任务：… 或查询进度", { exact: true });
  await trigger();
  const response = await resume;
  record.resumeHttp = response.status();
  assert.ok(response.ok(), "continuation request failed");
  const availability = await Promise.race([
    canSteer.waitFor({ timeout }).then(() => "steering"),
    response.finished().then(() => "ended"),
  ]);
  assert.equal(availability, "steering", "actual model continuation completed before a steering window appeared");
  const submit = responseFor("/api/chat/steer");
  await send();
  const steeringResponse = await submit;
  record.steeringHttp = steeringResponse.status();
  assert.ok(steeringResponse.ok(), "real model finished before correction admission or steering was refused");
  const receipt = await steeringResponse.json();
  assert.equal(receipt.turn_id, turn, "correction must attach to the original turn");
  record.steeringAdmissionMs = Math.round(performance.now() - resumedAt);
  await page.getByText(correction, { exact: true }).waitFor({ timeout });
  record.appliedScreenshot = await screenshot(`${record.kind}-applied`);
  const detail = await until(async () => {
    const current = await api("GET", `/api/conversations/${id}`);
    if (["failed", "cancelled", "interrupted", "waiting_approval", "waiting_input"].includes(current.run_status)) {
      record.terminal = current.run_status;
      throw new Error(`correction settled in ${current.run_status}`);
    }
    return current.run_status === "completed" && current;
  }, "corrected turn did not complete");
  assert.equal(detail.messages.filter(message => message.id === receipt.message_id && message.role === "user" && message.content === correction).length, 1);
  const turns = await getTurns(id);
  assert.equal(turns.length, 1, "continuation created another turn");
  assert.equal(turns[0].id, turn);
  const assistant = detail.messages.filter(message => message.role === "assistant" && message.content?.trim()).at(-1);
  assert.ok(assistant, "real model produced no final assistant text");
  assert.ok(assistant.content.includes(record.expectedMarker), "real model did not follow the correction marker");
  record.applied = true;
  record.terminal = detail.run_status;
  record.replyChars = assistant.content.length;
  record.finalScreenshot = await screenshot(`${record.kind}-complete`);
  return detail;
}

async function approvalCase() {
  const id = await newConversation(`原生审批补充 ${stamp}`);
  const output = join(workspace, `approval-${stamp}.txt`);
  assert.ok(!existsSync(output));
  const expected = `native-approval-${stamp}`;
  const expectedMarker = `校正审批-${stamp}`;
  const record = { kind: "approval", conversationId: id, expectedMarker };
  report.scenarios.push(record);
  const prompt = `请实际使用 write_file，把精确内容 ${expected} 写入 ${output}。只创建这一个文件。工具确认成功后，用中文详细解释本轮步骤、范围和结果，至少写五段。`;
  await composer().fill(prompt);
  await send();
  const pending = await until(async () => {
    const detail = await api("GET", `/api/conversations/${id}`);
    if (["completed", "failed", "waiting_input"].includes(detail.run_status)) {
      record.terminal = detail.run_status;
      throw new Error("configured model did not produce the required write approval");
    }
    return detail.run_status === "waiting_approval" &&
      (await api("GET", "/api/approvals/pending")).approvals.find(value => value.conversation_id === id);
  }, "write approval not observed");
  assert.equal(pending.tool_name, "write_file");
  assert.equal(normalizedPath(pending.arguments.path), normalizedPath(output));
  assert.equal(pending.arguments.content, expected);
  assert.ok(!existsSync(output), "file existed before user approval");
  const [turn] = await getTurns(id);
  assert.equal(turn.status, "waiting_approval");
  const toggle = page.getByRole("button", { name: "执行与审批", exact: true });
  if (await toggle.getAttribute("aria-pressed") !== "true") await toggle.click();
  const approve = page.getByRole("button", { name: "允许此次工具操作", exact: true });
  await approve.waitFor();
  record.waitingScreenshot = await screenshot("approval-waiting");
  const correction = `先不要写任何其他文件，也不要重做已批准的操作。将最终回复改为一句中文，必须包含 ${expectedMarker}。`;
  const detail = await steerAfterResume({
    id, turn: turn.id, correction, trigger: () => approve.click(),
    path: `/api/approvals/${pending.approval_id}/approve`, record,
  });
  assert.equal(await readFile(output, "utf8"), expected, "approved bytes did not match");
  const recordForCall = detail.execution_history.filter(row => row.toolCallId === pending.tool_call_id);
  assert.equal(recordForCall.length, 1);
  assert.equal(recordForCall[0].executionStatus, "succeeded");
  assert.equal(recordForCall[0].verificationStatus, "passed");
  assert.equal(detail.messages.filter(message => message.role === "tool" && message.tool_call_id === pending.tool_call_id).length, 1);
  assert.equal(detail.execution_history.filter(row => row.name === "write_file").length, 1, "correction created another write attempt");
  const audit = await api("GET", `/api/security/audit?conversation_id=${id}`);
  assert.equal(audit.events.filter(event => event.event_type === "execution_started" && event.tool_call_id === pending.tool_call_id).length, 1, "approved action must execute exactly once");
  record.approvedWriteVerified = true;
}

async function questionCase() {
  const id = await newConversation(`原生补问补充 ${stamp}`);
  const output = join(workspace, `question-${stamp}.txt`);
  assert.ok(!existsSync(output));
  const expectedMarker = `校正补问-${stamp}`;
  const record = { kind: "question", conversationId: id, expectedMarker };
  report.scenarios.push(record);
  await composer().fill(`请先使用 ask_user 的 text 类型补问我希望采用什么文档格式。必须等我回答后继续处理当前任务。回答后先写五段详细方案，再考虑把方案保存为 ${output}。`);
  await send();
  const question = await until(async () => {
    const detail = await api("GET", `/api/conversations/${id}`);
    if (["completed", "failed", "waiting_approval"].includes(detail.run_status)) {
      record.terminal = detail.run_status;
      throw new Error("configured model did not produce the required ask_user question");
    }
    return (await api("GET", `/api/conversations/${id}/inputs`))[0];
  }, "ask_user question not observed");
  assert.equal(question.kind, "text");
  const card = page.getByRole("region", { name: "需要补充信息" });
  const answer = "Markdown，按五段详细说明并给出示例。";
  await card.getByRole("textbox", { name: "你的回答" }).fill(answer);
  record.waitingScreenshot = await screenshot("question-waiting");
  const correction = `先不要写任何文件。改为一句中文，必须包含 ${expectedMarker}，并保留我刚选的 Markdown 格式。`;
  const detail = await steerAfterResume({
    id, turn: question.run_id, correction,
    trigger: () => card.getByRole("button", { name: "回答并继续", exact: true }).click(),
    path: `/api/conversation-inputs/${question.question_id}/answer`, record,
  });
  const result = detail.messages.find(message => message.role === "tool" && message.tool_call_id === question.tool_call_id);
  assert.equal(JSON.parse(result.content).text, answer);
  assert.ok(!existsSync(output), "question continuation created a file after read-only correction");
  assert.ok(!detail.execution_history.some(row => row.name === "write_file"), "write attempt should not follow the read-only correction");
  const assistant = detail.messages.filter(message => message.role === "assistant" && message.content?.trim()).at(-1);
  assert.ok(/markdown/i.test(assistant.content), "configured model did not retain the answered format");
  record.questionAndAnswerVerified = true;
}

let failure = false;
try {
  const browser = await chromium.connectOverCDP(cdpi, { timeout: 30000 });
  page = browser.contexts().flatMap(context => context.pages()).find(candidate => new URL(candidate.url()).origin === app);
  assert.ok(page, "root-owned native renderer not found at expected frontend origin");
  page.on("pageerror", () => faults.push({ phase, kind: "pageerror" }));
  page.on("console", message => { if (message.type() === "error") faults.push({ phase, kind: "console_error" }); });
  assert.ok(await page.evaluate(() => typeof window.__TAURI_INTERNALS__?.invoke === "function"), "ordinary Chromium pages are not accepted");
  const version = await (await fetch(`${cdpi}/json/version`)).json();
  report.browser = version.Browser;
  report.userAgent = await page.evaluate(() => navigator.userAgent);
  await page.evaluate(() => sessionStorage.setItem("yilian.setup.deferred", "true"));
  assert.equal((await api("GET", "/api/health")).status, "healthy");
  const readiness = await api("GET", "/api/chat/readiness");
  assert.ok(readiness.available, "no available actual model in isolated profile");
  report.model = { provider: readiness.provider, model: readiness.model };
  await api("PUT", "/api/security/approval-mode", { mode: "full_approval" });
  await api("POST", "/api/security/grants", { permission_id: "filesystem.write", effect: "allow", resource: { type: "filesystem", root: workspace, recursive: true } });
  report.workspaceId = (await api("POST", "/api/workspaces", { name: `原生补充验收 ${stamp}`, root_path: workspace })).id;
  for (const [name, run] of [["approval", approvalCase], ["question", questionCase]]) {
    if (process.argv[2] && process.argv[2] !== name) continue;
    phase = name;
    try {
      await run();
    } catch (error) {
      failure = true;
      const current = report.scenarios.at(-1);
      if (current?.kind === name) {
        current.failure = String(error.message).split("\n")[0];
        current.failureScreenshot = await screenshot(`${name}-failed`).catch(() => undefined);
      }
      if (conversation) await api("POST", "/api/chat/stop", { conversation_id: conversation }).catch(() => undefined);
    }
  }
  assert.ok(report.scenarios.length, "select approval or question");
} catch (error) {
  failure = true;
  report.failure = String(error.message).split("\n")[0];
} finally {
  report.status = failure ? "failed" : "passed";
  report.pageFaults = faults;
  const resultPath = join(evidence, `${stamp}-result.json`);
  await writeFile(resultPath, JSON.stringify(report, null, 2));
  console.log(JSON.stringify({ status: report.status, resultPath, cases: report.scenarios.map(({ kind, applied, terminal, failure }) => ({ kind, applied, terminal, failure })), failure: report.failure }));
  // Closing this client connection leaves the root-owned native application running.
  process.exit(failure ? 1 : 0);
}
