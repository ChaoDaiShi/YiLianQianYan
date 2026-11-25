// End-to-end test for the real desktop monitor with owned Win32 forms and a local model.
import assert from "node:assert/strict";
import {spawn} from "node:child_process";
import {createServer} from "node:http";
import {mkdtemp, mkdir, readFile, writeFile} from "node:fs/promises";
import {randomUUID} from "node:crypto";
import {join, resolve, basename} from "node:path";
import {tmpdir} from "node:os";
import {chromium} from "playwright-core";

assert.equal(process.platform, "win32");
assert.equal(process.env.YILIAN_SOFTWARE_BUILD_CONFIRMED, "1");
const root = resolve(import.meta.dirname, "../..");
const binary = resolve(process.env.YILIAN_NATIVE_BROWSER_BINARY || "");
assert.equal(basename(binary), "yilian-native-browser-fixture.exe");
const isolated = await mkdtemp(join(tmpdir(), "yilian-software-monitor-"));
const fixture = join(isolated, "target");
await mkdir(fixture);
console.log(`[software-monitor] evidence ${isolated}`);

const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
const children = [];
const checks = [];
const logs = [];
const modelEvents = [];
const modelErrors = [];
const metadata = [];
let browser, token, failure, sequence = 0;

async function until(read, predicate, label, timeout = 20000) {
  const deadline = Date.now() + timeout;
  let value;
  while (Date.now() < deadline) {
    value = await read();
    if (predicate(value)) return value;
    await delay(100);
  }
  throw new Error(`${label}: ${JSON.stringify(value)}`);
}

function launch(file, args, cwd, env, label) {
  const child = spawn(file, args, { cwd, env: {...process.env, ...env}, windowsHide: true });
  children.push(child);
  child.on("error", error => logs.push(`[${label}] ${error.message}\n`));
  for (const stream of [child.stdout, child.stderr]) {
    stream.on("data", chunk => logs.push(`[${label}] ${chunk}`));
  }
  return child;
}

function check(description) {
  checks.push(description);
  console.log(`[software-monitor] PASS ${description}`);
}

async function listen(server) {
  await new Promise(resolve => server.listen(0, "127.0.0.1", resolve));
  return server.address().port;
}

async function freePort() {
  const server = createServer();
  const port = await listen(server);
  await new Promise(resolve => server.close(resolve));
  return port;
}

const model = createServer(async (request, response) => {
  try {
    let raw = "";
    for await (const chunk of request) raw += chunk;
    const body = JSON.parse(raw);
    assert.equal(body.stream, false);
    assert.ok(!body.tools?.length);
    assert.deepEqual(body.messages.map(message => message.role), ["system", "user"]);
    const facts = JSON.parse(body.messages[1].content);
    assert.deepEqual(Object.keys(facts).sort(), ["event", "observed_at_ms"]);
    assert.ok(["process_exited", "window_closed", "window_rebuilt", "notification_received"].includes(facts.event));
    assert.ok(!raw.includes("SYNTHETIC_BODY_ONLY"));
    assert.ok(Number.isInteger(facts.observed_at_ms));
    modelEvents.push(facts.event);
    response.setHeader("Content-Type", "application/json");
    response.end(JSON.stringify({
      id: "fixture-monitor",
      choices: [{index: 0, message: {role: "assistant", content: `合成解释 ${facts.event}`}, finish_reason: "stop"}],
    }));
  } catch (error) {
    modelErrors.push(error.message);
    response.writeHead(400);
    response.end("Invalid fixture request");
  }
});

const modelPort = await listen(model);
const backendPort = await freePort();
const frontendPort = await freePort();
const debugPort = await freePort();
const frontend = `http://127.0.0.1:${frontendPort}`;
const backend = `http://127.0.0.1:${backendPort}`;

