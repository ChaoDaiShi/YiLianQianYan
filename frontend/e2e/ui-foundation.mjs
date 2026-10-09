import assert from "node:assert/strict";
import { writeFile } from "node:fs/promises";
import { join } from "node:path";

// A real, isolated backend supplies all page data. No mocked success or counters.
export async function uiFoundation({ page, backendPort, frontendPort, controlToken, evidenceDir, stage, zoomWorker }) {
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
  const final = stage === "final";
  const focus = process.env.YILIAN_E2E_UI_PAGES?.split(",");
  if (final) {
    sizes.splice(2, 0, [1600,900]);
    routes.push(["memory", "/memory", ".memory-page-body"], ["knowledge", "/knowledge", ".knowledge-page-body"], ["workflow", "/workflows", ".workspace-region h2"]);
  }
  const records = [];
  const violations = [];
  const capture = async ([name, path, ready], width, height, zoom = 1) => {
    await page.goto(`${origin}${path}`, { waitUntil: "domcontentloaded" });
    await page.locator(ready).first().waitFor();
    if (name === "settings") await page.getByRole("button", { name: "保存设置", exact: true }).scrollIntoViewIfNeeded();
    await page.evaluate(async () => {
      await document.fonts.ready;
      await Promise.all(document.getAnimations().filter(a => Number.isFinite(a.effect?.getComputedTiming().endTime)).map(a => a.finished.catch(() => {})));
    });
    if (name === "canvas" && zoom === 2) await page.getByLabel("画布布局与镜头").scrollIntoViewIfNeeded();
    const geometry = await collectGeometry(page);
    const record = { name, width, height, zoom, ...geometry };
    records.push(record);
    await page.screenshot({ path: join(evidenceDir, `${name}-${width}x${height}-zoom${zoom}.png`) });
    await writeFile(join(evidenceDir, "geometry.json"), JSON.stringify(records, null, 2));
    if (final) violations.push(...checkGeometry(record));
    if (final && name === "settings") {
      await page.getByRole("navigation", {name:"设置分类"}).getByRole("button",{name:"智能体",exact:true}).click();
      await page.getByLabel("名称",{exact:true}).fill("UI isolated save evidence");
      const save=page.getByRole("button",{name:"保存设置",exact:true});
      await save.scrollIntoViewIfNeeded();
      await save.click({trial:true});
      const enabled = await collectGeometry(page);
      await writeFile(join(evidenceDir,`enabled-save-${width}x${height}-zoom${zoom}.json`),JSON.stringify(enabled,null,2));
      assert.ok(enabled.elements.save.hit,"Enabled save must receive pointer input");
      const a=enabled.elements.save,b=enabled.elements.voice;
      assert.ok(a.y+a.height<=b.y || a.x+a.width<=b.x || b.x+b.width<=a.x,"Voice must not cover enabled save");
    }
  };
  for (const [width, height] of sizes) {
    await page.setViewportSize({ width, height });
    for (const route of routes.filter(([name]) => !focus || focus.includes(name))) await capture(route, width, height);
  }
  if (final && !focus) {
    const core = routes.filter(([name]) => ["chat", "canvas", "settings"].includes(name));
    await page.setViewportSize({width:3440,height:1440});
    for (const route of core) await capture(route,3440,1440);
    for (const width of [959,960,1179,1180]) {
      await page.setViewportSize({width,height:900});
      await capture(routes[0],width,900);
      assert.equal(await page.locator(".workbench-grid").getAttribute("data-mode"),width<960?"narrow":width<1180?"compact":"full");
    }
    // Actual browser tab zoom. No CSS zoom, pinch scaling, or DPI emulation.
    // https://developer.chrome.com/docs/extensions/reference/api/tabs#method-setZoom
    const base = await page.evaluate(() => ({width:innerWidth,height:innerHeight}));
    await writeFile(join(evidenceDir,"zoom-method.json"),JSON.stringify({method:"chrome.tabs.setZoom/getZoom",base,windowsDpi:"HUMAN_PENDING"},null,2));
    for (const zoom of [1.25,1.5,2]) {
      const actual = await zoomWorker.evaluate(async ({origin,zoom}) => {
        const tab = (await chrome.tabs.query({})).find(t => t.url?.startsWith(origin));
        await chrome.tabs.setZoom(tab.id,zoom);
        return chrome.tabs.getZoom(tab.id);
      }, {origin,zoom});
      assert.ok(Math.abs(actual-zoom)<0.001, "Browser must confirm requested zoom");
      for (const route of core) await capture(route,base.width,base.height,zoom);
    }
    const narrow = records.find(r => r.name === "canvas" && r.width === 1366 && r.zoom === 1);
    const wide = records.find(r => r.name === "canvas" && r.width === 2560 && r.zoom === 1);
    if (wide.elements.canvas.width < narrow.elements.canvas.width + 800) violations.push("Canvas did not gain meaningful working space");
    await zoomWorker.evaluate(async ({origin}) => { const tab=(await chrome.tabs.query({})).find(t=>t.url?.startsWith(origin)); await chrome.tabs.setZoom(tab.id,1); }, {origin});
    await page.setViewportSize({width:1280,height:720});
    await page.goto(`${origin}/settings?section=agent`,{waitUntil:"domcontentloaded"});
    await page.getByLabel("名称",{exact:true}).fill("UI isolated save evidence");
    const save = page.getByRole("button",{name:"保存设置",exact:true});
    await save.scrollIntoViewIfNeeded();
    await save.click({trial:true});
    const saved = page.waitForResponse(r=>r.url().endsWith("/api/settings") && r.request().method()==="PUT");
    await save.click();
    assert.ok((await saved).ok(),"Real isolated settings save");
    await page.getByText("设置已保存",{exact:true}).waitFor();
    await page.screenshot({path:join(evidenceDir,"settings-save-verified.png")});
    await writeFile(join(evidenceDir,"violations.json"),JSON.stringify(violations,null,2));
    assert.deepEqual(violations,[],"Responsive geometry contract");
  }
  await writeFile(join(evidenceDir,"violations.json"),JSON.stringify(violations,null,2));
  assert.deepEqual(violations,[],"Responsive geometry contract");
  return { stage, focus, screenshots: records.length, records: "geometry.json", windows_dpi: "HUMAN_PENDING", real_settings_save: final && !focus };
}

