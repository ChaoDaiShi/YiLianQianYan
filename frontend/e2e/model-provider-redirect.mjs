import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdtemp } from "node:fs/promises";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

const root = resolve(import.meta.dirname, "../..");
const isolated = await mkdtemp(join(tmpdir(), "yilian-provider-redirect-"));
const token = "model-redirect-fixture-control-session-0000000000";
const message = "synthetic-private-model-input";
const targetRequests = [];
const sameOriginRequests = [];
const logs = [];

async function listen(server) {
  await new Promise(done => server.listen(0, "127.0.0.1", done));
  return `http://127.0.0.1:${server.address().port}`;
}

async function readBody(request) {
  const chunks = [];
  for await (const chunk of request) chunks.push(chunk);
  return Buffer.concat(chunks).toString();
}

function completion(response) {
  response.writeHead(200, { "Content-Type": "text/event-stream" });
  response.end('data: {"choices":[{"index":0,"delta":{"content":"同源模型已返回"},"finish_reason":"stop"}]}\n\ndata: [DONE]\n\n');
}

const other = createServer(async (request, response) => {
  const body = await readBody(request);
  targetRequests.push({ method: request.method, body });
  completion(response);
});
const otherOrigin = await listen(other);
const configured = createServer(async (request, response) => {
  const body = await readBody(request);
  if (request.url === "/internal/chat/completions") {
    response.writeHead(307, { Location: "/same-origin-completion" });
    response.end();
  } else if (request.url === "/same-origin-completion") {
    sameOriginRequests.push({ method: request.method, body });
    completion(response);
  } else {
    response.writeHead(307, { Location: `${otherOrigin}/unconfigured-completion` });
    response.end();
  }
});
const configuredOrigin = await listen(configured);
const reservation = createServer();
const backend = await listen(reservation);
await new Promise(done => reservation.close(done));
const server = spawn(join(root, "target/debug/yilian-server.exe"), [], {
  cwd: root, windowsHide: true, stdio: ["ignore", "pipe", "pipe"],
  env: {
    ...process.env, YILIAN_HOST: new URL(backend).host, YILIAN_DATA_DIR: join(isolated, "state"),
    YILIAN_WORKSPACE: isolated, YILIAN_CONTROL_SESSION_TOKEN: token, YILIAN_MODEL_REDIRECT_FIXTURE: "loopback-fixture-key",
  },
});
for (const stream of [server.stdout, server.stderr]) stream.on("data", chunk => logs.push(String(chunk)));

async function api(method, path, body) {
  return fetch(backend + path, {
    method, signal: AbortSignal.timeout(30_000),
    headers: { "Content-Type": "application/json", "X-Yilian-Control-Session": token },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
}

async function chatAt(baseUrl) {
  const current = await api("GET", "/api/settings");
  assert.equal(current.status, 200);
  const settings = await current.json();
  const update = await api("PUT", "/api/settings", {
    ...settings, model: { ...settings.model, name: "redirect-fixture", provider: "custom", base_url: baseUrl,
      api_key_env: "YILIAN_MODEL_REDIRECT_FIXTURE", api_key_ref: null },
  });
  assert.equal(update.status, 200);
  const reply = await api("POST", "/api/chat", { message, workspace_mode: "chat" });
  assert.equal(reply.status, 200);
  return reply.text();
}

try {
  const deadline = Date.now() + 30_000;
  while (true) {
    try { if ((await api("GET", "/api/health")).ok) break; } catch { /* service is starting */ }
    assert.ok(Date.now() < deadline, "backend startup timed out");
    await new Promise(done => setTimeout(done, 120));
  }
  const blocked = await chatAt(configuredOrigin);
  assert.equal(targetRequests.length, 0, `Unconfigured origin received ${targetRequests.length} requests; POST copied: ${targetRequests.some(item => item.method === "POST" && item.body.includes(message))}`);
  assert.match(blocked, /模型服务重定向/);
  assert.ok(!blocked.includes(otherOrigin));
  const allowed = await chatAt(`${configuredOrigin}/internal`);
  assert.match(allowed, /同源模型已返回/);
  assert.equal(sameOriginRequests.length, 1);
  assert.equal(sameOriginRequests[0].method, "POST");
  assert.ok(sameOriginRequests[0].body.includes(message));
  console.log(JSON.stringify({ unconfiguredOriginPosts: targetRequests.length, sameOriginPosts: sameOriginRequests.length }));
} catch (error) {
  console.error(error, logs.slice(-5).join("").slice(-3000));
  process.exitCode = 1;
} finally {
  server.kill();
  for (const fixture of [configured, other]) {
    fixture.closeAllConnections();
    await new Promise(done => fixture.close(done));
  }
}