async function api(method, path, body) {
  const response = await fetch(backend + path, {
    method,
    signal: AbortSignal.timeout(10000),
    headers: {"Content-Type": "application/json", "X-Yilian-Control-Session": token},
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const text = await response.text();
  assert.equal(response.status, 200, `${path}: ${text}`);
  return JSON.parse(text);
}

const statePath = join(fixture, "state.json");
const stateFile = () => readFile(statePath, "utf8")
  .then(raw => JSON.parse(raw.replace(/^\uFEFF/, ""))).catch(() => null);
async function command(action) {
  const stamp = `${++sequence}:${action}`;
  await writeFile(join(fixture, "command.txt"), stamp);
  const state = await until(stateFile, item => item?.action === stamp, action);
  metadata.push(state);
  return state;
}

try {
  const powershell = join(process.env.SystemRoot, "System32/WindowsPowerShell/v1.0/powershell.exe");
  const target = launch(powershell, [
    "-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden",
    "-File", join(root, "frontend/e2e/software-monitor-target.ps1"), "-FixtureDirectory", fixture,
  ], root, {}, "synthetic-window");
  const first = await until(stateFile, item => item?.action === "ready" && item.visible, "synthetic window ready");

  launch(process.execPath, [join(root, "frontend/node_modules/vite/bin/vite.js"), "--host", "127.0.0.1",
    "--port", String(frontendPort), "--strictPort"], join(root, "frontend"), {VITE_API_BASE: backend}, "vite");
  await until(() => fetch(frontend).then(r => r.ok).catch(() => false), Boolean, "Vite ready");
  const app = launch(binary, [], root, {
    OPENAI_API_KEY: "",
    YILIAN_MONITOR_TEST_KEY: "local-fixture-value",
    YILIAN_NATIVE_FIXTURE_ROOT: isolated,
    YILIAN_NATIVE_FIXTURE_KEY: "local-fixture-value",
    YILIAN_HOST: `127.0.0.1:${backendPort}`,
    YILIAN_ALLOWED_ORIGINS: frontend,
    YILIAN_NATIVE_BROWSER_FRONTEND_URL: frontend,
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${debugPort}`,
  }, "desktop");
  await until(() => fetch(`http://127.0.0.1:${debugPort}/json/list`, {signal: AbortSignal.timeout(1000)})
    .then(r => r.json()).catch(() => []), list => list.some(item => item.type === "page" && item.url.startsWith(frontend)), "desktop renderer", 40000);
  browser = await chromium.connectOverCDP(`http://127.0.0.1:${debugPort}`);
  const page = await until(() => browser.contexts().flatMap(context => context.pages()).find(candidate => candidate.url().startsWith(frontend)), Boolean, "desktop page");
  const pageErrors = [];
  page.on("pageerror", error => pageErrors.push(error.message));
  await page.evaluate(() => sessionStorage.setItem("yilian.setup.deferred", "true"));
  await page.goto(frontend + "/software", {waitUntil: "domcontentloaded"});
  token = await until(() => page.evaluate(() => window.__TAURI_INTERNALS__.invoke("get_control_session_token")), value => typeof value === "string" && value.length >= 32, "control session");
  await page.evaluate(() => window.__TAURI_INTERNALS__.invoke("native_software_bind"));
  const settings = await api("GET", "/api/settings");
  await api("PUT", "/api/settings", {...settings, model: {
    ...settings.model, provider: "openai", name: "monitor-fixture", base_url: `http://127.0.0.1:${modelPort}`,
    api_key_env: "YILIAN_MONITOR_TEST_KEY", api_key_ref: null, max_tokens: 256,
  }});
  const registration = await api("POST", "/api/software/registrations", {
    name: "监管受控窗口", adapter: "generic_fixture", executable: powershell,
    workspace_id: null, allow_observe: false, allow_actions: false,
  });
  const path = `/api/software/registrations/${registration.id}`;
  const candidates = await api("POST", path + "/discover", {});
  const candidate = candidates.candidates.find(item => item.identity.pid === target.pid && item.identity.hwnd === first.hwnd);
  assert.ok(candidate);
  const firstSession = await api("POST", path + "/attach", {identity: candidate.identity, operation_id: randomUUID()});
  await page.getByRole("button", {name: "刷新软件状态"}).evaluate(element => element.click());
  const toggle = page.getByRole("switch", {name: "后台监管"});
  const current = async () => (await api("GET", "/api/software")).monitors.find(item => item.registration_id === registration.id);
  await until(() => toggle.isEnabled(), Boolean, "monitor switch enabled");
  assert.equal(await toggle.getAttribute("aria-checked"), "false");
  await toggle.evaluate(element => element.click());
  const startedMonitor = await until(current, item => item?.state === "watching" && item.notification_state === "watching", "monitor and notification source enabled");
  assert.equal(startedMonitor.session_id, firstSession.id);
  await page.getByRole("link", {name: "更多", exact: true}).evaluate(element => element.click());
  await page.waitForURL("**/more");
  check("software switch enables lifecycle and notification monitoring across routes");

  async function expectAlert(kind) {
    const status = await until(current, item => item?.latest?.kind === kind && item.latest.explanation === `合成解释 ${kind}`, kind);
    assert.equal(status.latest.explaining, false);
    return status;
  }

  const defocused = await command("defocus");
  assert.ok(defocused.visible && !defocused.foreground && !defocused.covered && !defocused.minimized);
  const alternateWindows = await api("POST", path + "/discover", {});
  const alternate = alternateWindows.candidates.find(item => item.identity.pid === target.pid && item.identity.hwnd === defocused.alternate_hwnd);
  assert.ok(alternate, "same process provides a second independently attached window");
  const otherSession = await api("POST", path + "/attach", {identity:alternate.identity, operation_id:randomUUID()});
  await delay(2100);
  assert.equal((await current()).latest, null);
  await command("notify");
  await expectAlert("notification_received");
  await page.getByRole("region", {name: "系统轻通知"}).getByText(/AI 解释：合成解释 notification_received/).waitFor();
  check("a real Windows notification from the defocused registered app reaches AI and the global reminder");
  await command("close");
  const windowClosed = await expectAlert("window_closed");
  await page.getByRole("region", {name: "系统轻通知"}).getByText(/AI 解释：合成解释 window_closed/).waitFor();
  await command("notify");
  const windowlessNotification = await expectAlert("notification_received");
  assert.equal(windowlessNotification.latest_lifecycle.id, windowClosed.latest.id);
  assert.equal(windowlessNotification.latest_notification.id, windowlessNotification.latest.id);
  const windowless = await until(() => api("GET", "/api/software"), snapshot => [firstSession, otherSession].every(attached => snapshot.sessions.find(item => item.id === attached.id)?.state === "lost"), "both controlled windows closed");
  await writeFile(join(isolated,"windowless-session-evidence.json"), JSON.stringify({sessions:windowless.sessions, capabilities:windowless.capability_evidence, monitors:windowless.monitors},null,2));
  const dimension = (id, name) => windowless.capability_evidence.find(row => row.session_id === id)?.entries.find(row => row.dimension === name);
  assert.equal(dimension(firstSession.id, "background_run")?.evidence_id, windowClosed.latest.id);
  assert.equal(dimension(otherSession.id, "background_run")?.status, "unverified");
  for (const session of [firstSession, otherSession]) {
    assert.equal(dimension(session.id, "background_interaction")?.status, "partial");
    assert.equal(dimension(session.id, "background_interaction")?.source, "windows_notification_only");
  }
  check("windowless process evidence stays with the monitored session; later application notification preserves it and grants no window action");
  const rebuilt = await command("rebuild");
  assert.notEqual(rebuilt.hwnd, first.hwnd);
  await expectAlert("window_rebuilt");
  check("defocused window closes and rebuilds, with an AI reminder outside the workbench");

  const covered = await command("cover");
  assert.ok(covered.visible && covered.covered && !covered.foreground && !covered.minimized);
  const latestBeforeCoverClose = (await current()).latest.id;
  await delay(2100);
  assert.equal((await current()).latest.id, latestBeforeCoverClose);
  await command("notify");
  await expectAlert("notification_received");
  check("a Windows notification reaches the monitor while the registered app is covered");
  await command("close");
  await expectAlert("window_closed");
  await command("rebuild");
  await expectAlert("window_rebuilt");
  check("occlusion is not an anomaly, and closing the covered window is detected");

  const minimized = await command("minimize");
  assert.ok(minimized.minimized && !minimized.foreground);
  const latestBeforeExit = (await current()).latest.id;
  await delay(2100);
  assert.equal((await current()).latest.id, latestBeforeExit);
  await command("notify");
  await expectAlert("notification_received");
  check("a Windows notification still reaches the monitor while the registered app is minimized");
  await command("exit");
  await until(() => target.exitCode, code => code !== null, "synthetic process exit");
  await expectAlert("process_exited");
  check("minimizing is not an anomaly, and process exit while minimized is detected");

  await page.goto(frontend + "/software", {waitUntil: "domcontentloaded"});
  await until(() => toggle.getAttribute("aria-checked"), value => value === "true", "monitor switch still on");
  await toggle.evaluate(element => element.click());
  await until(current, item => item === undefined, "monitor stopped");
  assert.deepEqual(modelErrors, []);
  assert.deepEqual(pageErrors, []);
  assert.deepEqual(modelEvents.slice(0, 8), ["notification_received", "window_closed", "notification_received", "window_rebuilt", "notification_received", "window_closed", "window_rebuilt", "notification_received"]);
  assert.equal(modelEvents.at(-1), "process_exited");
  check("switch disables monitoring and model requests contain only event types and timestamps");
  await page.evaluate(() => window.__TAURI_INTERNALS__.invoke("native_browser_fixture_schedule", {action: "exit"}));
  await until(() => app.exitCode, code => code === 0, "desktop shutdown");
} catch (error) {
  failure = String(error.stack || error);
  console.error(failure);
} finally {
  if (browser) await browser.close().catch(() => {});
  for (const child of children.reverse()) {
    if (child.exitCode === null && child.signalCode === null) {
      child.kill();
      await until(() => child.exitCode !== null || child.signalCode !== null, Boolean, "owned process cleanup", 5000);
    }
  }
  model.closeAllConnections();
  await new Promise(resolve => model.close(resolve));
  await writeFile(join(isolated, "services.log"), logs.join(""));
  await writeFile(join(isolated, "result.json"), JSON.stringify({
    status: failure ? "failed" : "passed", failure, checks, metadata, modelEvents, modelErrors,
    boundary: "Owned synthetic Win32 windows, local model fixture and lifecycle metadata only.",
  }, null, 2));
}
if (failure) process.exitCode = 1;
