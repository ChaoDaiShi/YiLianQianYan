import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { once } from "node:events";
import { mkdir, mkdtemp, readFile, stat, writeFile } from "node:fs/promises";
import { createServer as httpServer } from "node:http";
import { createServer as tcpServer } from "node:net";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { chromium } from "playwright-core";

// The dev-only native binary owns a disposable data/profile root and the real
// local API. Only embeddings are served by a deterministic loopback fixture.
const root = resolve(import.meta.dirname, "../..");
const binary = resolve(process.env.YILIAN_SPACE_NATIVE_BINARY || join(root, "target/debug/yilian-native-browser-fixture.exe"));
const isolated = await mkdtemp(join(tmpdir(), "yilian-memory-space-"));
const evidence = resolve(process.env.YILIAN_SPACE_EVIDENCE || join(root, "target/native-memory-space"));
await mkdir(evidence, { recursive: true });
const children = [];
const report = { native: true, actualSQLite: true, syntheticRecords: true, embeddingFixture: true, remoteCloud: false, screenshots: [], checks: [] };
const paths = {
  graph: "/api/memories/graph",
  relation: "/api/memories/relations",
  memory: "/api/memories",
  candidates: "/api/memories/candidates",
};
const wait = ms => new Promise(done => setTimeout(done, ms));
let client, fixture, stage = "startup";

async function until(probe, description, timeout = 30000) {
  let last;
  const deadline = Date.now() + timeout;
  for (; Date.now() < deadline; await wait(150)) {
    try { const result = await probe(); if (result) return result; }
    catch (error) { last = error.message; }
  }
  throw new Error(description + (last ? `: ${last}` : ""));
}

async function freePort() {
  const socket = tcpServer();
  socket.listen(0, "127.0.0.1");
  await once(socket, "listening");
  const result = socket.address().port;
  await new Promise(done => socket.close(done));
  return result;
}

function launch(command, args, env = {}, cwd = root) {
  const child = spawn(command, args, { cwd, windowsHide: true, stdio: ["ignore", "pipe", "pipe"], env: { ...process.env, ...env } });
  const messages = [];
  for (const stream of [child.stdout, child.stderr]) stream.on("data", chunk => messages.push(String(chunk)));
  children.push(child);
  return { child, messages };
}

async function stop(child) {
  if (!child || child.exitCode !== null || child.signalCode !== null) return;
  const closed = once(child, "close");
  child.kill();
  await closed;
}

async function startFrontend() {
  const [appPort, apiPort] = await Promise.all([freePort(), freePort()]);
  const app = `http://127.0.0.1:${appPort}`, backend = `http://127.0.0.1:${apiPort}`;
  const vite = launch(process.execPath, [join(root, "frontend/node_modules/vite/bin/vite.js"), "--host", "127.0.0.1", "--port", String(appPort), "--strictPort"], { VITE_API_BASE: backend }, join(root, "frontend"));
  await until(async () => {
    if (vite.child.exitCode !== null) throw new Error("Vite exited");
    return (await fetch(app)).ok;
  }, "Vite startup");
  return { app, backend };
}

