import { spawn } from "node:child_process";
import { mkdtemp, mkdir, rm } from "node:fs/promises";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { chromium } from "playwright-core";

const repositoryRoot = resolve(import.meta.dirname, "..", "..");
const frontendRoot = resolve(import.meta.dirname, "..");
const backendPort = 9420;
const frontendPort = 1420;
const controlToken = "v1-e2e-control-session-token-".padEnd(64, "x");
const edgePaths = [
  "C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe",
  "C:\\Program Files\\Microsoft\\Edge\\Application\\msedge.exe",
  "C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe",
  "C:\\Program Files (x86)\\Google\\Chrome\\Application\\chrome.exe",
];

const processes = [];
const logs = new Map();
let browser;
let isolatedRoot;

function capture(child, label) {
  const output = [];
  logs.set(label, output);
  for (const stream of [child.stdout, child.stderr]) {
    stream?.on("data", (chunk) => {
      output.push(chunk.toString());
      if (output.length > 80) output.shift();
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
  browser = await chromium.launch({ executablePath, headless: true });
  const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
  const browserErrors = [];
  page.on("pageerror", (error) => browserErrors.push(error.message));
  page.on("console", (message) => {
    if (message.type() === "error") browserErrors.push(message.text());
  });

  await page.goto(`http://127.0.0.1:${frontendPort}/tasks`, { waitUntil: "networkidle" });
  await page.getByRole("heading", { name: "任务中心" }).waitFor();
  const finishSetup = page.getByRole("button", { name: "确认并保存" });
  if (await finishSetup.isVisible()) {
    await finishSetup.click();
    await finishSetup.waitFor({ state: "hidden" });
  }
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
  await page.getByRole("heading", { name: "托管 Skill 与声明式 Plugin" }).waitFor();
  if (await page.getByText(/托管导入服务 unavailable/).count()) {
    throw new Error("Managed import protected route was unavailable");
  }
  await page.goto(`http://127.0.0.1:${frontendPort}/system`, { waitUntil: "networkidle" });
  await page.getByRole("heading", { name: "系统监控" }).waitFor();

  if (browserErrors.length) {
    throw new Error(`Browser errors: ${browserErrors.join(" | ")}`);
  }
  process.stdout.write(JSON.stringify({
    status: "passed",
    graphs_created: 2,
    real_backend: true,
    browser: executablePath,
  }) + "\n");
} catch (error) {
  for (const [label, output] of logs) {
    process.stderr.write(`\n[${label} tail]\n${output.join("")}\n`);
  }
  throw error;
} finally {
  await browser?.close().catch(() => {});
  for (const child of processes.reverse()) await stop(child);
  if (isolatedRoot) await rm(isolatedRoot, { recursive: true, force: true });
}
