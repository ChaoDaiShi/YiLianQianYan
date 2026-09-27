import { spawn, execFileSync } from "node:child_process";
import { mkdtemp, mkdir, rm, writeFile } from "node:fs/promises";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { chromium } from "playwright-core";
import { uiFoundation, collectGeometry } from "./ui-foundation.mjs";
import { canvasStudio, studioGeometry } from "./canvas-studio.mjs";
import { canvasStability } from "./canvas-stability.mjs";
import { chatViewRace } from "./chat-view-race.mjs";
import { workbenchU2 } from "./workbench-u2.mjs";

const repositoryRoot = resolve(import.meta.dirname, "..", "..");
const frontendRoot = resolve(process.env.YILIAN_E2E_FRONTEND_ROOT ?? resolve(import.meta.dirname, ".."));
if (process.env.YILIAN_E2E_FRONTEND_ROOT && process.env.YILIAN_E2E_WORKBENCH !== "before") {
  throw new Error("An alternate frontend root is only allowed for baseline capture");
}

function configuredPort(name, fallback) {
  const port = Number(process.env[name] ?? fallback);
  if (!Number.isInteger(port) || port < 1 || port > 65_535) {
    throw new Error(`${name} must be a valid TCP port`);
  }
  return port;
}

const backendPort = configuredPort("YILIAN_E2E_BACKEND_PORT", 9420);
const frontendPort = configuredPort("YILIAN_E2E_FRONTEND_PORT", 1420);
const controlToken = "v1-e2e-control-session-token-".padEnd(64, "x");
const edgePaths = [
  "C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe",
  "C:\\Program Files\\Microsoft\\Edge\\Application\\msedge.exe",
  "C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe",
  "C:\\Program Files (x86)\\Google\\Chrome\\Application\\chrome.exe",
];

const processes = [];
const logs = new Map();
const chatFocused = process.env.YILIAN_E2E_CHAT_FOCUSED;
const studioStage = process.env.YILIAN_E2E_STUDIO;
const workbenchStage = process.env.YILIAN_E2E_WORKBENCH;
const uiStage = process.env.YILIAN_E2E_UI_MATRIX;
const evidenceDir = join(repositoryRoot, "target", chatFocused ? "chat-reliability-r1" : workbenchStage ? "workbench-u2" : studioStage ? "canvas-studio" : uiStage === "baseline" ? "ui-baseline" : uiStage ? "ui-foundation" : "canvas-e2e", new Date().toISOString().replace(/[:.]/g, "-"));
const browserLog = [];
const networkLog = [];
let browser;
let zoomWorker;
let page;
let isolatedRoot;
let testExitCode = 1;

function capture(child, label) {
  for (const [name, stream] of [["stdout", child.stdout], ["stderr", child.stderr]]) {
    const output = [];
    logs.set(`${label}-${name}`, output);
    stream?.on("data", (chunk) => {
      output.push(chunk.toString());
    });
  }
  processes.push(child);
  return child;
}

async function portIsFree(port) {
  return new Promise((resolvePort, reject) => {
    const server = createServer();
    server.once("error", reject);
    server.listen(port, "127.0.0.1", () => server.close(() => resolvePort()));
  });
}

async function waitFor(url, timeoutMs = 30_000) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      const response = await fetch(url);
      if (response.ok) return;
    } catch {
      // The isolated service is still starting.
    }
    await new Promise((resolveWait) => setTimeout(resolveWait, 200));
  }
  throw new Error(`Timed out waiting for ${url}`);
}

async function stop(child) {
  if (!child || child.exitCode !== null) return;
  child.kill("SIGTERM");
  await Promise.race([
    new Promise((resolveExit) => child.once("exit", resolveExit)),
    new Promise((resolveWait) => setTimeout(resolveWait, 3_000)),
  ]);
  if (child.exitCode === null) child.kill("SIGKILL");
}

