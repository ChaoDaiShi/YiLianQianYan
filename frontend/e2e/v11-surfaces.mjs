import assert from "node:assert/strict";
import { existsSync } from "node:fs";
import { mkdir, writeFile } from "node:fs/promises";
import { join, resolve } from "node:path";
import { chromium } from "playwright-core";

const frontendPort = Number(process.env.FRONTEND_PORT || 1420);
const evidenceDir = resolve(process.env.EVIDENCE_DIR || "../target/v1.1-rc1/browser");
const edgePaths = [
  "C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe",
  "C:\\Program Files\\Microsoft\\Edge\\Application\\msedge.exe",
];
const executablePath = edgePaths.find(existsSync);
if (!executablePath) throw new Error("System Edge is required");
await mkdir(evidenceDir, { recursive: true });
const browser = await chromium.launch({ executablePath, headless: true });
const page = await browser.newPage({ viewport: { width: 1280, height: 720 } });
const results = [];

try {
  await page.goto(`http://127.0.0.1:${frontendPort}/setup`, { waitUntil: "networkidle" });
  await page.getByText("v1.1 首次配置", { exact: true }).waitFor();
  await page.screenshot({ path: join(evidenceDir, "setup-1280x720.png"), fullPage: true });
  await page.getByRole("button", { name: "稍后完成", exact: true }).click();
  await page.waitForURL(/\/chat/);

  for (const [width, height] of [[1280, 720], [1366, 768], [1920, 1080], [2560, 1440]]) {
    await page.setViewportSize({ width, height });
    for (const route of ["account", "search"]) {
      await page.goto(`http://127.0.0.1:${frontendPort}/${route}`, { waitUntil: "networkidle" });
      await page.locator(".page-header").waitFor();
      const geometry = await page.evaluate(() => ({
        bodyWidth: document.documentElement.clientWidth,
        bodyScrollWidth: document.documentElement.scrollWidth,
        mainWidth: document.querySelector("main")?.getBoundingClientRect().width ?? 0,
        smallestInputFont: Math.min(...Array.from(document.querySelectorAll("input,select,textarea")).map((element) => Number.parseFloat(getComputedStyle(element).fontSize)).filter(Number.isFinite), 99),
      }));
      assert.ok(geometry.bodyScrollWidth <= geometry.bodyWidth, `${route} overflows at ${width}`);
      assert.ok(geometry.mainWidth > 0, `${route} main content is missing`);
      if (geometry.smallestInputFont !== 99) assert.ok(geometry.smallestInputFont >= 14, `${route} input type is too small`);
      await page.screenshot({ path: join(evidenceDir, `${route}-${width}x${height}.png`), fullPage: true });
      results.push({ route, width, height, ...geometry });
    }
  }

  await page.goto(`http://127.0.0.1:${frontendPort}/account`, { waitUntil: "networkidle" });
  await page.getByText("仅本地", { exact: true }).waitFor();
  assert.equal(await page.getByRole("button", { name: "登录并连接", exact: true }).count(), 1);
  await page.goto(`http://127.0.0.1:${frontendPort}/search`, { waitUntil: "networkidle" });
  await page.getByRole("textbox", { name: "搜索问题" }).waitFor();
  assert.equal(await page.getByRole("button", { name: "搜索", exact: true }).count(), 1);

  const result = { status: "passed", real_backend: true, browser: executablePath, browser_version: browser.version(), checks: results };
  await writeFile(join(evidenceDir, "result.json"), JSON.stringify(result, null, 2));
  console.log(JSON.stringify({ status: result.status, screenshots: results.length + 1, browser: result.browser_version }));
} finally {
  await browser.close();
}