export async function collectGeometry(page) {
  return page.evaluate(() => {
    const box = (el) => {
      if (!el) return null;
      const r = el.getBoundingClientRect(), s = getComputedStyle(el);
      const hit = document.elementFromPoint(r.x+r.width/2,r.y+r.height/2);
      return { x:r.x, y:r.y, width:r.width, height:r.height, font:parseFloat(s.fontSize), scrollWidth:el.scrollWidth, clientWidth:el.clientWidth, offsetWidth:el.offsetWidth,offsetHeight:el.offsetHeight,disabled:!!el.disabled,hit:!!hit && (el===hit||el.contains(hit)) };
    };
    const selectors = {
      body:"body", nav:".nav-rail", navLabel:".nav-rail-item-label", header:".page-header, .workspace-region h2, .glass-header", description:".page-description",
      voice:'[data-testid="voice-pill"]', composer:".composer-card", message:".conversation-message-user", reading:".message-column",
      canvas:".task-world-canvas", node:".task-world-node", nodeTitle:".task-world-node-title", nodeSummary:".task-world-node-summary", nodeMeta:".task-world-node-execution",
      trail:".task-world-trail-item button", inspector:".task-world-inspector", toolbar:'[aria-label="画布视图工具"]', cameraToolbar:'[aria-label="画布布局与镜头"]', controls:".react-flow__controls", form:".settings-form", input:'input:not([type="checkbox"]):not([type="hidden"])',
    };
    const lowFonts = [...document.querySelectorAll("body *")].filter(el => {
      if (el.closest('svg,[aria-hidden="true"]')) return false;
      if (![...el.childNodes].some(n => n.nodeType === Node.TEXT_NODE && n.textContent.trim())) return false;
      const s = getComputedStyle(el), r = el.getBoundingClientRect();
      return r.width && r.height && s.visibility !== "hidden" && s.display !== "none" && parseFloat(s.fontSize) < 12;
    }).map(el => ({ tag:el.tagName, class:el.className, text:el.textContent.slice(0,70), font:parseFloat(getComputedStyle(el).fontSize) }));
    const save = [...document.querySelectorAll("button")].find(el=>el.textContent.trim()==="保存设置");
    const overflows = [...document.querySelectorAll('.workspace-region,.page-canvas,.system-settings-content,.task-center-body,.capability-page-body,.memory-page-body,.knowledge-page-body,.workbench-grid')].filter(el=>el.scrollWidth>el.clientWidth+1).map(el=>({class:el.className,scrollWidth:el.scrollWidth,clientWidth:el.clientWidth}));
    const targets = [...document.querySelectorAll('.page-header button,[aria-label="必需安全入口"] button,[aria-label="画布布局与镜头"] button,[aria-label="画布视图工具"] button')].map(el=>({text:el.textContent,disabled:el.disabled,...box(el)}));
    const grids = [...document.querySelectorAll('.system-monitor-grid,.monitor-toolbox-grid,.capability-card-grid,.capability-row-stack,.knowledge-capability-grid')].map(el=>({class:el.className,columns:getComputedStyle(el).gridTemplateColumns}));
    return { innerWidth, innerHeight, dpr:devicePixelRatio, document:{scrollWidth:document.documentElement.scrollWidth,clientWidth:document.documentElement.clientWidth}, elements:{...Object.fromEntries(Object.entries(selectors).map(([key,selector])=>[key,box(document.querySelector(selector))])),save:box(save)},lowFonts,overflows,targets,grids };
  });
}