try {
  await mkdir(evidenceDir, { recursive: true });
  await portIsFree(backendPort);
  await portIsFree(frontendPort);
  isolatedRoot = await mkdtemp(join(tmpdir(), "yilian-v1-e2e-"));
  const dataDir = join(isolatedRoot, "data");
  const workspace = join(isolatedRoot, "workspace");
  await mkdir(dataDir);
  await mkdir(workspace);

  const backendBinary = join(repositoryRoot, "target", "debug", "yilian-server.exe");
  capture(spawn(backendBinary, [], {
    cwd: repositoryRoot,
    env: {
      ...process.env,
      YILIAN_DATA_DIR: dataDir,
      YILIAN_WORKSPACE: workspace,
      YILIAN_HOST: `127.0.0.1:${backendPort}`,
      YILIAN_CONTROL_SESSION_TOKEN: controlToken,
    },
    stdio: ["ignore", "pipe", "pipe"],
    windowsHide: true,
  }), "backend");

  const viteEntry = join(frontendRoot, "node_modules", "vite", "bin", "vite.js");
  capture(spawn(process.execPath, [viteEntry, "--host", "127.0.0.1", "--port", String(frontendPort), "--strictPort"], {
    cwd: frontendRoot,
    env: {
      ...process.env,
      VITE_API_BASE: `http://127.0.0.1:${backendPort}`,
      VITE_CONTROL_SESSION_TOKEN: controlToken,
    },
    stdio: ["ignore", "pipe", "pipe"],
    windowsHide: true,
  }), "frontend");

  await Promise.all([
    waitFor(`http://127.0.0.1:${backendPort}/api/health`),
    waitFor(`http://127.0.0.1:${frontendPort}/tasks`),
  ]);

  const { existsSync } = await import("node:fs");
  const executablePath = edgePaths.find((path) => existsSync(path));
  if (!executablePath) throw new Error("No supported system Chromium browser was found");
  if (uiStage && uiStage !== "baseline") {
    const extension = join(isolatedRoot, "zoom-extension");
    await mkdir(extension);
    await writeFile(join(extension, "manifest.json"), JSON.stringify({manifest_version:3,name:"UI zoom evidence",version:"1.0",permissions:["tabs"],background:{service_worker:"background.js"}}));
    await writeFile(join(extension, "background.js"), "chrome.runtime.onInstalled.addListener(() => {});");
    browser = await chromium.launchPersistentContext(join(isolatedRoot,"edge-profile"), {executablePath,headless:true,viewport:{width:1440,height:900},args:[`--disable-extensions-except=${extension}`,`--load-extension=${extension}`]});
    zoomWorker = browser.serviceWorkers()[0] ?? await browser.waitForEvent("serviceworker", {timeout:10000});
  } else {
    browser = await chromium.launch({ executablePath, headless: true });
  }
  page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
  page.on("response", (response) => {
    const url = new URL(response.url());
    if (url.pathname.startsWith("/api/task-world/")) networkLog.push({ path: url.pathname, method: response.request().method(), status: response.status() });
  });
  const browserErrors = [];
  page.on("pageerror", (error) => { browserErrors.push(error.message); browserLog.push({ type: "pageerror", message: error.message }); });
  page.on("console", (message) => {
    browserLog.push({ type: message.type(), message: message.text() });
    if (message.type() === "error") browserErrors.push(message.text());
  });

  await page.goto(`http://127.0.0.1:${frontendPort}/tasks`, { waitUntil: "networkidle" });
  await page.getByRole("heading", { name: "任务中心" }).waitFor();
  const finishSetup = page.getByRole("button", { name: "确认并保存" });
  if (await finishSetup.isVisible()) {
    await finishSetup.click();
    await finishSetup.waitFor({ state: "hidden" });
  }
  const laterSetup = page.getByRole("button", { name: "稍后设置" });
  if (await laterSetup.isVisible()) {
    await laterSetup.click();
    await laterSetup.waitFor({ state: "hidden" });
  }
  if (chatFocused) {
    const result=await chatViewRace({page,frontendPort,evidenceDir});
    if(browserErrors.length)throw new Error(browserErrors.join(" | "));
    await writeFile(join(evidenceDir,"result.json"),JSON.stringify(result,null,2));
    testExitCode=0;
  } else if (workbenchStage) {
    const result=await workbenchU2({page,frontendPort,backendPort,controlToken,evidenceDir,stage:workbenchStage});
    const expectedProviderErrors=browserErrors.filter(message=>message==='Failed to load resource: the server responded with a status of 400 (Bad Request)');
    if(expectedProviderErrors.length!==(result.flows?.expectedProviderErrors??0))throw new Error('Unexpected provider rejection diagnostics');
    const unexpected=browserErrors.filter(message=>!expectedProviderErrors.includes(message));
    if(unexpected.length) throw new Error(`Workbench browser errors: ${unexpected.join(' | ')}`);
    await writeFile(join(evidenceDir,'result.json'),JSON.stringify({...result,browser:executablePath,browser_version:browser.version()},null,2));
    process.stdout.write(JSON.stringify(result)+'\n');
    testExitCode=0;
  } else if (studioStage) {
    const result=await canvasStudio({page,backendPort,frontendPort,controlToken,evidenceDir,stage:studioStage});
    if(browserErrors.length) throw new Error(`Studio browser errors: ${browserErrors.join(" | ")}`);
    await writeFile(join(evidenceDir,"result.json"),JSON.stringify({...result,browser:executablePath,browser_version:browser.version()},null,2));
    process.stdout.write(JSON.stringify(result)+"\n");
    testExitCode=0;
  } else if (uiStage) {
    const result = await uiFoundation({ page, backendPort, frontendPort, controlToken, evidenceDir, stage: uiStage, zoomWorker });
    if (browserErrors.length) throw new Error(`UI browser errors: ${browserErrors.join(" | ")}`);
    await writeFile(join(evidenceDir, "result.json"), JSON.stringify({status:"passed",browser:executablePath,real_backend:true, ...result}, null, 2));
    process.stdout.write(JSON.stringify(result) + "\n");
    testExitCode = 0;
  } else {
  const checkedViewports = [];
  if (!process.env.YILIAN_E2E_CANVAS_ONLY) {
  await page.getByRole("button", { name: "新建任务画布" }).click();
  await page.waitForURL(/\/task-world\/[^/]+$/);
  const firstGraphUrl = page.url();
  await page.getByRole("button", { name: "添加任务节点" }).click();
  await page.locator(".task-world-node").waitFor();
  await page.getByRole("button", { name: "自动布局" }).click();
  await page.getByRole("button", { name: "任务中心" }).click();
  await page.getByText("1 个图", { exact: true }).waitFor();

  await page.getByRole("button", { name: "新建任务画布" }).click();
  await page.waitForURL(/\/task-world\/[^/]+$/);
  if (page.url() === firstGraphUrl) throw new Error("Second graph reused the first graph identity");
  await page.getByRole("button", { name: "任务中心" }).click();
  await page.getByText("2 个图", { exact: true }).waitFor();

  await page.goto(`http://127.0.0.1:${frontendPort}/capabilities`, { waitUntil: "networkidle" });
  await page.locator('.u2-managed-imports > summary').click();
  await page.getByRole("heading", { name: "托管 Skill 与声明式 Plugin" }).waitFor();
  if (await page.getByText(/托管导入服务 unavailable/).count()) {
    throw new Error("Managed import protected route was unavailable");
  }
  await page.goto(`http://127.0.0.1:${frontendPort}/system`, { waitUntil: "networkidle" });
  await page.getByRole("heading", { name: "系统监控" }).waitFor();

  await page.goto(`http://127.0.0.1:${frontendPort}/chat`, { waitUntil: "networkidle" });
  await page.getByRole("button", { name: "与小涟语音对话" }).click();
  await page.locator('[data-testid="voice-pill"]').waitFor();
  for (const viewport of [
    { width: 1280, height: 720 },
    { width: 1366, height: 768 },
    { width: 1920, height: 1080 },
  ]) {
    await page.setViewportSize(viewport);
    await page.goto(`http://127.0.0.1:${frontendPort}/settings?section=model`, { waitUntil: "networkidle" });
    await page.getByRole("heading", { name: "设置", exact: true }).waitFor();
    const settingsContent = page.locator(".system-settings-content");
    const saveButton = page.getByRole("button", { name: "保存设置" });
    await saveButton.scrollIntoViewIfNeeded();
    if (!await saveButton.isVisible()) throw new Error(`Settings save is hidden at ${viewport.width}x${viewport.height}`);
    const horizontalOverflow = await settingsContent.evaluate((element) => element.scrollWidth > element.clientWidth + 1);
    if (horizontalOverflow) throw new Error(`Settings overflow horizontally at ${viewport.width}x${viewport.height}`);
    const [saveBox, pillBox] = await Promise.all([
      saveButton.boundingBox(),
      page.locator('[data-testid="voice-pill"]').boundingBox(),
    ]);
    if (!saveBox || !pillBox) throw new Error(`Settings or voice pill has no layout box at ${viewport.width}x${viewport.height}`);
    const overlaps = !(
      saveBox.x + saveBox.width <= pillBox.x ||
      pillBox.x + pillBox.width <= saveBox.x ||
      saveBox.y + saveBox.height <= pillBox.y ||
      pillBox.y + pillBox.height <= saveBox.y
    );
    if (overlaps) throw new Error(`Voice pill covers Settings save at ${viewport.width}x${viewport.height}`);
    checkedViewports.push(`${viewport.width}x${viewport.height}`);
  }
  }

  const canvasResult = await canvasStability({ page, backendPort, frontendPort, controlToken, evidenceDir });
  // The scenario deliberately provokes exactly one real stale revision. Match
  // that observed response to Chromium's network diagnostic, not to page errors.
  const expectedConflicts = browserErrors.filter((message) => message === "Failed to load resource: the server responded with a status of 409 (Conflict)");
  if (expectedConflicts.length !== canvasResult.stale_responses) throw new Error("Unexpected conflict diagnostics");
  const unexpectedErrors = browserErrors.filter((message) => !expectedConflicts.includes(message));
  if (unexpectedErrors.length) {
    throw new Error(`Browser errors: ${unexpectedErrors.join(" | ")}`);
  }
  const result = {
    status: "passed",
    graphs_created: process.env.YILIAN_E2E_CANVAS_ONLY ? 1 : 3,
    settings_viewports: checkedViewports,
    real_backend: true,
    browser: executablePath,
    canvas_stability: canvasResult,
    evidence: evidenceDir,
  };
  await writeFile(join(evidenceDir, "result.json"), JSON.stringify(result, null, 2));
  process.stdout.write(JSON.stringify(result) + "\n");
  testExitCode = 0;
  }
} catch (error) {
  if (page && studioStage) await writeFile(join(evidenceDir,"failure-studio.json"),JSON.stringify(await studioGeometry(page).catch(()=>({})),null,2));
  if (page) await writeFile(join(evidenceDir, "failure-geometry.json"), JSON.stringify(await collectGeometry(page).catch(() => ({})), null, 2));
  await page?.screenshot({ path: join(evidenceDir, "failure.png"), fullPage: true }).catch(() => {});
  await writeFile(join(evidenceDir, "failure.txt"), error.stack || String(error));
  for (const [label, output] of logs) {
    process.stderr.write(`\n[${label} tail]\n${output.join("")}\n`);
  }
  throw error;
} finally {
  for (const [label, output] of logs) await writeFile(join(evidenceDir, `${label}.log`), output.join(""));
  await writeFile(join(evidenceDir, "browser-console.json"), JSON.stringify(browserLog, null, 2));
  await writeFile(join(evidenceDir, "task-network.json"), JSON.stringify(networkLog, null, 2));
  await writeFile(join(evidenceDir, "exit-code.txt"), String(testExitCode));
  if (process.platform === "win32") {
    try {
      const diagnostic = execFileSync("powershell.exe", ["-NoProfile", "-Command", `[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false); $ports = Get-NetTCPConnection -State Listen -ErrorAction SilentlyContinue | Where-Object { $_.LocalPort -in @(9420,1420,${backendPort},${frontendPort}) } | Select-Object LocalAddress,LocalPort,OwningProcess; $processes = Get-CimInstance Win32_Process | Select-Object ProcessId,ParentProcessId,Name,ExecutablePath; @{ ports = @($ports); processes = @($processes) } | ConvertTo-Json -Depth 4`], { windowsHide: true, encoding: "utf8", timeout: 15_000 });
      await writeFile(join(evidenceDir, "ports-and-processes.json"), diagnostic);
    } catch (error) { await writeFile(join(evidenceDir, "diagnostic-error.txt"), String(error)); }
  }
  process.stderr.write(`E2E evidence: ${evidenceDir}\n`);
  await browser?.close().catch(() => {});
  for (const child of processes.reverse()) await stop(child);
  if (isolatedRoot) await rm(isolatedRoot, { recursive: true, force: true });
}
