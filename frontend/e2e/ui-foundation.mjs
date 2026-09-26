import assert from "node:assert/strict";
import { writeFile } from "node:fs/promises";
import { join } from "node:path";

// A real, isolated backend supplies all page data. No mocked success or counters.
export async function uiFoundation({ page, backendPort, frontendPort, controlToken, evidenceDir, stage }) {
  const origin = `http://127.0.0.1:${frontendPort}`;
  const api = async (path, body) => {
    const response = await fetch(`http://127.0.0.1:${backendPort}${path}`, {
      method: "POST", headers: { "Content-Type": "application/json", "X-Yilian-Control-Session": controlToken }, body: JSON.stringify(body),
    });
    assert.ok(response.ok, `Fixture ${path}: ${response.status}`);
    return response.json();
  };
  await api("/api/task-world/graphs", { id: "ui-foundation", nodes: ["A", "B", "C"].map((id) => ({ id, kind: "work", title: `检查桌面布局 ${id}`, input: { instruction: "核对字号、布局和真实状态，保留清楚可读的说明。" }, retry_policy: { max_attempts: 2 } })), edges: [] });
  await page.goto(`${origin}/chat`, { waitUntil: "domcontentloaded" });
  await page.getByRole("button", { name: "与小涟语音对话" }).click();
  await page.locator('[data-testid="voice-pill"]').waitFor();
  await page.locator('.global-voice-expand').click();
  const conversation = await api("/api/conversations", { title: "UI 可读性验收" });
  const stream = await fetch(`http://127.0.0.1:${backendPort}/api/chat`, {
    method: "POST", headers: { "Content-Type": "application/json", "X-Yilian-Control-Session": controlToken },
    body: JSON.stringify({conversation_id:conversation.id,message:"请检查桌面文字与布局是否清楚可读。这是本地 UI 验收输入。"}),
  });
  assert.ok(stream.ok);
  await api("/api/chat/stop", {conversation_id:conversation.id});
  await stream.body.cancel();
  const chatPath = `/chat/${conversation.id}`;
  const routes = [
    ["chat", chatPath, ".conversation-message-user"], ["tasks", "/tasks", ".workspace-region h2"],
    ["canvas", "/task-world/ui-foundation", ".task-world-node"], ["settings", "/settings?section=model", ".system-settings-content"],
    ["capabilities", "/capabilities", ".capability-page"], ["system", "/system", ".workspace-region h2"],
  ];
  const sizes = [[1280,720], [1366,768], [1920,1080], [2560,1440]];
  const records = [];
  for (const [width, height] of sizes) {
    await page.setViewportSize({ width, height });
    for (const [name, path, ready] of routes) {
      await page.goto(`${origin}${path}`, { waitUntil: "domcontentloaded" });
      await page.locator(ready).first().waitFor();
      if (name === "settings") await page.getByRole("button", { name: "保存设置", exact: true }).scrollIntoViewIfNeeded();
      await page.evaluate(() => document.fonts.ready);
      await page.screenshot({ path: join(evidenceDir, `${name}-${width}x${height}.png`) });
      const geometry = await collectGeometry(page);
      records.push({ name, width, height, zoom: 1, ...geometry });
      await writeFile(join(evidenceDir, "geometry.json"), JSON.stringify(records, null, 2));
    }
  }
  return { stage, screenshots: records.length, records: "geometry.json", windows_dpi: "HUMAN_PENDING" };
}

export async function collectGeometry(page) {
  return page.evaluate(() => {
    const box = (el) => {
      if (!el) return null;
      const r = el.getBoundingClientRect(), s = getComputedStyle(el);
      return { x:r.x, y:r.y, width:r.width, height:r.height, font:parseFloat(s.fontSize), scrollWidth:el.scrollWidth, clientWidth:el.clientWidth };
    };
    const selectors = {
      body:"body", nav:".nav-rail", navLabel:".nav-rail-item-label", header:".page-header, .workspace-region h2, .glass-header", description:".page-description",
      voice:'[data-testid="voice-pill"]', composer:".composer-card", message:".conversation-message-user", reading:".message-column",
      canvas:".task-world-canvas", node:".task-world-node", nodeTitle:".task-world-node-title", nodeSummary:".task-world-node-summary", nodeMeta:".task-world-node-execution",
      trail:".task-world-trail-item button", inspector:".task-world-inspector", toolbar:'[aria-label="画布视图工具"]', form:".system-settings-content", input:'input:not([type="checkbox"]):not([type="hidden"])',
    };
    const lowFonts = [...document.querySelectorAll("body *")].filter(el => {
      if (el.closest('svg,[aria-hidden="true"]')) return false;
      if (![...el.childNodes].some(n => n.nodeType === Node.TEXT_NODE && n.textContent.trim())) return false;
      const s = getComputedStyle(el), r = el.getBoundingClientRect();
      return r.width && r.height && s.visibility !== "hidden" && s.display !== "none" && parseFloat(s.fontSize) < 12;
    }).map(el => ({ tag:el.tagName, class:el.className, text:el.textContent.slice(0,70), font:parseFloat(getComputedStyle(el).fontSize) }));
    const save = [...document.querySelectorAll("button")].find(el=>el.textContent.trim()==="保存设置");
    return { innerWidth, innerHeight, dpr:devicePixelRatio, document:{scrollWidth:document.documentElement.scrollWidth,clientWidth:document.documentElement.clientWidth}, elements:{...Object.fromEntries(Object.entries(selectors).map(([key,selector])=>[key,box(document.querySelector(selector))])),save:box(save)},lowFonts };
  });
}