function checkGeometry(r) {
  const errors = [], e = r.elements;
  const require = (condition,message) => {if (!condition) errors.push(`${r.name} ${r.width}x${r.height} zoom ${r.zoom}: ${message}`);};
  const visible = b => b && b.width>0 && b.height>0 && b.x>=-1 && b.y>=-1 && b.x+b.width<=r.innerWidth+1 && b.y+b.height<=r.innerHeight+1;
  const overlaps = (a,b) => a && b && a.x < b.x+b.width && b.x < a.x+a.width && a.y < b.y+b.height && b.y < a.y+a.height;
  require(r.document.scrollWidth<=r.document.clientWidth,"document horizontal overflow");
  require(r.overflows.length===0,`scroll region overflow ${JSON.stringify(r.overflows)}`);
  require(visible(e.nav),"navigation outside viewport");
  require(visible(e.header),"header outside viewport");
  require(visible(e.voice),"voice pill outside viewport");
  for (const key of ["composer","save","toolbar","cameraToolbar","controls"]) if(e[key]) require(!overlaps(e.voice,e[key]),`voice covers ${key}`);
  for(const [key,min] of Object.entries({body:16,navLabel:12,description:14,input:14,message:16,nodeTitle:15,nodeSummary:13,nodeMeta:12,trail:13})) if(e[key]) require(e[key].font>=min,`${key} font ${e[key].font} < ${min}`);
  require(r.lowFonts.length===0,`small product text ${JSON.stringify(r.lowFonts)}`);
  for (const target of r.targets) {
    require(visible(target),`action outside viewport: ${target.text}`);
    if (!target.disabled) require(target.hit,`action obscured: ${target.text}`);
    require(target.height>=32,`action too small: ${target.text}`);
  }
  if(r.name==="chat") {require(e.message,"real user message missing");require(e.reading?.width<=1100,"reading line too long");require(visible(e.composer),"composer outside viewport");}
  if(r.name==="settings") {require(e.form?.width<=1180,"form too wide");require(visible(e.save)&&(e.save.disabled||e.save.hit),"save is not reachable");}
  if(r.name==="canvas") {require(e.node?.offsetWidth===240&&e.node?.offsetHeight===128,"node geometry changed");require(e.canvas?.height>=280,"canvas unusably short");}
  if(r.zoom!==1) require(Math.abs(r.dpr-r.zoom)<0.01,"browser zoom DPR not confirmed");
  return errors;
}
