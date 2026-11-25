import assert from "node:assert/strict";
import { existsSync } from "node:fs";
import { mkdir, writeFile } from "node:fs/promises";
import { chromium } from "playwright-core";

// Existing local backend and model readiness are real. Only the onboarding
// preference and the explicit unavailable/error cases below are controlled.
const frontend = process.env.YILIAN_SETUP_FRONTEND ?? "http://127.0.0.1:1420";
const output = process.env.YILIAN_SETUP_EVIDENCE ?? "C:/Users/25113/.codex/visualizations/2026/10/29/01a1881b-f385-7b01-a3a7-270f9de6e401/shared-goal-followup";
const executablePath = ["C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe", "C:/Program Files/Microsoft/Edge/Application/msedge.exe"].find(existsSync);
const browser = await chromium.launch({ executablePath, headless: true });
const steps = [];
try {
  const page = await browser.newPage({ viewport: { width: 1366, height: 768 } });
  await page.route("**/api/system/preferences", async route => {
    if (route.request().method() !== "GET") return route.continue();
    const response = await route.fetch();
    const preferences = await response.json();
    await route.fulfill({ response, json: { ...preferences, setup_completed: false } });
  });
  await page.goto(`${frontend}/chat`);
  await page.waitForFunction(() => document.querySelector('[data-chat-view="home"]') || document.querySelector(".quick-setup-page"));
  assert.equal(new URL(page.url()).pathname, "/chat", "An existing ready model must not force first-run setup");
  assert.equal(await page.locator('[aria-label="模型配置提醒"]').count(), 0);
  steps.push("Actual ready model bypasses incomplete onboarding without an unconfigured reminder");

  await page.addInitScript(() => window.sessionStorage.setItem("yilian.setup.deferred", "true"));
  await page.route("**/api/chat/readiness", route => route.fulfill({ json: { configured: false, available: false, provider: "", model: "" } }));
  await page.goto(`${frontend}/chat`);
  await page.locator('[aria-label="模型配置提醒"]').waitFor();
  assert.match(await page.locator('[aria-label="模型配置提醒"]').innerText(), /尚未配置可用的聊天模型/);
  steps.push("Controlled unavailable model shows an accurate setup reminder");

  await page.unroute("**/api/chat/readiness");
  await page.route("**/api/chat/readiness", route => route.fulfill({ status: 503, json: { error: "controlled readiness outage" } }));
  await page.goto(`${frontend}/chat`);
  const unknown = page.locator('[aria-label="模型状态提醒"]');
  await unknown.waitFor();
  assert.match(await unknown.innerText(), /暂时无法确认聊天模型状态/);
  assert.equal(await page.locator('[aria-label="模型配置提醒"]').count(), 0);
  steps.push("Controlled readiness failure remains unknown instead of unconfigured");

  await page.unroute("**/api/chat/readiness");
  await page.route("**/api/settings", async route => {
    const response = await route.fetch();
    const settings = await response.json();
    await route.fulfill({ response, json: { ...settings, provider_readiness: { ...settings.provider_readiness, model: { configured: false, available: false, provider: "", model: "" } } } });
  });
  await page.goto(`${frontend}/setup?model=1`);
  await page.getByRole("button", { name: "继续使用当前模型" }).waitFor();
  assert.equal(new URL(page.url()).pathname, "/setup");
  steps.push("Explicit setup recognizes real chat readiness even when controlled global readiness is unavailable");
  await mkdir(output, { recursive: true });
  await writeFile(`${output}/setup-readiness.json`, JSON.stringify({ passed: true, frontend, model_readiness: "real local backend", controlled: ["setup_completed=false in GET response only", "unavailable readiness", "503 readiness", "global settings readiness unavailable while chat readiness remains real"], steps }, null, 2));
  console.log(JSON.stringify({ passed: true, steps }));
} finally { await browser.close(); }
