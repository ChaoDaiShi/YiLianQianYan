import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { existsSync } from "node:fs";
import { readFile, writeFile, mkdir } from "node:fs/promises";
import { createServer } from "node:net";
import { join, resolve } from "node:path";
import { chromium } from "playwright-core";
import { pathToFileURL } from "node:url";

// Browser plugin not available: use the project's installed Playwright and system Edge.
// The preserved isolated DB and file APIs are real. Long-text/error/approval responses
// are explicitly synthetic layout fixtures, not lifecycle or approval evidence.
const root = resolve(import.meta.dirname, "../..");
const threadRoot = "C:/Users/25113/.codex/visualizations/2026/10/29/01a1881b-f385-7b01-a3a7-270f9de6e401";
const previous = JSON.parse(await readFile(join(threadRoot, "shared-goal-state/result.json"), "utf8"));
const evidence = join(threadRoot, "shared-goal-followup", process.env.YILIAN_LAYOUT_BASELINE === "1" ? "layout-before" : "layout-after");
await mkdir(evidence, { recursive: true });
const backendPort = await port(), frontendPort = await port();
const backendBase = `http://127.0.0.1:${backendPort}`, frontendBase = `http://127.0.0.1:${frontendPort}`;
const token = "shared-layout-controlled-session-".padEnd(64, "x");
const children = [], serviceLogs = [], consoleLogs = [], geometry = [];
const longTitle = "当前目标：检查共享任务在窄窗口和长内容中的阅读与操作体验。".repeat(12);
const longError = `受控布局错误：等待用户确认目标输出目录 ${"F:/project/very-long-unbroken-segment/".repeat(12)}，请检查输入后继续。`;
let browser, page;
function start(file, args, env, label) { const child = spawn(file, args, { cwd: label === "vite" ? join(root, "frontend") : root, env: { ...process.env, ...env }, windowsHide: true, stdio: ["ignore", "pipe", "pipe"] }); for (const stream of [child.stdout, child.stderr]) stream.on("data", chunk => serviceLogs.push(`[${label}] ${chunk}`)); children.push(child); return child; }
const graphId = previous.graph_id, conversationId = previous.conversation_id, workspaceId = previous.workspace_id;
let stress = false, failRead = false, history = false;
const goalPath = `/api/task-world/proofloop/graphs/${graphId}/goal`;
const graphPath = `/api/task-world/graphs/${graphId}/detail`;

