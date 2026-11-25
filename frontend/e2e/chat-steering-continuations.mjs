import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { existsSync } from "node:fs";
import { mkdtemp, readFile } from "node:fs/promises";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

const root = resolve(import.meta.dirname, "../..");
const temp = await mkdtemp(join(tmpdir(), "yilian-steering-continuations-"));
const token = "steering-continuations-fixture-token-000000000000";
const originalFile = join(temp, "approved.txt");
const staleFile = join(temp, "stale.txt");
const supplement = "先不要写文件，只解释方案。";
const scenarios = [
  { kind: "approval", prompt: "请写入审批测试文件", tool: "write_file", arguments: { path: originalFile, content: "唯一的已批准写入" } },
  { kind: "question", prompt: "请写入补问测试文件", tool: "ask_user", arguments: { title: "想使用什么格式？", kind: "text", allow_text: true } },
].filter(scenario => !process.argv[2] || process.argv[2] === scenario.kind);
assert.ok(scenarios.length, "expected approval or question");
const calls = new Map();
function deferred() {
  let resolve;
  const promise = new Promise(done => { resolve = done; });
  return { promise, resolve };
}
for (const scenario of scenarios) {
  scenario.entered = deferred();
  scenario.release = deferred();
}
function sendModel(response, delta, finishReason) {
  response.writeHead(200, { "Content-Type": "text/event-stream" });
  response.write(`data: ${JSON.stringify({ choices: [{ index: 0, delta, finish_reason: finishReason }], usage: { prompt_tokens: 18, completion_tokens: 7, total_tokens: 25 } })}\n\n`);
  response.end("data: [DONE]\n\n");
}
const provider = createServer(async (request, response) => {
  const chunks = [];
  for await (const chunk of request) chunks.push(chunk);
  const body = JSON.parse(Buffer.concat(chunks).toString());
  const scenario = scenarios.find(item => body.messages.some(message => message.role === "user" && message.content === item.prompt));
  if (!scenario) {
    sendModel(response, { content: "新的后续对话。" }, "stop");
    return;
  }
  const count = (calls.get(scenario.kind) ?? 0) + 1;
  calls.set(scenario.kind, count);
  const tool = (id, name, args) => ({ tool_calls: [{ index: 0, id, type: "function", function: { name, arguments: JSON.stringify(args) } }] });
  if (count === 1) {
    sendModel(response, tool(`original-${scenario.kind}`, scenario.tool, scenario.arguments), "tool_calls");
  } else if (count === 2) {
    scenario.entered.resolve();
    await scenario.release.promise;
    sendModel(response, tool(`stale-${scenario.kind}`, "write_file", { path: staleFile, content: "过期工具提案" }), "tool_calls");
  } else {
    scenario.replanned = body;
    sendModel(response, { content: "已按补充要求仅解释方案。" }, "stop");
  }
});
async function listen(server) {
  await new Promise(done => server.listen(0, "127.0.0.1", done));
  return server.address().port;
}
const portProbe = createServer();
const backendPort = await listen(portProbe);
await new Promise(done => portProbe.close(done));
const providerPort = await listen(provider);
const base = `http://127.0.0.1:${backendPort}`;
const child = spawn(process.env.YILIAN_E2E_SERVER || join(root, "target", "debug", process.platform === "win32" ? "yilian-server.exe" : "yilian-server"), [], {
  cwd: root,
  windowsHide: true,
  env: {
    ...process.env,
    YILIAN_HOST: `127.0.0.1:${backendPort}`,
    YILIAN_WORKSPACE: temp,
    YILIAN_DATA_DIR: join(temp, "state"),
    YILIAN_CONTROL_SESSION_TOKEN: token,
    YILIAN_STEERING_FIXTURE_KEY: "local-fixture",
  },
  stdio: ["ignore", "pipe", "pipe"],
});
const logs = [];
for (const stream of [child.stdout, child.stderr]) stream.on("data", chunk => logs.push(String(chunk)));
const headers = { "Content-Type": "application/json", "X-Yilian-Control-Session": token };
async function request(method, path, body) {
  const result = await fetch(base + path, { method, headers, body: body === undefined ? undefined : JSON.stringify(body), signal: AbortSignal.timeout(20000) });
  return result;
}
async function json(method, path, body) {
  const response = await request(method, path, body);
  assert.ok(response.ok, `${path}: ${response.status}`);
  return response.json();
}
async function readEvents(response, match = () => false) {
  assert.ok(response.ok, `unexpected SSE status ${response.status}`);
  const reader = response.body.getReader();
  const decoder = new TextDecoder();
  const events = [];
  let pending = "";
  while (true) {
    const { done, value } = await reader.read();
    if (done) return { events, matched: undefined };
    pending += decoder.decode(value, { stream: true });
    const frames = pending.split("\n\n");
    pending = frames.pop();
    for (const frame of frames) {
      const data = frame.split("\n").find(line => line.startsWith("data: "));
      if (!data) continue;
      const event = JSON.parse(data.slice(6));
      events.push(event);
      if (match(event)) return { events, matched: event, reader };
    }
  }
}
try {
  const startupDeadline = Date.now() + 15000;
  while (true) {
    try { if ((await request("GET", "/api/health")).ok) break; } catch {}
    assert.ok(Date.now() < startupDeadline, "backend startup timeout");
    await new Promise(done => setTimeout(done, 100));
  }
  const settings = await json("GET", "/api/settings");
  await json("PUT", "/api/settings", { ...settings, model: {
    ...settings.model, provider: "openai", name: "steering-fixture", base_url: `http://127.0.0.1:${providerPort}`, api_key_env: "YILIAN_STEERING_FIXTURE_KEY", api_key_ref: null,
  } });
  await json("POST", "/api/security/grants", { permission_id: "filesystem.write", effect: "allow", resource: { type: "filesystem", root: temp, recursive: true } });
  await json("PUT", "/api/security/approval-mode", { mode: "full_approval" });
  for (const scenario of scenarios) {
    const { id } = await json("POST", "/api/conversations", { title: scenario.kind });
    const initial = await readEvents(await request("POST", "/api/chat", { conversation_id: id, message: scenario.prompt, workspace_mode: "work" }));
    const pending = initial.events.find(event => event.type === (scenario.kind === "approval" ? "approval_required" : "user_input_required"));
    assert.ok(pending, `missing ${scenario.kind} wait event`);
    const turnId = initial.events.find(event => event.accepts_steering)?.turn_id;
    assert.ok(turnId, "initial conversation has a real turn ID");
    assert.equal((await request("POST", "/api/chat/steer", { conversation_id: id, turn_id: turnId, message: supplement })).status, 409);
    const response = scenario.kind === "approval"
      ? await request("POST", `/api/approvals/${pending.approval_id}/approve`, { conversation_id: id })
      : await request("POST", `/api/conversation-inputs/${pending.args.question_id}/answer`, { revision: pending.args.revision, text: "Markdown" });
    // Keep draining while the model waits, so admission and event ordering are observed independently.
    const streamed = readEvents(response);
    await scenario.entered.promise;
    const accepted = await request("POST", "/api/chat/steer", { conversation_id: id, turn_id: turnId, message: supplement });
    scenario.release.resolve();
    const resumed = await streamed;
    assert.equal(accepted.status, 200, `${scenario.kind}: should accept steering in the resumed turn`);
    const { message_id: messageId } = await accepted.json();
    assert.ok(resumed.events.some(event => event.accepts_steering && event.turn_id === turnId), "same turn is announced after continuation starts");
    assert.ok(resumed.events.some(event => event.type === "steering_applied" && event.message_id === messageId), "applied supplement emits its persisted identity");
    assert.ok(resumed.events.some(event => event.type === "done" && event.result === "已按补充要求仅解释方案。"));
    assert.equal(calls.get(scenario.kind), 3);
    assert.ok(!existsSync(staleFile), "model's stale tool proposal must not execute");
    assert.ok(scenario.replanned.messages.some(message => message.role === "user" && message.content === supplement));
    assert.ok(!scenario.replanned.tools?.some(tool => tool.function.name === "write_file"));
    const detail = await json("GET", `/api/conversations/${id}`);
    assert.equal(detail.messages.filter(message => message.id === messageId).length, 1);
    assert.equal(detail.messages.filter(message => message.tool_call_id === `original-${scenario.kind}`).length, 1);
    if (scenario.kind === "approval") {
      assert.equal(resumed.events.filter(event => event.type === "tool_start" && event.tool_call_id === "original-approval").length, 1);
      assert.equal(resumed.events.filter(event => event.type === "tool_end" && event.tool_call_id === "original-approval" && event.status === "success").length, 1);
    }
    assert.ok(!resumed.events.some(event => event.tool_call_id === `stale-${scenario.kind}`), "discarded proposals must not enter the execution trace");
    assert.ok(!detail.messages.some(message => message.tool_call_id === `stale-${scenario.kind}`));
    assert.equal(detail.run_status, "completed");
    const turns = await json("GET", `/api/conversations/${id}/turns`);
    assert.equal((turns.turns ?? turns).length, 1);
    assert.equal((turns.turns ?? turns)[0].id, turnId);
  }
  if (scenarios.some(scenario => scenario.kind === "approval")) {
    assert.equal(await readFile(originalFile, "utf8"), "唯一的已批准写入");
  }
  console.log("Approval and question continuations preserved their turn, replanned, and did not replay the approved operation.");
} catch (error) {
  console.error(error, logs.join("").slice(-3000));
  process.exitCode = 1;
} finally {
  for (const scenario of scenarios) scenario.release.resolve();
  provider.closeAllConnections();
  provider.close();
  child.kill();
}
