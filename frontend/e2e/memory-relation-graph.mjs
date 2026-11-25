import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { existsSync } from "node:fs";
import { mkdir, mkdtemp } from "node:fs/promises";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { chromium } from "playwright-core";

const root = resolve(import.meta.dirname, "../..");
const isolated = await mkdtemp(join(tmpdir(), "yilian-memory-graph-"));
const evidence = resolve(process.env.YILIAN_MEMORY_GRAPH_EVIDENCE || join(root, "target", "memory-relation-graph-e2e"));
await mkdir(evidence, { recursive: true });
const token = "memory-graph-fixture-control-session-000000000000";
const children = [], logs = [];
let browser;

async function freePort() {
  const server = createServer();
  await new Promise(done => server.listen(0, "127.0.0.1", done));
  const port = server.address().port;
  await new Promise(done => server.close(done));
  return port;
}

function launch(command, args, env, cwd = root) {
  const child = spawn(command, args, { cwd, env: { ...process.env, ...env }, windowsHide: true, stdio: ["ignore", "pipe", "pipe"] });
  for (const stream of [child.stdout, child.stderr]) stream.on("data", chunk => logs.push(String(chunk)));
  children.push(child);
}

async function ready(url) {
  const limit = Date.now() + 30_000;
  while (Date.now() < limit) {
    try { if ((await fetch(url)).ok) return; } catch { /* startup polling */ }
    await new Promise(done => setTimeout(done, 120));
  }
  throw new Error(`${url} did not become ready`);
}

const backendPort = await freePort(), frontendPort = await freePort();
const base = `http://127.0.0.1:${backendPort}`, front = `http://127.0.0.1:${frontendPort}`;
const headers = { "Content-Type": "application/json", "X-Yilian-Control-Session": token };

async function api(method, path, body) {
  const response = await fetch(base + path, { method, headers, signal: AbortSignal.timeout(30_000), body: body === undefined ? undefined : JSON.stringify(body) });
  const raw = await response.text();
  assert.ok(response.ok, `${method} ${path} ${response.status}: ${raw}`);
  return raw ? JSON.parse(raw) : null;
}