try {
  start(process.env.YILIAN_E2E_BACKEND_BINARY ?? previous.backendBinary.path, [], { YILIAN_DATA_DIR: join(previous.isolatedRoot, "data"), YILIAN_WORKSPACE: join(previous.isolatedRoot, "workspace"), YILIAN_HOST: `127.0.0.1:${backendPort}`, YILIAN_ALLOWED_ORIGINS: frontendBase, YILIAN_CONTROL_SESSION_TOKEN: token }, "backend");
  start(process.execPath, ["--input-type=module", "-e", `import {createServer} from ${JSON.stringify(pathToFileURL(join(root, "frontend/node_modules/vite/dist/node/index.js")).href)}; const server=await createServer({root:${JSON.stringify(join(root, "frontend"))},server:{host:'127.0.0.1',port:${frontendPort},strictPort:true,watch:{ignored:['**/*']}}}); await server.listen();`], { VITE_API_BASE: backendBase, VITE_CONTROL_SESSION_TOKEN: token }, "vite");
  await Promise.all([ready(`${backendBase}/api/health`), ready(frontendBase)]);
  const executablePath = ["C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe", "C:/Program Files/Microsoft/Edge/Application/msedge.exe"].find(existsSync);
  browser = await chromium.launch({ executablePath, headless: true });
  page = await browser.newPage();
  page.on("pageerror", error => consoleLogs.push({ type: "pageerror", text: error.message }));
  page.on("console", message => { if (["error", "warning"].includes(message.type())) consoleLogs.push({ type: message.type(), text: message.text() }); });
  await page.addInitScript(() => sessionStorage.setItem("yilian.setup.deferred", "true"));
  await page.route("**/api/**", async route => {
    const path = new URL(route.request().url()).pathname;
    if (route.request().method() !== "GET") return route.continue();
    if (history && path === `/api/conversations/${conversationId}`) {
      const response = await route.fetch(), json = await response.json();
      json.messages = [{ id: "layout-user", role: "user", content: "查看当前任务状态与成果。", created_at: Date.now() - 1000 }, { id: "layout-assistant", role: "assistant", content: "目标状态与成果可在上方查看，后续对话继续使用同一目标。", created_at: Date.now() }];
      return route.fulfill({ response, json });
    }
    if (![goalPath, graphPath].includes(path)) return route.continue();
    if (failRead && path === goalPath) return route.fulfill({ status: 503, contentType: "application/json", json: { error: longError, code: "controlled_layout_error" } });
    if (!stress) return route.continue();
    const response = await route.fetch(), json = await response.json();
    if (path === goalPath) { json.contract.user_goal = longTitle; json.contract.deliverables[0].title = `超长成果文件名-${"共享状态验收".repeat(35)}.md`; }
    else { json.detail.nodes[0].status = "running"; json.detail.nodes[0].title = "需要审批的长标题步骤".repeat(20); json.detail.nodes[0].latest_execution.status = "waiting_approval"; json.detail.nodes[0].latest_execution.error = longError; }
    return route.fulfill({ response, json });
  });

  for (const width of process.env.YILIAN_LAYOUT_BASELINE === "1" ? [390] : [1366, 960, 720, 390]) {
    const height = width === 390 ? 844 : 768;
    await page.setViewportSize({ width, height });
    await visit(`/chat/${conversationId}`); await summary().waitFor();
    const input = page.getByRole("textbox", { name: "聊天输入", exact: true });
    await input.fill("输入一段可完整阅读和发送的中文内容，按钮不应挤压文字。");
    const inputBox = await input.boundingBox();
    assert.ok(inputBox.width >= Math.min(200, width - 150), `chat text area squeezed at ${width}: ${inputBox.width}`);
    for (const name of ["发送消息", "导入资源", "切换工作模式", "模型与思考设置"]) {
      const box = await page.getByRole("button", { name, exact: true }).boundingBox();
      assert.ok(box && box.width >= 26 && box.height >= 26 && box.x >= 0 && box.x + box.width <= width + 1 && box.y >= 0 && box.y + box.height <= height, `unusable ${name} at ${width}: ${JSON.stringify(box)}`);
    }
    await measure(`chat-${width}`, width);
    await page.screenshot({ path: join(evidence, `chat-${width}.png`), fullPage: true, animations: "disabled" });
    await summary().getByRole("button", { name: "任务详情与成果", exact: true }).focus();
    await page.keyboard.press("Enter");
    const dialog = page.getByRole("dialog", { name: "任务详情与成果", exact: true }); await dialog.waitFor();
    await dialog.getByRole("button", { name: "查看成果", exact: true }).click();
    await dialog.getByRole("region", { name: "目标交付物" }).getByRole("button", { name: "下载真实文件", exact: true }).waitFor({ state: "visible" });
    await page.screenshot({ path: join(evidence, `details-${width}.png`), fullPage: true, animations: "disabled" });
    await page.keyboard.press("Escape"); await dialog.waitFor({ state: "hidden" });
    assert.equal(await summary().getByRole("button", { name: "任务详情与成果", exact: true }).evaluate(element => element === document.activeElement), true, `dialog focus not restored ${width}`);
    const permissions = summary().getByRole("button", { name: "权限与提醒", exact: true });
    await permissions.focus(); await page.keyboard.press("Enter");
    const policy = page.getByRole("dialog", { name: "权限与提醒", exact: true });
    await policy.getByRole("checkbox", { name: "本目标开启状态提醒", exact: true }).waitFor({ state: "visible" });
    await policy.getByText(/当前审批模式：/).waitFor();
    await page.screenshot({ path: join(evidence, `policy-${width}.png`), fullPage: true, animations: "disabled" });
    await page.keyboard.press("Escape"); await policy.waitFor({ state: "hidden" });
    assert.equal(await permissions.evaluate(element => element === document.activeElement), true, `policy dialog focus not restored ${width}`);
    await visit(`/workspaces/${workspaceId}?tab=tasks&graph=${graphId}`); await summary().waitFor();
    assert.equal(await page.getByTestId("goal-overview").count(), 0, "workspace must open details on demand instead of stacking the full overview below its summary");
    await measure(`workspace-${width}`, width);
    await page.screenshot({ path: join(evidence, `workspace-${width}.png`), fullPage: true, animations: "disabled" });
    await visit(`/task-world/${graphId}`); await summary().waitFor();
    await page.getByRole("button", { name: "专家视图", exact: true }).click();
    await page.getByTestId("task-world-canvas").waitFor({ state: "visible" });
    const run = await page.getByRole("button", { name: "运行选中节点", exact: true }).boundingBox();
    assert.ok(run && run.x >= 0 && run.x + run.width <= width + 1 && run.y + run.height <= height, `canvas execution action clipped ${width}: ${JSON.stringify(run)}`);
    await measure(`canvas-${width}`, width);
    await page.screenshot({ path: join(evidence, `canvas-${width}.png`), fullPage: true, animations: "disabled" });
  }
  history = true;
  for (const width of [720, 390]) {
    const height = width === 390 ? 844 : 768;
    await page.setViewportSize({ width, height });
    await visit(`/chat/${conversationId}`); await summary().waitFor();
    const input = page.getByRole("textbox", { name: "聊天输入", exact: true });
    await input.fill("保持输入框完整可读，长段落在输入区域内滚动。\n".repeat(18));
    const box = await input.boundingBox(), send = await page.getByRole("button", { name: "发送消息", exact: true }).boundingBox();
    assert.ok(box.width >= Math.min(200, width - 150) && box.height <= 128 && send.y + send.height <= height, `history input/buttons clipped ${width}`);
    await measure(`history-chat-${width}`, width);
    await page.screenshot({ path: join(evidence, `history-chat-${width}.png`), fullPage: true, animations: "disabled" });
  }
  history = false;
  stress = true;
  for (const width of [1366, 720, 390]) {
    await page.setViewportSize({ width, height: width === 390 ? 844 : 768 });
    await visit(`/chat/${conversationId}`); await summary().waitFor();
    assert.equal(await summary().getAttribute("data-phase"), "waiting_approval");
    await measure(`long-approval-${width}`, width);
    await page.screenshot({ path: join(evidence, `long-approval-${width}.png`), fullPage: true, animations: "disabled" });
    await summary().getByRole("button", { name: "任务详情与成果", exact: true }).click();
    const dialog = page.getByRole("dialog", { name: "任务详情与成果", exact: true });
    await dialog.waitFor();
    const fullTitle = dialog.locator(".goal-view-full-title");
    await fullTitle.locator("summary").focus(); await page.keyboard.press("Enter");
    assert.equal(await fullTitle.getAttribute("open"), "");
    assert.equal(await fullTitle.locator(":scope > p").innerText(), longTitle, "full long goal must remain readable after keyboard expansion");
    const scrollers = await dialog.evaluate(element => [element, ...element.querySelectorAll("*")].filter(node => /auto|scroll/.test(getComputedStyle(node).overflowY) && node.scrollHeight > node.clientHeight + 2).length);
    assert.equal(scrollers, 1, "details must use one vertical scroll owner");
    await fullTitle.locator("summary").focus(); await page.keyboard.press("Enter");
    await page.screenshot({ path: join(evidence, `long-details-${width}.png`), fullPage: true, animations: "disabled" });
    await page.keyboard.press("Escape");
  }
  failRead = true;
  await page.setViewportSize({ width: 390, height: 844 });
  await visit(`/chat/${conversationId}`);
  await page.getByRole("alert").filter({ hasText: "状态同步失败" }).waitFor();
  await measure("long-error-390", 390);
  await page.getByRole("button", { name: "重新读取", exact: true }).focus();
  await page.screenshot({ path: join(evidence, "long-error-390.png"), fullPage: true, animations: "disabled" });
  failRead = false;
  await page.keyboard.press("Enter"); await summary().waitFor();
  const errors = consoleLogs.filter(item => item.type === "pageerror");
  assert.deepEqual(errors, []);
  await writeFile(join(evidence, "result.json"), JSON.stringify({ status: "passed", browser: "system Edge via installed Playwright; Browser plugin unavailable", real_data: "preserved isolated SQLite and artifact APIs", layout_fixtures: "synthetic long title, approval state and 503 error only", geometry, consoleLogs }, null, 2));
  console.log(JSON.stringify({ status: "passed", evidence, geometry }));
} catch (error) {
  if (page) { await page.screenshot({ path: join(evidence, "failure.png"), fullPage: true, animations: "disabled" }); await writeFile(join(evidence, "failure-page.txt"), `${page.url()}\n${await page.locator("body").innerText()}`); }
  await writeFile(join(evidence, "failure.txt"), error.stack ?? String(error));
  throw error;
} finally {
  await browser?.close();
  for (const child of children.reverse()) { if (child.exitCode === null) child.kill(); }
  await writeFile(join(evidence, "services.log"), serviceLogs.join(""));
}

function summary() { return page.locator(`.goal-tracking-summary[data-graph-id="${graphId}"]`).first(); }
async function visit(path) { await page.goto(`${frontendBase}${path}`, { waitUntil: "domcontentloaded" }); }
async function measure(name, width) {
  const data = await page.evaluate(() => ({ width: document.documentElement.clientWidth, scrollWidth: document.documentElement.scrollWidth, height: document.documentElement.clientHeight, scrollHeight: document.documentElement.scrollHeight }));
  assert.ok(data.scrollWidth <= data.width + 2 && data.scrollHeight <= data.height + 2, `${name} page overflow: ${JSON.stringify(data)}`);
  geometry.push({ name, viewport: width, ...data });
}
async function port() { const server = createServer(); await new Promise(resolveListen => server.listen(0, "127.0.0.1", resolveListen)); const next = server.address().port; await new Promise(resolveClose => server.close(resolveClose)); return next; }
async function ready(url) { const end = Date.now() + 30000; while (Date.now() < end) { try { if ((await fetch(url)).ok) return; } catch {} await new Promise(resolveWait => setTimeout(resolveWait, 100)); } throw new Error(`unavailable ${url}`); }

