import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { once } from "node:events";
import { existsSync } from "node:fs";
import { mkdir, mkdtemp, readFile, writeFile } from "node:fs/promises";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { chromium } from "playwright-core";

const root = resolve(import.meta.dirname, "../..");
const source = process.env.YILIAN_LIVE_CONFIG_DB;
assert.ok(source, "Set YILIAN_LIVE_CONFIG_DB to the existing model configuration database");
const isolated = await mkdtemp(join(tmpdir(), "yilian-native-live-"));
const workspace = join(isolated, "workspace");
const evidence = resolve(process.env.YILIAN_LIVE_EVIDENCE || join(root, "target/native-live-interaction"));
await Promise.all([mkdir(workspace), mkdir(evidence, { recursive: true })]);
const jobs = [];
let browser;
let stage = "startup";

async function port() {
  const socket = createServer();
  socket.listen(0, "127.0.0.1");
  await once(socket, "listening");
  const result = socket.address().port;
  await new Promise(done => socket.close(done));
  return result;
}

function launch(command, args, env, cwd = root) {
  const child = spawn(command, args, { cwd, env: { ...process.env, ...env }, windowsHide: true, stdio: ["ignore", "pipe", "pipe"] });
  const output = [];
  for (const pipe of [child.stdout, child.stderr]) pipe.on("data", data => output.push(String(data)));
  jobs.push(child);
  return { child, output };
}

async function until(action, description, timeout = 90000) {
  const deadline = Date.now() + timeout;
  let last;
  while (Date.now() < deadline) {
    try { const result = await action(); if (result) return result; }
    catch (error) { last = error; }
    await new Promise(done => setTimeout(done, 200));
  }
  throw new Error(`${description}${last ? ` (${last.message})` : ""}`);
}