try {
  launch(process.env.YILIAN_E2E_SERVER || join(root, "target/debug/yilian-server.exe"), [], {
    YILIAN_HOST: new URL(base).host, YILIAN_WORKSPACE: isolated, YILIAN_DATA_DIR: join(isolated, "state"),
    YILIAN_ALLOWED_ORIGINS: front, YILIAN_CONTROL_SESSION_TOKEN: token,
  });
  launch(process.execPath, [join(root, "frontend/node_modules/vite/bin/vite.js"), "--host", "127.0.0.1", "--port", String(frontendPort), "--strictPort"],
    { VITE_API_BASE: base, VITE_CONTROL_SESSION_TOKEN: token }, join(root, "frontend"));
  await Promise.all([ready(`${base}/api/health`), ready(front)]);
  const executablePath = ["C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe", "C:/Program Files/Microsoft/Edge/Application/msedge.exe"].find(existsSync);
  assert.ok(executablePath);
  browser = await chromium.launch({ executablePath, headless: true });
  const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
  const errors = [];
  page.on("pageerror", error => errors.push(error.message));
  await page.addInitScript(() => sessionStorage.setItem("yilian.setup.deferred", "true"));
  await page.goto(`${front}/memory`);
  await page.getByRole("button", { name: "添加记忆", exact: true }).click();
  const add = page.getByRole("dialog", { name: "添加记忆", exact: true });
  await add.getByLabel("分类", { exact: true }).selectOption("relationship");
  await add.getByLabel("关系类型").selectOption("kinship");
  await add.getByLabel("起点人物").fill("小林");
  await add.getByLabel("关系", { exact: true }).fill("姐妹");
  await add.getByLabel("关联人物").fill("小周");
  await add.getByLabel("发生日期（可选）").fill("2026-11-13");
  await add.getByLabel("关系依据").selectOption("inferred");
  await add.getByRole("button", { name: "添加", exact: true }).click();
  await add.waitFor({ state: "hidden" });
  const persisted = (await api("GET", "/api/memories/library")).find(item => item.content.includes("小林"));
  assert.ok(persisted.content.endsWith("待确认"));
  assert.deepEqual(JSON.parse(persisted.metadata).relationship, {
    kind: "kinship", subject: "小林", label: "姐妹", object: "小周", directed: false, certainty: "inferred", occurredAt: "2026-11-13",
  });
  const graph = page.getByRole("region", { name: "记忆关系图" });
  await graph.getByRole("checkbox", { name: "显示待确认" }).check();
  await graph.getByRole("button", { name: "人物 小林", exact: true }).click();
  await graph.getByText("直接关系列表", { exact: true }).click();
  await graph.getByRole("button", { name: /小林.*姐妹.*小周.*亲属关系.*待确认/ }).waitFor();
  assert.equal(await graph.locator(".react-flow__edge-path[marker-end]").count(), 0);
  await page.screenshot({ path: join(evidence, "01-kinship-evidence.png"), animations: "disabled" });
  await graph.getByRole("button", { name: /2026-11-13.*查看关系依据/ }).click();
  const detail = page.getByRole("dialog", { name: "记忆详情与来源" });
  await detail.locator(".memory-detail-body").getByText(persisted.content, { exact: true }).waitFor();
  await detail.getByRole("button", { name: "永久清除记忆" }).click();
  const confirm = page.getByRole("dialog", { name: "永久清除记忆" });
  await confirm.getByRole("button", { name: "清除记忆", exact: true }).click();
  await page.getByText("这里暂时还没有记忆记录", { exact: true }).waitFor();
  assert.equal((await api("GET", "/api/memories/library")).length, 0);
  assert.equal((await fetch(`${base}/api/memories/${persisted.id}`, { headers })).status, 404);
  assert.ok((await api("GET", "/api/memories/activity?recent=false")).entries.some(entry => entry.kind === "memory_deleted" && entry.content == null));
  assert.equal((await api("GET", `/api/memories/retrieve?q=${encodeURIComponent("小林")}`)).count, 0);

  const content = "水文资料：岷江年径流量需要核对公开站点原文。";
  const upload = await fetch(`${base}/api/resources/ingest?${new URLSearchParams({ name: "岷江水文.txt" })}`, {
    method: "POST", headers: { ...headers, "Content-Type": "text/plain" }, body: content,
  });
  assert.ok(upload.ok);
  const { document } = await api("POST", "/api/knowledge/documents", { resource_id: (await upload.json()).id });
  const original = await api("GET", `/api/knowledge/documents/${document.id}?include_chunks=false`);
  await api("POST", "/api/knowledge/entries", {
    topic: "水文", title: "岷江径流结论", summary: "资料中提到需要核对站点原文。",
    sources: [{ document_id: document.id, version: original.version, start_char: 0, end_char: [...content].length }],
  });
  await page.getByRole("tab", { name: "知识引用", exact: true }).click();
  await page.getByRole("button", { name: "引用关系图", exact: true }).click();
  const knowledge = page.getByRole("region", { name: "知识引用关系图" });
  await knowledge.getByRole("button", { name: "知识条目 岷江径流结论", exact: true }).click();
  await knowledge.getByRole("button", { name: "资料原文 岷江水文.txt", exact: true }).waitFor();
  await knowledge.getByText("直接关系列表", { exact: true }).click();
  await knowledge.getByRole("button", { name: /查看关系依据/ }).click();
  const citation = page.getByRole("dialog", { name: "引用依据", exact: true });
  await citation.getByText(content, { exact: true }).waitFor();
  assert.ok((await citation.getByRole("link", { name: "定位原文" }).getAttribute("href")).includes(`document=${document.id}`));
  await page.keyboard.press("Escape");
  await citation.waitFor({ state: "hidden" });
  await api("DELETE", `/api/knowledge/documents/${document.id}`);
  await page.getByRole("button", { name: "重新核对来源", exact: true }).click();
  await knowledge.getByText("来源已移除", { exact: true }).waitFor();
  assert.equal(await knowledge.getByRole("link", { name: "打开原文", exact: true }).count(), 0);
  await page.screenshot({ path: join(evidence, "02-removed-document-anchor.png"), animations: "disabled" });
  assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1));
  assert.deepEqual(errors, []);
  console.log(JSON.stringify({ savedKinship: true, inferred: true, memoryPurged: true, exactKnowledgeAnchor: true, removedSourceUnlinked: true, evidence }));
} catch (error) {
  console.error(error, logs.slice(-6).join("").slice(-3000));
  process.exitCode = 1;
} finally {
  await browser?.close();
  for (const child of children.reverse()) child.kill();
}