async function startNative({ app, backend }, dataRoot = isolated, executable = binary) {
  const cdpPort = await freePort();
  const native = launch(executable, [], {
    YILIAN_NATIVE_FIXTURE_ROOT: dataRoot,
    YILIAN_NATIVE_BROWSER_FRONTEND_URL: app,
    YILIAN_HOST: new URL(backend).host,
    YILIAN_ALLOWED_ORIGINS: app,
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${cdpPort}`,
    YILIAN_MEMORY_SPACE_EMBEDDING_KEY: "loopback-test-only",
  }, join(root, "src-tauri"));
  const cdpUrl = `http://127.0.0.1:${cdpPort}`;
  await until(async () => (await fetch(cdpUrl + "/json/version")).ok, "native CDP startup");
  const browser = await chromium.connectOverCDP(cdpUrl);
  const page = await until(() => browser.contexts().flatMap(context => context.pages()).find(tab => tab.url().startsWith(app)), "native main WebView2");
  const pageErrors = [];
  page.on("pageerror", error => pageErrors.push(error.message));
  await page.evaluate(() => sessionStorage.setItem("yilian.setup.deferred", "true"));
  assert.equal(await page.evaluate(() => typeof window.__TAURI_INTERNALS__?.invoke), "function");
  await until(async () => (await fetch(backend + "/api/health")).ok, "native local backend startup");
  return { browser, page, native: native.child, backend, app, pageErrors };
}

async function stopNative() {
  await client?.browser.close();
  await stop(client?.native);
}

async function rawApi(method, path, body) {
  return client.page.evaluate(async ({ backend, method, path, body }) => {
    const token = await window.__TAURI_INTERNALS__.invoke("get_control_session_token");
    const response = await fetch(backend + path, { method, headers: { "Content-Type": "application/json", "X-Yilian-Control-Session": token }, body: body === undefined ? undefined : JSON.stringify(body) });
    return { status: response.status, payload: await response.json().catch(() => null) };
  }, { backend: client.backend, method, path, body });
}

async function api(method, path, body) {
  const { status, payload } = await rawApi(method, path, body);
  assert.ok(status >= 200 && status < 300, `${method} ${path}: HTTP ${status} ${payload?.code || ""}`);
  return payload;
}

const memoryPath = id => `${paths.memory}/${id}`;
const relationPath = id => `${paths.relation}/${id}`;
const make = (content, extra = {}) => api("POST", paths.memory, { content, category: "note", source: "manual", ...extra });
const edit = (id, content) => api("PUT", memoryPath(id), { content });
const govern = id => api("GET", memoryPath(id) + "/governance");
const getGraph = (query = "") => api("GET", paths.graph + query);
const layers = (snapshot, id) => snapshot.histories[id]?.map(version => version.layer).sort((a, b) => a - b);
const graph = () => client.page.getByRole("region", { name: "记忆关系图", exact: true });
const space = () => client.page.getByRole("region", { name: "记忆网络立体视图", exact: true });
const button = name => client.page.getByRole("button", { name, exact: true });
const tab = name => client.page.getByRole("tab", { name, exact: true });
const marker = (id, layer) => space().locator(`[data-memory-id="${id}"][data-layer="${layer}"]`);
const flow = () => graph().locator(".react-flow__viewport");
const node = id => graph().locator(`.react-flow__node[data-id="memory:${id}"]`);
const camera = () => space().evaluate(element => [
  element.dataset.cameraAzimuth, element.dataset.cameraElevation, element.dataset.cameraZoom, element.dataset.orbiting,
]);

async function capture(name) {
  const size = await client.page.evaluate(() => ({ width: innerWidth, height: innerHeight, dpr: devicePixelRatio, scheme: document.documentElement.dataset.colorScheme }));
  await client.page.screenshot({ path: join(evidence, name), animations: "disabled" });
  report.screenshots.push({ name, ...size });
}

function timingStats(samples) {
  assert.ok(samples.length > 0, "performance sampling recorded observable browser work");
  const sorted = [...samples].sort((a, b) => a - b);
  const percentile = p => Number(sorted[Math.ceil(sorted.length * p) - 1].toFixed(2));
  return { samples: sorted.length, p50Ms: percentile(.5), p95Ms: percentile(.95), maxMs: percentile(1) };
}

async function blankCanvasPoint(locator, movement = { x: 55, y: 26 }) {
  const point = await locator.evaluate((element, movement) => {
    const box = element.getBoundingClientRect();
    for (const yf of [.18, .26, .36, .48, .61, .72]) {
      for (const xf of [.12, .22, .42, .6, .76]) {
        const x = box.x + box.width * xf, y = box.y + box.height * yf;
        if (document.elementFromPoint(x, y) === element && document.elementFromPoint(x + movement.x, y + movement.y) === element) return { x, y };
      }
    }
    return null;
  }, movement);
  assert.ok(point, "viewport contains a bare canvas path");
  return point;
}

async function measureOrbit() {
  const surface = space().locator("svg.memory-space-canvas");
  const bounds = await surface.boundingBox();
  const x = bounds.x + bounds.width * .24, y = bounds.y + bounds.height * .52;
  await surface.evaluate(svg => {
    const section = svg.closest(".memory-space-workspace");
    const trace = { frames: [], latencies: [] };
    let pendingTime = null, frameId, previousFrame;
    function receive(event) {
      if (event.isTrusted && (event.buttons & 4) && pendingTime === null) pendingTime = event.timeStamp;
    }
    function frame(now) {
      if (previousFrame !== undefined) trace.frames.push(now - previousFrame);
      previousFrame = now;
      frameId = requestAnimationFrame(frame);
    }
    const observer = new MutationObserver(() => {
      if (pendingTime === null) return;
      trace.latencies.push(performance.now() - pendingTime);
      pendingTime = null;
    });
    observer.observe(section, { attributes: true, attributeFilter: ["data-camera-azimuth", "data-camera-elevation"] });
    svg.addEventListener("pointermove", receive, true);
    frameId = requestAnimationFrame(frame);
    window.finishMemorySpaceProfile = () => {
      cancelAnimationFrame(frameId);
      observer.disconnect();
      svg.removeEventListener("pointermove", receive, true);
      return {
        ...trace,
        nodes: section.querySelectorAll("[data-version-id]").length,
        edges: section.querySelectorAll("[data-edge-id]").length,
        eventEdges: section.querySelectorAll("[data-event-edge]").length,
      };
    };
  });
  await client.page.mouse.move(x, y);
  await client.page.mouse.down({ button: "middle" });
  try {
    for (let frame = 1; frame <= 36; frame++) {
      const offset = frame * Math.PI / 18;
      await client.page.mouse.move(x + Math.sin(offset) * 70, y + Math.cos(offset) * 31, { steps: 2 });
      await client.page.evaluate(() => new Promise(requestAnimationFrame));
    }
  } finally {
    await client.page.mouse.up({ button: "middle" });
  }
  const sample = await client.page.evaluate(() => window.finishMemorySpaceProfile());
  assert.ok(sample.frames.length >= 30 && sample.latencies.length >= 8, "native performance trace captured sustained orbit");
  return {
    method: "WebView2 rAF intervals and trusted pointermove to committed camera DOM mutation. One continuous CDP middle-button orbit with 36 frame-paced positions; end-to-screen scanout is outside these measurements.",
    viewport: await client.page.evaluate(() => ({ width: innerWidth, height: innerHeight, dpr: devicePixelRatio })),
    versionNodes: sample.nodes, renderedEdges: sample.edges, eventEdges: sample.eventEdges,
    frameInterval: timingStats(sample.frames), pointerToCamera: timingStats(sample.latencies),
  };
}

async function appearance(scheme) {
  await client.page.goto(client.app + "/settings?section=appearance");
  await client.page.getByRole("radio", { name: scheme === "dark" ? /夜间/ : /白天/ }).click();
  await client.page.locator(`:root[data-color-scheme="${scheme}"]`).waitFor();
}

async function openGraph() {
  await client.page.goto(client.app + "/memory");
  await tab("长期记忆").click();
  await button("关系图谱").click();
  await graph().waitFor();
}

async function backupDatabase(source, destination) {
  const script = "import sqlite3,sys; s=sqlite3.connect(sys.argv[1]); d=sqlite3.connect(sys.argv[2]); s.backup(d); d.close(); s.close()";
  const task = launch("python", ["-c", script, source, destination]);
  const [code] = await once(task.child, "close");
  assert.equal(code, 0, "SQLite Online Backup API");
}

async function resizeNative(width, height) {
  const script = `
$ErrorActionPreference = 'Stop'
$owner = Get-Process -Id ([int]$env:YILIAN_SPACE_TEST_PID) -ErrorAction Stop
if (![String]::Equals($owner.Path,[IO.Path]::GetFullPath($env:YILIAN_SPACE_TEST_BINARY),[StringComparison]::OrdinalIgnoreCase)) { throw 'Unexpected process' }
if ($owner.MainWindowHandle -eq [IntPtr]::Zero) { throw 'No native main window' }
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class MemorySpaceWindow {
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h, IntPtr after, int x, int y, int cx, int cy, uint flags);
  [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int l,t,r,b; }
}
'@
[void][MemorySpaceWindow]::SetThreadDpiAwarenessContext([IntPtr](-4))
$h=$owner.MainWindowHandle
$client=New-Object MemorySpaceWindow+RECT
$outer=New-Object MemorySpaceWindow+RECT
if (![MemorySpaceWindow]::GetClientRect($h,[ref]$client) -or ![MemorySpaceWindow]::GetWindowRect($h,[ref]$outer)) { throw 'Cannot read HWND bounds' }
$dpi=[MemorySpaceWindow]::GetDpiForWindow($h)/96
$w=[int]($dpi*[int]$env:YILIAN_SPACE_TEST_WIDTH + ($outer.r-$outer.l) - ($client.r-$client.l))
$h2=[int]($dpi*[int]$env:YILIAN_SPACE_TEST_HEIGHT + ($outer.b-$outer.t) - ($client.b-$client.t))
if (![MemorySpaceWindow]::SetWindowPos($h,[IntPtr]::Zero,0,0,$w,$h2,0x16)) { throw 'Native resize failed' }
`;
  const task = launch("powershell.exe", ["-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden", "-EncodedCommand", Buffer.from(script, "utf16le").toString("base64")], {
    YILIAN_SPACE_TEST_PID: String(client.native.pid), YILIAN_SPACE_TEST_BINARY: binary,
    YILIAN_SPACE_TEST_WIDTH: String(width), YILIAN_SPACE_TEST_HEIGHT: String(height),
  });
  const [code] = await once(task.child, "close");
  assert.equal(code, 0, task.messages.join(""));
  await until(async () => client.page.evaluate(({ width, height }) => Math.abs(innerWidth - width) <= 4 && Math.abs(innerHeight - height) <= 4, { width, height }), "native window bounds");
}

async function startEmbeddingFixture() {
  let calls = 0;
  const server = httpServer((request, response) => {
    let body = "";
    request.on("data", chunk => { body += chunk; });
    request.on("end", () => {
      if (request.method !== "POST" || request.url !== "/embeddings") return response.writeHead(404).end();
      if (request.headers.authorization !== "Bearer loopback-test-only") return response.writeHead(401).end();
      calls += 1;
      const input = JSON.parse(body).input;
      const start = [...String(input)][0]?.codePointAt(0) ?? 0;
      response.writeHead(200, { "Content-Type": "application/json" }).end(JSON.stringify({ data: [{ embedding: [1, (start % 5) / 5, 0.2] }] }));
    });
  });
  server.listen(0, "127.0.0.1");
  await once(server, "listening");
  return { server, url: `http://127.0.0.1:${server.address().port}`, count: () => calls };
}

async function verifyMemorySpace(options) {
  stage = "create controlled records and relation separation";
  assert.equal((await getGraph("?limit=100&offset=0&withdrawn=false")).memories.length, 0);
  await openGraph();
  await graph().getByText("这里暂时还没有记忆记录",{exact:true}).waitFor();
  await button("添加第一条记忆").waitFor();
  const source = await api("POST", "/api/conversations", { title: "隔离验收：星桥校对来源" });
  const a = await make("星桥发布 · 初版发布窗口周一", { source_conversation_id: source.id });
  const b = await make("数据核验 · 初版要求逐项复核");
  const c = await make("巡检清单 · 回查归档字段");
  const d = await make("观测站点 · 尚未确认的关联");
  const withdrawn = await make("已停用校验样本 · 不在有效图和召回中");
  const confirmed = await api("POST", paths.relation, { source_memory_id: a.id, target_memory_id: b.id, label: "发布与数据核验对应" });
  const pending = await api("POST", paths.relation, { source_memory_id: c.id, target_memory_id: d.id, label: "尚需核对的线索", status: "pending" });
  await api("POST", memoryPath(withdrawn.id) + "/withdraw", { expected_revision: 1, reason: "验收撤回" });
  await edit(a.id, "星桥发布 · 修订发布窗口周三");
  await edit(a.id, "星桥发布 · 定稿发布窗口周五");
  await edit(a.id, "星桥发布 · 定稿发布窗口周五");
  let before = await getGraph();
  assert.deepEqual(layers(before, a.id), [0, 1, 2]);
  assert.deepEqual(layers(before, b.id), [0]);
  assert.equal(before.memories.find(item => item.id === a.id).content, "星桥发布 · 定稿发布窗口周五");
  assert.ok(!before.memories.some(item => item.id === withdrawn.id));
  const withdrawnProjection = await getGraph("?withdrawn=true");
  assert.deepEqual(layers(withdrawnProjection, withdrawn.id), [0, 1]);
  assert.equal(withdrawnProjection.histories[withdrawn.id].find(version => version.layer === 1).status, "withdrawn");
  assert.ok(!JSON.stringify(await api("GET", `${paths.memory}/retrieve?q=${encodeURIComponent("已停用校验样本")}`)).includes(withdrawn.id));
  assert.deepEqual(layers(before, c.id), [0]);
  assert.ok(before.relations.some(item => item.id === pending.id && item.status === "pending"));
  const edited = await api("PATCH", relationPath(confirmed.id), { expected_revision: confirmed.revision, label: "发布记录与核验清单对应" });
  before = await getGraph();
  assert.equal(edited.revision, confirmed.revision + 1);
  assert.deepEqual(layers(before, a.id), [0, 1, 2]);
  assert.deepEqual(layers(before, b.id), [0]);
  const linkHistory = before.relation_versions.filter(item => item.relation_id === confirmed.id).sort((u, v) => u.revision - v.revision);
  assert.deepEqual(linkHistory.map(version => version.label), ["发布与数据核验对应", "发布记录与核验清单对应"]);
  assert.ok(linkHistory[1].source_version_id && linkHistory[1].source_version_id !== linkHistory[0].source_version_id);
  assert.equal(linkHistory[0].target_version_id, linkHistory[1].target_version_id);
  assert.deepEqual(layers(before, d.id), [0]);
  report.checks.push("独立层数与同内容不重复计层", "关系独立修订并锚定真实版本", "撤回不进入有效结果及召回");

  stage = "explicit related memory candidate review";
  await openGraph();
  await client.page.getByText(/未配置向量模型.*按文本/).waitFor();
  await button("管理对应关系").click();
  const manage = client.page.getByRole("dialog", { name: "管理对应关系", exact: true });
  await manage.getByRole("button", { name: "发布记录与核验清单对应 · 已确认", exact: true }).click();
  const relation = client.page.getByRole("dialog", { name: "对应关系与历史", exact: true });
  await relation.getByText("关系历史（2）", { exact: true }).click();
  await relation.getByText(/发布与数据核验对应/).waitFor();
  await relation.getByRole("button", { name: "提议关联记忆纠正", exact: true }).click();
  const correction = client.page.getByRole("dialog", { name: "提议关联记忆纠正", exact: true });
  await correction.getByRole("combobox", { name: "要纠正的记忆", exact: true }).selectOption(b.id);
  await correction.getByRole("combobox", { name: "对应的变更（可选）", exact: true }).selectOption(`${a.id}:3`);
  await correction.getByRole("textbox", { name: "建议的新内容", exact: true }).fill("数据核验 · 增加发布窗口复核");
  await correction.getByRole("textbox", { name: "纠正依据", exact: true }).fill("用户根据发布窗口调整确认数据核验清单");
  await correction.getByRole("button", { name: "提交待审阅", exact: true }).click();
  await correction.waitFor({ state: "hidden" });
  const candidate = (await api("GET", paths.candidates)).entries.find(item => item.content === "数据核验 · 增加发布窗口复核");
  assert.ok(candidate, "linked correction entered review");
  assert.deepEqual(layers(await getGraph(), b.id), [0]);
  await tab("待审阅").click();
  await client.page.getByRole("button", { name: /数据核验 · 增加发布窗口复核/ }).click();
  const review = client.page.getByRole("dialog", { name: "审阅记忆候选", exact: true });
  await review.getByText("用户根据发布窗口调整确认数据核验清单", { exact: true }).waitFor();
  await review.getByText("数据核验 · 初版要求逐项复核", { exact: true }).waitFor();
  await capture("00-related-correction-review.png");
  await review.getByRole("button", { name: "确认并保存", exact: true }).click();
  await review.waitFor({ state: "hidden" });
  await client.page.getByText("已更新关联记忆", { exact: true }).waitFor();
  const already = await getGraph();
  assert.deepEqual(layers(already, a.id), [0, 1, 2]);
  assert.deepEqual(layers(already, b.id), [0, 1]);
  assert.equal(already.memories.find(item => item.id === b.id).content, candidate.content);
  assert.equal(already.histories[b.id].find(item => item.layer === 1).reason, "用户根据发布窗口调整确认数据核验清单");
  assert.equal(already.current_version_ids[a.id], `${a.id}:3`);
  assert.equal(already.current_version_ids[b.id], `${b.id}:2`);
  const linkedEvent = already.change_events.find(event => event.version_id === `${b.id}:2`);
  assert.ok(linkedEvent?.id && linkedEvent.related_version_id === `${a.id}:3` && linkedEvent.related_event_id, "review persists related event and exact endpoints");
  assert.equal((await api("GET", paths.candidates)).entries.some(item => item.id === candidate.id), false);
  const unchanged = await api("POST", relationPath(confirmed.id) + "/correction-candidate", {
    target_memory_id: b.id, content: candidate.content, reason: "相同内容不重复更新", expected_revision: (await govern(b.id)).revision,
  });
  assert.equal(unchanged.candidate_id, null);
  assert.deepEqual(layers(await getGraph(), b.id), [0, 1]);
  report.checks.push("关联项审阅前不改写，确认只新增该项的版本");

  stage = "removed chat source stays unavailable without deleting confirmed memory";
  await api("DELETE", "/api/conversations/" + source.id);
  const noSource = await getGraph();
  assert.ok(noSource.memories.find(item => item.id === a.id).source_removed);
  assert.ok(noSource.histories[a.id].every(version => version.source_removed));
  assert.ok(noSource.histories[a.id].every(version => !version.source_conversation_id));
  report.checks.push("聊天移除后历史内容保留、来源标为已移除且不可打开");

  stage = "native light 2D projection and 3D input";
  await client.page.emulateMedia({reducedMotion:"no-preference"});
  await appearance("light");
  await openGraph();
  await graph().getByText("当前展示有效记忆", { exact: true }).waitFor();
  const current = graph().getByRole("button", { name: /星桥发布.*定稿发布窗口周五/ });
  await current.waitFor();
  await client.page.waitForFunction(() => Boolean(document.querySelector(".react-flow__viewport")?.style.transform));
  assert.equal(await graph().getByText(/初版发布窗口周一|修订发布窗口周三/).count(), 0);
  assert.equal(await graph().getByText(/已停用校验样本|尚需核对的线索/).count(), 0);
  await graph().locator(`.react-flow__edge[data-id="${confirmed.id}"]`).waitFor({ state: "attached" });
  await capture("01-light-current-projection.png");
  const showPending = client.page.getByRole("checkbox", { name: "显示待确认", exact: true });
  await showPending.check();
  const pendingLine = graph().locator(`.react-flow__edge[data-id="${pending.id}"] .react-flow__edge-path`);
  await pendingLine.waitFor({ state: "attached" });
  assert.ok(await pendingLine.evaluate(element => getComputedStyle(element).strokeDasharray !== "none"));
  await showPending.uncheck();
  await pendingLine.waitFor({ state: "detached" });
  const active2dCount = await graph().locator(".react-flow__node").count();
  assert.equal(active2dCount, 4, "2D projection has one node per currently valid memory");

  const pane = graph().locator(".react-flow__pane");
  const blank = await blankCanvasPoint(pane);
  const beforePan = await flow().getAttribute("style");
  await client.page.mouse.move(blank.x, blank.y);
  await client.page.mouse.down();
  await client.page.mouse.move(blank.x + 55, blank.y + 26, { steps: 6 });
  await client.page.mouse.up();
  await until(async () => (await flow().getAttribute("style")) !== beforePan, "2D pan");
  const beforeWheel = await flow().getAttribute("style");
  await client.page.mouse.wheel(0, -120);
  await until(async () => (await flow().getAttribute("style")) !== beforeWheel, "2D wheel zoom");
  const beforeLayout = await getGraph();
  await button("适合画面").click();
  await flow().evaluate(element => new Promise(resolve => {
    let previous = element.style.transform, unchanged = 0;
    function observe() {
      const next = element.style.transform;
      unchanged = next === previous ? unchanged + 1 : 0;
      previous = next;
      if (unchanged < 4) requestAnimationFrame(observe);
      else resolve();
    }
    requestAnimationFrame(observe);
  }));
  const originalPosition = await node(a.id).evaluate(element => element.style.transform);
  const neighborPosition = await node(b.id).evaluate(element => element.style.transform);
  await button("移动节点").click();
  const star = await node(a.id).locator(".memory-star-core").boundingBox();
  assert.ok(star && star.width >= 39 && star.height >= 39, "star core has a usable target at fit zoom");
  await client.page.mouse.move(star.x + star.width / 2, star.y + star.height / 2);
  await client.page.mouse.down();
  await client.page.mouse.move(star.x + star.width / 2 + 55, star.y + star.height / 2 - 39, { steps: 8 });
  await client.page.mouse.up();
  await until(async () => await node(a.id).evaluate(element => element.style.transform) !== originalPosition, "manual star positioning");
  assert.equal(await node(b.id).evaluate(element => element.style.transform), neighborPosition, "moving one memory does not drag its neighbor");
  assert.deepEqual(await getGraph(), beforeLayout, "moving a star adds no data changes");
  await button("恢复自动布局").click();
  await until(async () => await node(a.id).evaluate(element => element.style.transform) === originalPosition, "reset star offset");
  const resetStar = await node(a.id).locator(".memory-star-core").boundingBox();
  await client.page.mouse.move(resetStar.x + resetStar.width / 2, resetStar.y + resetStar.height / 2);
  await client.page.mouse.down();
  await client.page.mouse.move(resetStar.x + resetStar.width / 2 + 39, resetStar.y + resetStar.height / 2 - 25, {steps:8});
  await client.page.mouse.up();
  await until(async () => await node(a.id).evaluate(element => element.style.transform) !== originalPosition, "layout offset remains on the moving memory");
  const movedAnchor = await node(a.id).evaluate(element => {
    const position = new DOMMatrix(getComputedStyle(element).transform);
    return { x: position.e + element.offsetWidth / 2, y: position.f + element.offsetHeight / 2 };
  });
  await button("移动节点").click();
  await current.click();
  const inspector = client.page.getByRole("complementary", { name: "选中记忆详情" });
  await inspector.getByText("星桥发布 · 定稿发布窗口周五", { exact: true }).waitFor();
  await inspector.getByRole("tab", { name: "演变", exact: true }).click();
  assert.equal(await inspector.getByRole("listitem").count(), 3);
  assert.ok(await inspector.locator("del").count() > 0);
  assert.ok(await inspector.locator("ins").count() > 0);
  await capture("01c-light-version-differences.png");
  await inspector.getByRole("tab", { name: "关系", exact: true }).click();
  await inspector.getByRole("button", { name: "数据核验", exact: true }).click();
  await inspector.getByText("数据核验 · 增加发布窗口复核", { exact: true }).waitFor();
  await inspector.getByRole("button", { name: "上一节点", exact: true }).click();
  await inspector.getByRole("tab", { name: "来源", exact: true }).click();
  await inspector.getByText("来源已移除", { exact: true }).waitFor();
  assert.equal(await inspector.getByRole("link").count(), 0);
  await capture("01b-light-selected-inspector.png");
  await graph().getByRole("button", { name: "返回整体", exact: true }).waitFor();
  await client.page.waitForFunction(() => Math.abs(Number(document.querySelector(".react-flow__viewport")?.style.transform.split("scale(").at(-1)?.split(")")[0])) > 0);
  const focused2d = await flow().getAttribute("style");
  const beforeView = await getGraph();
  await client.page.evaluate(({ id, link }) => {
    const node = document.querySelector(`.react-flow__node[data-id="memory:${id}"]`);
    const box = node.getBoundingClientRect();
    const samples = [];
    window.memorySpaceTransitionProbe = {samples, center:[box.x + box.width/2, box.y + box.height/2]};
    const watch = () => {
      const scene = document.querySelector(".memory-space-workspace");
      if (scene) {
        const item = scene.querySelector(`[data-memory-id="${id}"][data-current=true]`);
        const position = item?.getScreenCTM();
        samples.push({progress:Number(scene.dataset.spaceProgress), projected:position ? [position.e,position.f] : null,
          edge:!!scene.querySelector(`[data-edge-id="${link}"]`), phase:scene.dataset.spacePhase});
      }
      if (samples.length < 90 && scene?.dataset.spacePhase !== "active") requestAnimationFrame(watch);
    };
    requestAnimationFrame(watch);
  }, {id:a.id,link:confirmed.id});
  await button("立体查看").click();
  await space().waitFor();
  await until(async () => (await space().getAttribute("data-space-phase")) === "active", "3D transition finished");
  const trace = await client.page.evaluate(() => window.memorySpaceTransitionProbe);
  assert.ok(trace.samples.some(sample => sample.progress > 0 && sample.progress < 1), "intermediate transition frames exist");
  const first = trace.samples.find(sample => sample.projected && sample.progress <= .06);
  assert.ok(first && first.edge, "starting frame preserves the same logical memory and link IDs");
  assert.ok(Math.hypot(first.projected[0]-trace.center[0],first.projected[1]-trace.center[1]) < 20, "3D starts from the same screen position");
  assert.equal(await graph().locator(".memory-network-flat").evaluate(element => getComputedStyle(element).opacity), "0", "2D content cannot leak into the 3D canvas");
  await button("适合画面").click();
  await capture("02-light-version-depth.png");
  assert.equal(await marker(a.id, 0).count(), 1);
  assert.equal(await marker(a.id, 1).count(), 1);
  assert.equal(await marker(a.id, 2).count(), 1);
  assert.equal(await marker(b.id, 1).count(), 1);
  assert.equal(await marker(a.id, 2).getAttribute("data-current"), "true");
  const ordinary = space().locator(`[data-edge-id="${confirmed.id}"]`);
  assert.equal(await ordinary.getAttribute("data-event-edge"), null, "a correspondence remains distinct from a causal event");
  assert.match(await ordinary.locator("path").getAttribute("d"), /[CQ]/i);
  const evidenceEdge = space().locator(`[data-edge-id="event:${linkedEvent.id}"][data-event-edge]`);
  assert.equal(await evidenceEdge.count(), 1, "reviewed association has a separate visual edge");
  await evidenceEdge.focus();
  await evidenceEdge.press("Enter");
  const eventDetails = client.page.getByRole("dialog", { name: "关联变更依据", exact: true });
  await eventDetails.getByText("用户选定的来源版本", { exact: true }).waitFor();
  await eventDetails.getByText("确认后的目标版本", { exact: true }).waitFor();
  await capture("02c-related-change-evidence.png");
  await client.page.keyboard.press("Escape");
  await eventDetails.waitFor({ state: "hidden" });
  const depths = await Promise.all([0, 1, 2].map(layer => marker(a.id, layer).getAttribute("data-depth")));
  assert.equal(new Set(depths).size, 3);
  for (const layer of [0, 1, 2]) {
    const position = await marker(a.id,layer).evaluate(element => ({x:Number(element.dataset.worldX), y:Number(element.dataset.worldY)}));
    assert.ok(Math.hypot(position.x-movedAnchor.x,position.y-movedAnchor.y) < 1, "every version keeps the manually moved horizontal anchor");
  }
  const start = await camera();
  const canvas = await space().locator("svg.memory-space-canvas").boundingBox();
  assert.ok(canvas);
  const x = canvas.x + canvas.width * .4, y = canvas.y + canvas.height * .48;
  await client.page.mouse.move(x, y);
  await client.page.mouse.down({ button: "middle" });
  await client.page.mouse.move(x + 2, y + 1);
  assert.equal((await camera())[3], "false", "tiny jitter doesn't begin rotation");
  await client.page.mouse.move(x + 80, y + 60, { steps: 8 });
  await until(async () => (await camera())[0] !== start[0] && (await camera())[1] !== start[1], "middle orbit updates both angles");
  await client.page.mouse.up({ button: "middle" });
  await until(async () => (await camera())[3] === "false", "orbit releases");
  const afterOrbit = await camera();
  await client.page.mouse.move(x + 30, y + 10);
  assert.deepEqual(await camera(), afterOrbit);
  const selectedBeforePan = await marker(a.id, 2).getAttribute("transform");
  const blank3d = await blankCanvasPoint(space().locator("svg.memory-space-canvas"), {x: 48, y: 25});
  await client.page.mouse.move(blank3d.x, blank3d.y);
  await client.page.mouse.down();
  await client.page.mouse.move(blank3d.x + 48, blank3d.y + 25, {steps: 8});
  await client.page.mouse.up();
  await until(async () => (await marker(a.id, 2).getAttribute("transform")) !== selectedBeforePan, "3D blank drag pans");
  assert.deepEqual((await camera()).slice(0,3), afterOrbit.slice(0,3), "3D primary pan keeps camera angle and zoom");
  await button("拖动旋转").click();
  await client.page.mouse.move(canvas.x + 34, canvas.y + canvas.height * .65);
  await client.page.mouse.down();
  await client.page.mouse.move(canvas.x + 85, canvas.y + canvas.height * .65 - 24, {steps:5});
  await client.page.mouse.up();
  await until(async () => (await camera())[0] !== afterOrbit[0] && (await camera())[3] === "false", "alternate drag changes view");
  await button("拖动旋转").click();
  const beforeZoom = await camera();
  await client.page.mouse.move(x + 30, y + 10);
  await client.page.mouse.wheel(0, -180);
  await until(async () => (await camera())[2] !== beforeZoom[2], "3D wheel zoom");
  await client.page.mouse.down({ button: "middle" });
  await client.page.mouse.move(x + 37, y + 17);
  await until(async () => (await camera())[3] === "true", "second orbit begins");
  await client.page.evaluate(() => window.dispatchEvent(new Event("blur")));
  await until(async () => (await camera())[3] === "false", "window-blur cleanup");
  const afterBlur = await camera();
  await client.page.mouse.move(x + 45, y + 75);
  await client.page.mouse.up({ button: "middle" });
  assert.deepEqual(await camera(), afterBlur);
  await client.page.mouse.down({ button: "middle" });
  await client.page.mouse.move(x + 51, y + 80);
  await until(async () => (await camera())[3] === "true", "orbit before canvas exit");
  await client.page.mouse.move(canvas.x + canvas.width / 2, canvas.y - 5, { steps: 8 });
  await until(async () => (await camera())[3] === "false", "canvas-exit cleanup");
  await client.page.mouse.up({ button: "middle" });
  await button("适合画面").click();
  const offCanvas = await space().locator("[data-version-id]").evaluateAll(markers => {
    const bounds = markers[0]?.closest("svg")?.getBoundingClientRect();
    return markers.filter(marker => {
      const dot = marker.querySelector(".memory-space-marker-dot")?.getBoundingClientRect();
      return !dot || !bounds || dot.left < bounds.left || dot.top < bounds.top || dot.right > bounds.right || dot.bottom > bounds.bottom;
    }).length;
  });
  assert.equal(offCanvas, 0, "fit keeps all version markers inside the canvas after the orbit");
  await capture("02b-light-orbited-versions.png");
  const projectedLayers = await Promise.all([0, 1, 2].map(layer => marker(a.id, layer).getAttribute("transform")));
  assert.equal(new Set(projectedLayers).size, 3, "three true version positions remain distinct after orbit");

  await marker(a.id, 0).focus();
  await marker(a.id, 0).press("Enter");
  const versionDetails = client.page.getByRole("dialog", { name: "记忆版本详情", exact: true });
  await versionDetails.waitFor();
  await versionDetails.getByText("星桥发布 · 初版发布窗口周一", { exact: true }).waitFor();
  await versionDetails.getByText("来源已移除", { exact: true }).waitFor();
  assert.equal(await versionDetails.getByRole("link", { name: /来源/ }).count(), 0);
  await capture("03-light-historical-version-details.png");
  await client.page.keyboard.press("Escape");
  await versionDetails.waitFor({ state: "hidden" });
  await client.page.mouse.move(x, y);
  await client.page.mouse.down({ button: "middle" });
  await client.page.mouse.move(x + 25, y + 15);
  await until(async () => (await camera())[3] === "true", "orbit begins before mode switch");
  await button("返回二维").evaluate(element => element.click());
  await client.page.mouse.up({ button: "middle" });
  await until(async () => (await space().getAttribute("data-space-phase")) === "exit", "return animation begins");
  await flow().waitFor({ state: "attached" });
  await space().waitFor({ state: "detached" });
  await graph().getByRole("button", { name: "返回整体", exact: true }).waitFor();
  await until(async () => (await flow().getAttribute("style")) === focused2d, "2D camera restores after returning");
  await client.page.evaluate(() => {
    const probe = { entryProgress: null, exitProgress: null };
    window.memoryEarlyReturn = probe;
    function reverse() {
      const scene = document.querySelector(".memory-space-workspace");
      const progress = Number(scene?.dataset.spaceProgress);
      if (scene?.dataset.spacePhase === "exit") {
        probe.exitProgress = progress;
        return;
      }
      if (probe.entryProgress === null && scene?.dataset.spacePhase === "enter" && progress > .15 && progress < .75) {
        probe.entryProgress = progress;
        [...document.querySelectorAll(".memory-network-modes button")].find(button => button.textContent.includes("返回二维")).click();
      }
      requestAnimationFrame(reverse);
    }
    requestAnimationFrame(reverse);
  });
  await button("立体查看").click();
  await client.page.waitForFunction(() => window.memoryEarlyReturn.exitProgress !== null);
  await space().waitFor({state:"detached"});
  const reversed = await client.page.evaluate(() => window.memoryEarlyReturn);
  assert.ok(Math.abs(reversed.entryProgress - reversed.exitProgress) < .20, "reversed fold starts near the interrupted angle");
  await client.page.emulateMedia({reducedMotion:"reduce"});
  assert.equal(await client.page.evaluate(() => matchMedia("(prefers-reduced-motion: reduce)").matches), true);
  await client.page.evaluate(() => {
    const progress = [];
    window.memoryReducedTrace = progress;
    function record() {
      const section = document.querySelector(".memory-space-workspace");
      if (section) progress.push(Number(section.dataset.spaceProgress));
      if (section?.dataset.spacePhase !== "active") requestAnimationFrame(record);
    }
    requestAnimationFrame(record);
  });
  await button("立体查看").click();
  await until(async () => (await space().getAttribute("data-space-phase")) === "active", "reduced-motion 3D activated");
  const reducedProgress = await client.page.evaluate(() => window.memoryReducedTrace);
  assert.ok(reducedProgress.length && !reducedProgress.some(value => value > 0 && value < 1), "reduced-motion camera skips intermediate poses");
  await button("返回二维").click();
  await space().waitFor({state:"detached"});
  await client.page.emulateMedia({reducedMotion:"no-preference"});
  assert.deepEqual(await getGraph(), beforeView, "camera and mode changes never alter memory or relationship history");
  report.transition = { enterSamples: trace.samples.length, flatOffsetPixels: Math.hypot(first.projected[0]-trace.center[0],first.projected[1]-trace.center[1]), reversed, reducedMotionSamples: reducedProgress.length };
  report.checks.push("二维当前投影、画布平移缩放与星核拖动复位", "详情显示当前正文、差异、依据、失效来源及上一节点", "经审阅的关联事件显示独立边与真实依据", "中键旋转、松键、移出画布、合成失焦、主键平移与替代旋转", "开合反向与减少动态效果，后台数据未改动");

  stage = "larger graph, dark theme, and native narrow viewport";
  const bulk = [];
  for (let index = 1; index <= 32; index++) {
    bulk.push(await make(`观测记录 ${String(index).padStart(2, "0")} · ${index === 1 ? "长内容验证横向不会撑出应用可视区域。".repeat(32) : "等待下一次实际更新"}`));
  }
  for (let index = 0; index < bulk.length; index += 4) {
    await api("POST", paths.relation, { source_memory_id: bulk[index].id, target_memory_id: bulk[index + 1].id, label: "同组观测记录" });
    await api("POST", paths.relation, { source_memory_id: bulk[index + 1].id, target_memory_id: bulk[index + 2].id, label: "同组复查记录" });
  }
  assert.equal((await getGraph()).memories.length, 36);
  await appearance("dark");
  await openGraph();
  await graph().getByRole("button", { name: "展开更多", exact: true }).click();
  await graph().getByRole("button", { name: "展开更多", exact: true }).click();
  await button("立体查看").click();
  await space().waitFor();
  await until(async () => (await space().getAttribute("data-space-phase")) === "active", "large graph unfold completed");
  assert.ok(await space().locator("[data-version-id]").count() >= 36);
  report.performance = { wide: await measureOrbit() };
  await button("恢复倾角").click();
  await capture("04-dark-larger-network.png");
  await resizeNative(900, 650);
  assert.equal(await client.page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1), true);
  await button("返回二维").waitFor();
  report.performance.narrow = await measureOrbit();
  await button("恢复倾角").click();
  await button("适合画面").click();
  const layoutStats = await space().evaluate(region => {
    const rectangles = [...region.querySelectorAll(".memory-space-marker-label rect,.memory-space-object-face")]
      .map(label => label.getBoundingClientRect());
    let overlap = 0;
    for (const [index, a] of rectangles.entries()) for (const b of rectangles.slice(index + 1)) {
      if (Math.min(a.right, b.right) - Math.max(a.left, b.left) > 1 && Math.min(a.bottom, b.bottom) - Math.max(a.top, b.top) > 1) overlap += 1;
    }
    return { visibleLabels: rectangles.length, overlap, versions: region.querySelectorAll("[data-version-id]").length };
  });
  assert.ok(layoutStats.visibleLabels > 0 && layoutStats.visibleLabels < layoutStats.versions);
  assert.equal(layoutStats.overlap, 0);
  report.narrowLayout = layoutStats;
  await capture("05-dark-narrow-native-window.png");
  report.checks.push("浅色深色、36 条有效记忆、900 × 650 原生窗体无页面横溢");

  stage = "backup, index rebuild, and desktop deletion replay";
  const registry = JSON.parse(await readFile(join(isolated, "data", "profile-registry.json"), "utf8"));
  const profile = join(isolated, "data", "profiles", registry.profiles[registry.active_profile_id].directory_name, "app.db");
  const snapshot = join(isolated, "before-purge.db");
  await backupDatabase(profile, snapshot);
  await api("DELETE", memoryPath(a.id));
  assert.ok((await stat(profile + ".memory-purge.sqlite")).size > 0);
  const assertPurged = async () => {
    const projected = await getGraph();
    assert.ok(!projected.memories.some(item => item.id === a.id || item.content.includes("星桥发布")));
    assert.equal(projected.histories[a.id], undefined);
    assert.ok(!projected.relations.some(item => item.id === confirmed.id));
    assert.ok(!projected.relation_versions.some(item => item.relation_id === confirmed.id));
    assert.ok(!projected.change_events.some(item => item.memory_id === a.id || item.related_memory_id === a.id));
    assert.deepEqual(layers(projected, b.id), [0, 1]);
    assert.equal((await rawApi("GET", memoryPath(a.id) + "/history")).status, 404);
    assert.ok(!JSON.stringify(await api("GET", `${paths.memory}/retrieve?q=${encodeURIComponent("星桥发布")}`)).includes(a.id));
  };
  await assertPurged();
  const faultsBeforeRestart = client.pageErrors.slice();
  await stopNative();
  await backupDatabase(snapshot, profile);
  client = await startNative(options);
  await assertPurged();

  fixture = await startEmbeddingFixture();
  const config = await api("GET", "/api/settings");
  const configured = await api("PUT", "/api/settings", { ...config, model: { ...config.model, embedding_model: "qa-embedding", embedding_base_url: fixture.url, embedding_api_key_env: "YILIAN_MEMORY_SPACE_EMBEDDING_KEY" } });
  assert.ok(!configured?.error, "isolated embedding fixture settings saved");
  const reindex = await api("POST", paths.memory + "/reindex", { limit: 50 });
  assert.ok(fixture.count() > 0 && reindex.succeeded > 0, "real embedding rebuild exercised against the loopback fixture");
  await assertPurged();
  const indexed = await getGraph();
  assert.equal(indexed.semantic.configured, true);
  assert.ok(indexed.semantic.indexed_memory_ids.includes(b.id));
  assert.ok(indexed.semantic.similarities.length > 0);
  assert.ok(!JSON.stringify(indexed.semantic).includes(a.id), "purged records have no vector graph projections");
  assert.ok(indexed.relations.every(relation => relation.id !== confirmed.id));
  report.reindex = reindex;
  assert.deepEqual([...faultsBeforeRestart, ...client.pageErrors], []);
  report.checks.push("SQLite 恢复旧快照重放删除日志，历史和边均未复活", "嵌入重建与召回使用真实本地 API + 循环测试向量");

  stage = "reject and concurrent correction retry";
  await api("PATCH", relationPath(pending.id), { expected_revision: 1, status: "confirmed" });
  const dRevision = (await govern(d.id)).revision;
  const stageCorrection = content => api("POST", relationPath(pending.id) + "/correction-candidate", {
    target_memory_id: d.id, content, reason: "并发审阅隔离样本", expected_revision: dRevision,
  });
  const rejected = await stageCorrection("观测站点 · 不应新增的草稿");
  await api("POST", `${paths.candidates}/${rejected.candidate_id}/dismiss`, {});
  assert.equal((await govern(d.id)).revision, dRevision);
  const correctionText = "观测站点 · 并发确认后保留的更新";
  const proposal = await stageCorrection(correctionText);
  const body = { content:correctionText, category:"note", scope:"profile" };
  const attempts = await Promise.all([
    rawApi("POST", `${paths.candidates}/${proposal.candidate_id}/accept`, body),
    rawApi("POST", `${paths.candidates}/${proposal.candidate_id}/accept`, body),
  ]);
  assert.deepEqual(attempts.map(result => result.status).sort(), [200,409]);
  assert.equal((await govern(d.id)).revision, dRevision + 1);
  report.checks.push("拒绝候选不生成版本，重复并发接受只提交一次");

  stage = "graph scope and current pointer distinct from latest revision";
  const restrictedChat = await api("POST", "/api/conversations", {title:"隔离档案范围验收"});
  const restricted = await make("受限的对话记忆 · 转入对话范围");
  await api("POST", memoryPath(restricted.id) + "/correct", {
    expected_revision:1, content:restricted.content, scope:`conversation:${restrictedChat.id}`, reason:"验收限定采用范围",
  });
  const unscoped = await getGraph();
  assert.ok(!unscoped.memories.some(item => item.id === restricted.id));
  assert.equal(unscoped.histories[restricted.id], undefined);
  const allowed = await getGraph(`?conversation_id=${restrictedChat.id}`);
  assert.ok(allowed.memories.some(item => item.id === restricted.id));
  assert.equal((await rawApi("GET", paths.graph + "?workspace_id=missing-graph-workspace")).status, 403);
  assert.ok([401,403].includes((await fetch(client.backend + paths.graph)).status), "control-session permissions gate graph endpoint");
  report.checks.push("档案图谱与授权对话范围隔离，未知范围和无控制会话请求遭拒");

  const scheduled = await make("排期检查 · 当前有效周一");
  await edit(scheduled.id, "排期检查 · 未来切换周日");
  await stopNative();
  // The product has no scheduling UI. Set the future validity window in this disposable
  // database to exercise the product's read path against real persisted history.
  const scheduleScript = "import sqlite3,sys,time; db=sqlite3.connect(sys.argv[1]); cur=db.execute('UPDATE memory_governance SET valid_from=? WHERE memory_id=?',(int(time.time()*1000)+86400000,sys.argv[2])); assert cur.rowcount==1; db.commit()";
  const scheduleFixture = launch("python", ["-c",scheduleScript,profile,scheduled.id]);
  assert.equal((await once(scheduleFixture.child,"close"))[0], 0, scheduleFixture.messages.join(""));
  client = await startNative(options);
  const futureSnapshot = await getGraph();
  assert.deepEqual(layers(futureSnapshot,scheduled.id), [0,1]);
  assert.equal(futureSnapshot.current_version_ids[scheduled.id], `${scheduled.id}:1`);
  assert.equal(futureSnapshot.memories.find(item => item.id === scheduled.id).content, "排期检查 · 当前有效周一");
  await openGraph();
  const search = client.page.getByRole("textbox",{name:/搜索记忆/});
  await search.fill("一定不存在的隔离记忆词条");
  await graph().getByText("没有匹配的记忆记录",{exact:true}).waitFor();
  await button("清除搜索与筛选").click();
  await search.fill("排期检查");
  await graph().getByRole("button",{name:/排期检查.*当前有效周一/}).waitFor();
  assert.equal(await graph().getByText(/未来切换周日/).count(),0);
  await until(() => node(scheduled.id).evaluate(element => {
    const card = element.getBoundingClientRect();
    const pane = element.closest(".react-flow").getBoundingClientRect();
    return card.left >= pane.left && card.right <= pane.right && card.top >= pane.top && card.bottom <= pane.bottom;
  }), "filtered current-version card is in the visible canvas");
  await capture("06-current-head-before-scheduled-revision.png");
  await button("立体查看").click();
  await until(() => space().getAttribute("data-space-phase").then(value => value === "active"), "scheduled revisions unfold");
  const effectiveMarker = space().locator(`[data-version-id="${scheduled.id}:1"]`);
  const futureMarker = space().locator(`[data-version-id="${scheduled.id}:2"]`);
  assert.equal(await effectiveMarker.getAttribute("data-current"), "true");
  assert.equal(await effectiveMarker.getAttribute("data-layer"), "0");
  assert.equal(await futureMarker.getAttribute("data-current"), "false");
  assert.equal(await futureMarker.getAttribute("data-layer"), "1");
  await capture("06a-effective-base-and-newer-version.png");
  await button("返回二维").click();
  await space().waitFor({state:"detached"});
  report.checks.push("真正空库和筛选无结果分别提供可操作入口", "隔离 SQLite 测试夹具设定的未来修订不取代二维、三维当前生效版本");

  if (process.env.YILIAN_SPACE_ROLLBACK_BINARY) {
    stage = "previous native binary → new → previous → new, isolated migration and rollback";
    const prior = resolve(process.env.YILIAN_SPACE_ROLLBACK_BINARY);
    assert.notEqual(prior, binary, "previous and updated binaries must be different files");
    const rollbackRoot = await mkdtemp(join(tmpdir(), "yilian-memory-space-rollback-"));
    await stopNative();
    client = await startNative(options, rollbackRoot, prior);
    const old = await make("旧版写入 · 原始内容");
    const neighbor = await make("旧版邻居 · 保留");
    await edit(old.id, "旧版更新 · 正文已核对");
    assert.deepEqual((await api("GET", memoryPath(old.id) + "/history")).map(version => version.revision), [2, 1]);
    await stopNative();
    client = await startNative(options, rollbackRoot);
    const upgraded = await getGraph();
    assert.deepEqual(layers(upgraded, old.id), [0, 1]);
    assert.equal(upgraded.histories[old.id].find(version => version.layer === 0).relationship_known, false);
    const upgradeLink = await api("POST", paths.relation, { source_memory_id: old.id, target_memory_id: neighbor.id, label: "迁移重开后的对应" });
    await stopNative();
    client = await startNative(options, rollbackRoot, prior);
    assert.equal((await api("GET", memoryPath(old.id))).content, "旧版更新 · 正文已核对");
    await edit(old.id, "回滚运行写入 · 修改仍保留");
    assert.deepEqual((await api("GET", memoryPath(old.id) + "/history")).map(version => version.revision), [3, 2, 1]);
    await stopNative();
    client = await startNative(options, rollbackRoot);
    const upgradedAgain = await getGraph();
    assert.deepEqual(layers(upgradedAgain, old.id), [0, 1, 2]);
    assert.equal(upgradedAgain.memories.find(item => item.id === old.id).content, "回滚运行写入 · 修改仍保留");
    assert.ok(upgradedAgain.relations.some(item => item.id === upgradeLink.id));
    assert.deepEqual(client.pageErrors, []);
    report.checks.push("真实旧二进制迁移、回滚后读取与写入、重新升级恢复");
    report.rollback = "previous-native → current-native → previous-native → current-native";
  }

}

try {
  const options = await startFrontend();
  client = await startNative(options);
  report.app = await client.page.title();
  report.userAgent = await client.page.evaluate(() => navigator.userAgent);
  if (process.env.YILIAN_SPACE_VISUAL === "orbit") {
    stage = "native memory orbit reference";
    fixture = await startEmbeddingFixture();
    const { verifyMemoryOrbit } = await import("./native-memory-orbit.mjs");
    await verifyMemoryOrbit({
      page: client.page, api, report, fixture, capture, appearance, openGraph, resizeNative, until,
      setStage: next => { stage = next; },
    });
    assert.deepEqual(client.pageErrors, []);
  } else {
    await verifyMemorySpace(options);
  }
  await writeFile(join(evidence, "result.json"), JSON.stringify(report, null, 2));
  console.log(JSON.stringify(report, null, 2));
} catch (error) {
  console.error(`[${stage}] ${error.stack ?? error}`);
  await client?.page.screenshot({ path: join(evidence, "failure.png") }).catch(() => {});
  const diagnostic = await client?.page.evaluate(() => Object.fromEntries(
    [".memory-network", ".memory-network-flat", ".react-flow", ".react-flow__viewport", ".react-flow__nodes", ".react-flow__node"].map(selector => [
      selector, [...document.querySelectorAll(selector)].slice(0, 8).map(node => {
        const { x, y, width, height } = node.getBoundingClientRect();
        const style = getComputedStyle(node);
        return { x, y, width, height, visibility: style.visibility, opacity: style.opacity, display: style.display, transform: style.transform, inline: node.getAttribute("style") };
      }),
    ]),
  )).catch(() => null);
  if (diagnostic) await writeFile(join(evidence, "failure-state.json"), JSON.stringify({ stage, diagnostic, pageErrors: client.pageErrors }, null, 2));
  process.exitCode = 1;
} finally {
  await stopNative().catch(() => {});
  if (fixture) await new Promise(done => fixture.server.close(done));
  for (const child of children.reverse()) await stop(child).catch(() => {});
}