try {
  const profile = process.env.YILIAN_LIVE_MODEL;
  const seed = launch("python", [join(import.meta.dirname, "native-live-seed.py"), source, join(isolated, "data/yilianqianyan-v1.db"), ...(profile ? [profile] : [])], {});
  const [seedExit] = await once(seed.child, "close");
  assert.equal(seedExit, 0, "configuration reference seeding failed");
  const model = JSON.parse(seed.output.join(""));
  const [appPort, apiPort, cdpPort] = await Promise.all([port(), port(), port()]);
  const appUrl = `http://127.0.0.1:${appPort}`;
  const apiUrl = `http://127.0.0.1:${apiPort}`;
  const cdpUrl = `http://127.0.0.1:${cdpPort}`;
  launch(process.execPath, [join(root, "frontend/node_modules/vite/bin/vite.js"), "--host", "127.0.0.1", "--port", String(appPort), "--strictPort"], { VITE_API_BASE: apiUrl }, join(root, "frontend"));
  await until(async () => (await fetch(appUrl)).ok, "Vite");
  const native = launch(process.env.YILIAN_LIVE_NATIVE_BINARY || join(root, "target/debug/yilian-native-browser-fixture.exe"), [], {
    YILIAN_NATIVE_FIXTURE_ROOT: isolated, YILIAN_NATIVE_BROWSER_FRONTEND_URL: appUrl, YILIAN_HOST: `127.0.0.1:${apiPort}`,
    YILIAN_ALLOWED_ORIGINS: appUrl, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${cdpPort}`,
  }, join(root, "src-tauri"));
  await until(async () => (await fetch(cdpUrl + "/json/version")).ok, "native WebView2 CDP");
  browser = await chromium.connectOverCDP(cdpUrl);
  const page = await until(() => browser.contexts().flatMap(context => context.pages()).find(tab => tab.url().startsWith(appUrl)), "Tauri main WebView");
  const faults = new Map();
  const countFault = key => faults.set(key, (faults.get(key) ?? 0) + 1);
  const resourcePath = raw => {
    try { return new URL(raw).pathname.replace(/[0-9a-f]{8}-[0-9a-f-]{27,}/gi, ":id"); }
    catch { return "unresolved"; }
  };
  page.on("pageerror", () => countFault("pageerror"));
  page.on("response", response => { if (response.status() >= 400) countFault(`${response.status()} ${resourcePath(response.url())}`); });
  page.on("requestfailed", request => countFault(`${request.failure()?.errorText || "failed"} ${resourcePath(request.url())}`));
  await page.evaluate(() => sessionStorage.setItem("yilian.setup.deferred", "true"));
  await until(async () => (await fetch(apiUrl + "/api/health")).ok, "native backend");
  const api = async (method, path, body) => page.evaluate(async ({ url, method, path, body }) => {
    const token = await window.__TAURI_INTERNALS__.invoke("get_control_session_token");
    const response = await fetch(url + path, { method, headers: { "Content-Type": "application/json", "X-Yilian-Control-Session": token }, body: body === undefined ? undefined : JSON.stringify(body) });
    const payload = await response.json().catch(() => null);
    return { status: response.status, payload };
  }, { url: apiUrl, method, path, body });
  const requireApi = async (method, path, body) => {
    const { status, payload } = await api(method, path, body);
    assert.ok(status >= 200 && status < 300, `${method} ${path}: ${status} ${payload?.code || ""}`);
    return payload;
  };

  stage = "configured provider readiness";
  const readiness = await requireApi("GET", "/api/chat/readiness");
  assert.ok(readiness.available, "configured profile has no resolvable credential");
  assert.equal(readiness.model, model.model);
  console.log(`WebView2 Tauri IPC ready; configured HTTPS provider ${model.host}, model ${model.model}`);
  const appInfo = { model, native: true, webview: await page.evaluate(() => navigator.userAgent), app: await page.title() };
  await writeFile(join(evidence, "runtime.json"), JSON.stringify(appInfo, null, 2));

  if (process.argv.includes("--probe")) {
    stage = "native console and transport probe";
    const home = await requireApi("POST", "/api/conversations", { title: "原生传输诊断" });
    await page.goto(appUrl + `/chat/${home.id}`);
    await page.getByRole("textbox", { name: "聊天输入" }).waitFor();
    await page.getByRole("button", { name: "并行运行" }).click();
    await page.getByRole("dialog", { name: "当前会话的并行运行" }).waitFor();
    await page.goto(appUrl + "/memory");
    await page.getByRole("tab", { name: "待审阅" }).click();
    await page.getByText("暂无待审阅候选", { exact: true }).waitFor();
    const diagnostic = { ...appInfo, requests: Object.fromEntries(faults) };
    await writeFile(join(evidence, "transport-probe.json"), JSON.stringify(diagnostic, null, 2));
    console.log(JSON.stringify(diagnostic.requests));
  } else if (process.argv.includes("--memory-text")) {
    stage = "native AI from saved memory";
    const { verifyNativeMemoryText } = await import("./native-memory-text-live.mjs");
    await verifyNativeMemoryText({page,appUrl,requireApi,evidence,appInfo});
  } else if (process.argv.includes("--relations")) {
    stage = "native AI relation graph";
    const { verifyNativeRelationGraph } = await import("./native-relation-live.mjs");
    await verifyNativeRelationGraph({ page, appUrl, apiUrl, requireApi, evidence, appInfo });
  } else if (process.argv.includes("--continuations")) {
    stage = "approval and question continuations";
    const run = launch(process.execPath, [join(import.meta.dirname, "native-steering-live.mjs")], {
      YILIAN_LIVE_CDP_URL: cdpUrl, YILIAN_LIVE_APP_URL: appUrl, YILIAN_LIVE_API_URL: apiUrl,
      YILIAN_LIVE_WORKSPACE: workspace, YILIAN_LIVE_STEERING_EVIDENCE: join(evidence, "continuations"),
    });
    const [code] = await once(run.child, "close");
    console.log(run.output.join(""));
    assert.equal(code, 0, "native steering scenarios");
  } else {
    await requireApi("PUT", "/api/security/approval-mode", { mode: "full_approval" });
    await requireApi("POST", "/api/security/grants", { permission_id: "filesystem.write", effect: "allow", resource: { type: "filesystem", root: workspace, recursive: true } });
    const home = await requireApi("POST", "/api/conversations", { title: "原生并行验收" });
    await page.goto(appUrl + `/chat/${home.id}`);
    const composer = page.getByRole("textbox", { name: "聊天输入" });
    async function send(message) {
      await composer.fill(message);
      await page.getByRole("button", { name: "发送消息", exact: true }).click();
    }
    const pending = async () => (await requireApi("GET", "/api/approvals/pending")).approvals;
    const taskText = name => `请实际调用 write_file 写入 ${name}.txt，绝对路径为 ${join(workspace, `${name}.txt`)}，内容严格为 ${name}_WEBVIEW2_OK。只做这一项操作。`;
    const lanesPath = `/api/conversations/${home.id}/lanes`;
    const lanes = () => requireApi("GET", lanesPath);
    stage = "parallel tasks with live model";
    await send(`新任务：${taskText("alpha")}`);
    const alpha = await until(async () => (await lanes()).find(lane => lane.kind === "work" && lane.title.includes("alpha")) || (await lanes()).find(lane => lane.kind === "work"), "first lane accepted");
    await until(async () => (await pending()).find(item => item.conversation_id === alpha.id && item.tool_name === "write_file"), "first live write approval");
    await send(`新任务：${taskText("beta")}`);
    const beta = await until(async () => (await lanes()).find(lane => lane.kind === "work" && lane.id !== alpha.id), "second lane accepted");
    await until(async () => (await pending()).find(item => item.conversation_id === beta.id && item.tool_name === "write_file"), "second live write approval");
    assert.equal(existsSync(join(workspace, "alpha.txt")), false);
    assert.equal(existsSync(join(workspace, "beta.txt")), false);
    await send("闲聊：请用一句中文问候我，带上‘原生会话’四个字。");
    const chat = await until(async () => (await lanes()).find(lane => lane.kind === "chat" && lane.run_status === "completed"), "live parallel chat completed");
    const chatDetail = await requireApi("GET", `/api/conversations/${chat.id}`);
    assert.ok(chatDetail.messages.some(item => item.role === "assistant" && item.content.includes("原生会话")));
    assert.equal((await lanes()).filter(lane => lane.run_status === "waiting_approval").length, 2);
    await page.getByText(/原生会话/).last().waitFor();
    await page.screenshot({ path: join(evidence, "01-parallel-base.png"), animations: "disabled" });
    await page.getByRole("button", { name: "并行运行", exact: true }).click();
    const parallelDialog = page.getByRole("dialog", { name: "当前会话的并行运行" });
    await parallelDialog.getByText("等待审批", { exact: true }).nth(1).waitFor();
    await page.screenshot({ path: join(evidence, "02-parallel-full.png"), animations: "disabled" });
    await parallelDialog.screenshot({ path: join(evidence, "01-native-parallel.png"), animations: "disabled" });
    await page.keyboard.press("Escape");
    stage = "isolated cancel and approve";
    await requireApi("POST", "/api/chat/task-command", { conversation_id: beta.id, message: "取消当前任务" });
    const approval = (await pending()).find(item => item.conversation_id === alpha.id);
    assert.ok(approval);
    const approved = await page.evaluate(async ({ url, id, conversation }) => {
      const token = await window.__TAURI_INTERNALS__.invoke("get_control_session_token");
      const res = await fetch(`${url}/api/approvals/${id}/approve`, { method: "POST", headers: { "Content-Type": "application/json", "X-Yilian-Control-Session": token }, body: JSON.stringify({ conversation_id: conversation }) });
      await res.text();
      return res.status;
    }, { url: apiUrl, id: approval.approval_id, conversation: alpha.id });
    assert.equal(approved, 200);
    const written = await readFile(join(workspace, "alpha.txt"), "utf8");
    assert.equal(written.trim(), "alpha_WEBVIEW2_OK");
    assert.equal(existsSync(join(workspace, "beta.txt")), false);

    stage = "live model memory review";
    const sourceChat = await requireApi("POST", "/api/conversations", { title: "原生记忆审阅来源" });
    await page.goto(appUrl + `/chat/${sourceChat.id}`);
    await send("对这个验收工作空间，我长期用 UTF-8 导出 CSV，并希望所有报表统一用简体中文。请只确认收到这条信息。");
    await until(async () => (await requireApi("GET", `/api/conversations/${sourceChat.id}`)).run_status === "completed", "live memory source reply");
    await page.goto(appUrl + "/memory");
    await page.getByRole("tab", { name: "待审阅" }).click();
    await page.getByRole("button", { name: "从对话提取" }).click();
    await page.getByLabel("来源对话", { exact: true }).selectOption(sourceChat.id);
    await page.getByRole("button", { name: "生成候选" }).click();
    await page.getByText(/已生成 [1-9]\d* 条待审阅候选/).waitFor({ timeout: 90000 });
    assert.equal((await requireApi("GET", "/api/memories")).length, 0);
    const candidates = (await requireApi("GET", "/api/memories/candidates")).entries;
    const candidate = candidates.find(item => item.source_conversation_id === sourceChat.id && /UTF-8/i.test(item.content));
    assert.ok(candidate?.source_message_id, "live model proposed a correctly sourced UTF-8 candidate");
    const candidateButton = page.getByRole("button", { name: new RegExp(candidate.content.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")) });
    await candidateButton.waitFor();
    await page.screenshot({ path: join(evidence, "03-memory-base.png"), animations: "disabled" });
    await candidateButton.click();
    const reviewDialog = page.getByRole("dialog", { name: "审阅记忆候选" });
    await reviewDialog.getByRole("button", { name: "确认并保存" }).waitFor();
    await page.screenshot({ path: join(evidence, "04-memory-full.png"), animations: "disabled" });
    await reviewDialog.screenshot({ path: join(evidence, "02-native-memory-review.png"), animations: "disabled" });
    await page.getByLabel("适用范围").selectOption("conversation");
    await page.getByRole("button", { name: "确认并保存" }).click();
    await page.getByText("已保存为长期记忆", { exact: true }).waitFor();
    const saved = (await requireApi("GET", "/api/memories")).find(item => item.content === candidate.content);
    assert.ok(saved && saved.source === "manual");
    const governance = await requireApi("GET", `/api/memories/${saved.id}/governance`);
    assert.equal(governance.scope, `conversation:${sourceChat.id}`);
    await writeFile(join(evidence, "result.json"), JSON.stringify({ ...appInfo, parallelApprovals: 2, chatCompletedBeforeApproval: true, alphaWriteVerified: true, betaCancelledWithoutFile: true, liveCandidateReviewed: true }, null, 2));
    console.log("Native WebView2 parallel chat/tasks and memory review passed with the configured HTTPS provider.");
  }
  assert.equal(native.child.exitCode, null, "native app stayed alive");
} catch (error) {
  console.error(`[${stage}] ${error.message}`);
  await browser?.contexts()[0]?.pages()[0]?.screenshot({ path: join(evidence, "failure.png") }).catch(() => {});
  process.exitCode = 1;
} finally {
  await browser?.close();
  for (const child of jobs.reverse()) if (child.exitCode === null && child.signalCode === null) child.kill();
}
