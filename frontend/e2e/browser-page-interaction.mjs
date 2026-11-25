// Rendered React regression with controlled IPC/API fixtures, not native acceptance.
// Run from frontend: node e2e/browser-page-interaction.mjs
import assert from "node:assert/strict";
import { access, mkdir, mkdtemp, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { createServer } from "vite";
import { chromium } from "playwright-core";

const frontend = resolve(import.meta.dirname, "..");
const evidence = process.env.YILIAN_BROWSER_UI_EVIDENCE || await mkdtemp(join(tmpdir(), "yilian-browser-ui-"));
await mkdir(evidence, { recursive: true });
const html = `<!doctype html><html><head><meta charset="utf-8"></head><body style="margin:0"><div id="root" style="height:100dvh"></div><script type="module" src="/__browser_fixture.tsx"></script></body></html>`;
const fixtureModule = `
import React from 'react';
import ReactDOM from 'react-dom/client';
import {MemoryRouter,useLocation} from 'react-router-dom';
import {useBrowserNavigationEvents} from '/src/surfaces/AppRoot.tsx';
import BrowserPage from '/src/features/browser/BrowserPage.tsx';
import {initializeControlSession} from '/src/api/controlSession.ts';
import {applyThemeToDom} from '/src/theme/applyTheme.ts';
import {DEFAULT_THEME} from '/src/theme/presets.ts';
import '/src/index.css';
applyThemeToDom(DEFAULT_THEME,'light');
await initializeControlSession();
const detached=new URLSearchParams(location.search).has('detached');
const root=ReactDOM.createRoot(document.getElementById('root'));
window.unmountBrowserFixture=()=>root.unmount();
function FixtureNavigation(){useBrowserNavigationEvents();const location=useLocation();return React.createElement('output',{'data-testid':'router-location',hidden:true},location.pathname+location.search);}
const initialRoute=detached?'/browser/window?tab=old-tab':new URLSearchParams(location.search).get('route')||'/browser';
const fixture=React.createElement(MemoryRouter,{initialEntries:[initialRoute]},React.createElement(React.Fragment,null,React.createElement(FixtureNavigation),React.createElement(BrowserPage,{detached})));
root.render(new URLSearchParams(location.search).has('strict')?React.createElement(React.StrictMode,null,fixture):fixture);
`;
const server = await createServer({
  root: frontend, logLevel: "error",
  server: { host: "127.0.0.1", port: 0, strictPort: false },
  plugins: [{ name: "browser-interaction-harness",
   resolveId(id) { if (id === "/__browser_fixture.tsx") return id; },
   load(id) { if (id === "/__browser_fixture.tsx") return fixtureModule; },
   configureServer(instance) {
    instance.middlewares.use(async (request, response, next) => {
      if (!request.url.startsWith("/__browser_interaction") || request.url.includes("html-proxy")) return next();
      try {
        response.setHeader("Content-Type", "text/html");
        response.end(await instance.transformIndexHtml(request.url, html));
      } catch (error) { next(error); }
    });
  } }],
});
let browser;
const results = [];
try {
  await server.listen();
  const candidates = [process.env.YILIAN_UI_BROWSER, "C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe", "C:/Program Files/Google/Chrome/Application/chrome.exe"].filter(Boolean);
  let executablePath;
  for (const path of candidates) { try { await access(path); executablePath = path; break; } catch {} }
  assert.ok(executablePath, "Set YILIAN_UI_BROWSER to an installed Chromium executable");
  browser = await chromium.launch({ executablePath, headless: true });
  const page = await browser.newPage({ viewport: { width: 1366, height: 768 } });
  page.setDefaultTimeout(30000);
  const errors = [];
  page.on("pageerror", error => { errors.push(String(error)); console.error(String(error)); });
  page.on("console", message => { if (message.type() === "error") { errors.push(message.text()); console.error(message.text()); } });
  await page.route("**/api/**", route => {
    const path = new URL(route.request().url()).pathname;
    if (!path.startsWith("/api/")) return route.continue();
    const body = path === "/api/workspaces" ? { workspaces: [{ id: "project", name: "项目 A", status: "active" }] }
      : path === "/api/browser/sites" ? { sites: [{ id: "saved", workspace_id: "project", title: "真实接口契约站点", url: "http://localhost:3000/saved", bookmarked: true, status: "active" }], truncated: false }
      : path === "/api/browser/visits" ? {visits:[{id:"known-visit",site_id:"saved",title:"已登记站点访问",redacted_url:"http://localhost:3000/[redacted]",visited_at:Date.now()},{id:"redacted-only",title:"仅脱敏历史",redacted_url:"https://example.com/[redacted]",visited_at:Date.now()}],truncated:false}
      : {};
    return route.fulfill({ json: body, headers: { "Access-Control-Allow-Origin": "*", "Access-Control-Allow-Headers": "*" } });
  });
  await page.addInitScript(() => {
    window.fixtureCalls = [];
    window.fixtureUnknown = [];
    window.fixturePresentation = null;
    window.fixtureHideDelay = 0;
    window.fixtureTabSequence = 0;
    window.fixtureUi = {requests: [], zoom: {}};
    window.fixtureCommands = [];
    window.fixtureListeners = {};
    window.fixtureListenerRegistrations = [];
    window.fixtureListenerRemovals = [];
    window.fixturePendingListeners = 0;
    const params = new URLSearchParams(location.search);
    window.fixtureListenerDelay = Number(params.get("listenDelay")||0);
    const listenerGate = params.has("listenHold") ? new Promise(resolve => { window.fixtureReleaseListeners = resolve; }) : null;
    window.fixtureCallbacks = {};
    window.fixtureEmit = (event, payload) => Object.values(window.fixtureListeners).filter(item => item.event === event).forEach(item => window.fixtureCallbacks[item.handler]({event, payload}));
    window.__TAURI_EVENT_PLUGIN_INTERNALS__ = {unregisterListener: (event,id) => {assertRegistered(event,id);window.fixtureListenerRemovals.push({event,id});delete window.fixtureListeners[id];}};
    function assertRegistered(event,id) {if(window.fixtureListeners[id]?.event!==event)throw new Error(`Unregistered listener ${event}:${id}`);}
    window.fixtureTabs = [{ session_id: "old-session", tab_id: "old-tab", navigation_id: 1, url: "http://localhost:3000/old", title: "Existing page", mode: new URLSearchParams(location.search).has("detached") ? "detached" : "embedded", can_go_back: false, can_go_forward: false }];
    window.__TAURI_INTERNALS__ = { transformCallback: callback => {const id=Object.keys(window.fixtureCallbacks).length+1;window.fixtureCallbacks[id]=callback;return id;}, invoke: async (command, args) => {
      if (command === "plugin:event|listen") {window.fixtureListenerRegistrations.push(args);window.fixturePendingListeners++;if(listenerGate)await listenerGate;else if(window.fixtureListenerDelay)await new Promise(resolve=>setTimeout(resolve,window.fixtureListenerDelay));window.fixtureListeners[args.handler]=args;window.fixturePendingListeners--;return args.handler;}
      if (command === "plugin:event|unlisten") return;
      if (command === "native_browser_ui_state") return structuredClone(window.fixtureUi);
      if (command === "native_browser_command") {window.fixtureCommands.push(args);if(args.command === "zoom") {window.fixtureUi.zoom[args.target.tab_id]=args.payload.factor;return {factor:args.payload.factor};}return {query:args.payload.query, matched:true,match_count:1,active_match_index:1};}
      if (command === "native_browser_resolve_request") {window.fixtureCommands.push(args);if(window.fixtureResolveError) throw new Error(window.fixtureResolveError);if(args.decision==="save"&&window.fixtureResolveCancelled)return {saved:false,cancelled:true};window.fixtureUi.requests=window.fixtureUi.requests.filter(item=>item.id!==args.id);return {};}
      if (command === "get_control_session_token") return "controlled-browser-interaction-token-only";
      if (command === "native_browser_tabs") return window.fixtureTabs;
      if (command === "native_browser_unknown") return window.fixtureUnknown;
      if (command !== "native_browser_execute") throw new Error(`Unexpected fixture command: ${command}`);
      const request = args.request;
      window.fixtureCalls.push(request);
      if (request.action === "present") {
        if (request.payload.mode === "hidden" && window.fixtureHideDelay) await new Promise(resolve => setTimeout(resolve, window.fixtureHideDelay));
        window.fixturePresentation = { mode: request.payload.mode, target: request.target };
      }
      if (request.action === "open") {
        const suffix = ++window.fixtureTabSequence;
        const tab = { session_id: `new-session-${suffix}`, tab_id: `new-tab-${suffix}`, navigation_id: 1, url: request.payload.url, title: "Opened page", mode: "embedded", workspace_id: request.payload.workspace_id, ephemeral: request.payload.ephemeral };
        window.fixtureTabs = [...window.fixtureTabs, tab];
        return { state: "verified", data: { tab } };
      }
      if (request.action === "navigate") window.fixtureTabs = window.fixtureTabs.map(tab => tab.tab_id === request.target.tab_id ? { ...tab, url: request.payload.url, navigation_id: tab.navigation_id + 1 } : tab);
      if (request.action === "close") window.fixtureTabs = window.fixtureTabs.filter(tab => tab.tab_id !== request.target.tab_id);
      return { state: "verified", data: {} };
    } };
  });
  const base = `http://127.0.0.1:${server.httpServer.address().port}/__browser_interaction`;
  await page.goto(base, { waitUntil: "commit" });
  const oldTab = page.getByRole("tab", { name: "Existing page" });
  await oldTab.waitFor();
  await page.waitForFunction(()=>Object.values(window.fixtureListeners).some(item=>item.event==="browser:shortcut"));
  assert.equal(await page.evaluate(()=>Object.values(window.fixtureListeners).filter(item=>item.event==="browser:opened").length),1,"the global router must be the sole browser:opened listener owner");
  const shortcutRegistrations = await page.evaluate(()=>window.fixtureListenerRegistrations.filter(item=>item.event==="browser:shortcut").length);
  await page.evaluate(()=>window.fixtureEmit("browser:opened",{tab_id:"old-tab"}));
  await page.waitForFunction(()=>document.querySelector('[data-testid="router-location"]').textContent==="/browser?tab=old-tab");
  await page.evaluate(()=>window.fixtureEmit("browser:shortcut",{action:"new-tab",tab_id:"old-tab"}));
  await page.getByRole("heading",{name:"搜索与浏览"}).waitFor();
  assert.equal(await page.getByTestId("router-location").textContent(),"/browser","starting a draft must remove the previously selected tab route");
  await page.evaluate(()=>window.fixtureEmit("browser:opened",{tab_id:"old-tab"}));
  await page.waitForFunction(()=>document.querySelector('[role="tab"][aria-selected="true"]')?.textContent.includes("Existing page"));
  await page.waitForTimeout(2300);
  assert.equal(await oldTab.getAttribute("aria-selected"),"true");
  assert.equal(await page.getByRole("heading",{name:"搜索与浏览"}).count(),0);
  results.push("native new-tab clears the old tab query so global browser:opened restores the same old tab across polling");
  assert.equal(await page.getByLabel("系统浏览器", { exact: true }).count(), 1);
  for (const width of [1366, 960, 390]) {
    await page.setViewportSize({ width, height: 768 });
    await page.screenshot({ path: join(evidence, `active-${width}.png`) });
    const geometry = await page.evaluate(() => {
      const section = document.querySelector(".browser-page");
      const canvas = document.querySelector(".browser-canvas").getBoundingClientRect();
      const controls = [...section.querySelectorAll("button,input,select")];
      return { width: section.clientWidth, scrollWidth: section.scrollWidth, canvasHeight: canvas.height, chromeHeight: canvas.top - section.getBoundingClientRect().top,
        outside: controls.filter(element => { const rect = element.getBoundingClientRect(); return rect.width > 0 && (rect.left < -1 || rect.right > innerWidth + 1); }).map(element => element.getAttribute("aria-label") || element.textContent) };
    });
    assert.ok(geometry.scrollWidth <= geometry.width + 1, JSON.stringify(geometry));
    assert.deepEqual(geometry.outside, [], JSON.stringify(geometry));
    assert.ok(geometry.canvasHeight > 500, JSON.stringify(geometry));
    if (width >= 960) assert.ok(geometry.chromeHeight <= 100, JSON.stringify(geometry));
    results.push({ activeWidth: width, geometry });
  }
  await page.getByLabel("新建标签页").click();
  await page.waitForFunction(() => window.fixtureCalls.some(call => call.action === "present" && call.payload.mode === "hidden" && call.target.tab_id === "old-tab"));
  await page.getByRole("heading", { name: "搜索与浏览" }).waitFor();
  assert.equal(await page.getByLabel("网址", { exact: true }).count(), 0);
  assert.equal(await page.getByLabel("搜索网页或输入网址", { exact: true }).count(), 1);
  assert.equal(await page.getByRole("navigation", { name: "项目站点快捷方式" }).count(), 0);
  assert.equal(await page.getByRole("button", { name: "管理项目站点", exact: true }).count(), 1);
  await page.keyboard.press("Control+l");
  assert.equal(await page.getByLabel("搜索网页或输入网址", { exact: true }).evaluate(element => document.activeElement === element), true);
  await oldTab.click();
  // Two complete 1-second polling cycles must retain the actual selected page.
  await page.waitForTimeout(2300);
  assert.equal(await oldTab.getAttribute("aria-selected"), "true");
  assert.equal(await page.getByRole("heading", { name: "搜索与浏览" }).count(), 0);
  assert.equal(await page.getByLabel("网址", { exact: true }).inputValue(), "http://localhost:3000/old");
  assert.equal(await page.getByLabel("关闭当前会话", { exact: true }).count(), 0);
  results.push("existing tab remains selected after + and two polling cycles");

  await page.evaluate(() => {
    window.fixtureUnknown = [{ operation_id: "review-fixture", action: "click", target: { session_id: "old-session", tab_id: "old-tab", navigation_id: 1 } }];
  });
  await page.getByRole("button", { name: "待审阅操作（1）" }).click();
  await page.getByRole("dialog", { name: "审阅不确定的浏览器操作" }).waitFor();
  await page.waitForFunction(() => window.fixturePresentation?.mode === "hidden");
  await page.evaluate(() => { window.fixtureCalls = []; window.fixtureHideDelay = 250; });
  await page.getByLabel("关闭审阅不确定的浏览器操作", { exact: true }).click();
  await page.waitForFunction(() => window.fixturePresentation?.mode === "embedded");
  await page.waitForTimeout(350);
  const restored = await page.evaluate(() => ({ mode: window.fixturePresentation?.mode,
    modes: window.fixtureCalls.filter(call => call.action === "present").map(call => call.payload.mode) }));
  assert.equal(restored.mode, "embedded", JSON.stringify(restored));
  assert.equal(restored.modes.includes("hidden"), false, JSON.stringify(restored));
  results.push("closing review restores Embedded without a same-tab cleanup Hidden, including delayed hide delivery");
  await page.evaluate(() => { window.fixtureUnknown = []; window.fixtureHideDelay = 0; });
  await page.evaluate(() => {
    window.fixtureCalls = [];
    window.fixtureTabs = window.fixtureTabs.map(tab => ({ ...tab, navigation_id: tab.navigation_id + 1 }));
  });
  await page.waitForFunction(() => window.fixtureCalls.some(call => call.action === "present" && call.target.navigation_id === 2));
  await page.setViewportSize({ width: 960, height: 768 });
  await page.waitForFunction(() => window.fixtureCalls.some(call => call.action === "present" && call.payload.width > 900));
  const updatedModes = await page.evaluate(() => window.fixtureCalls.filter(call => call.action === "present").map(call => call.payload.mode));
  assert.equal(updatedModes.includes("hidden"), false, JSON.stringify(updatedModes));
  results.push("same-tab navigation generation and viewport resize update geometry without ownership-release hides");

  await page.getByLabel("新建标签页", { exact: true }).click();
  await page.getByRole("heading", { name: "搜索与浏览" }).waitFor();
  for (const width of [1366, 960, 390]) {
    await page.setViewportSize({ width, height: 768 });
    await page.screenshot({ path: join(evidence, `start-${width}.png`) });
    const geometry = await page.evaluate(() => {
      const section = document.querySelector(".browser-page");
      const controls = [...section.querySelectorAll("button,input,select")].filter(element => element.getBoundingClientRect().width > 0);
      return { width: section.clientWidth, scrollWidth: section.scrollWidth, viewport: document.documentElement.clientWidth,
        outside: controls.filter(element => { const rect = element.getBoundingClientRect(); return rect.left < -1 || rect.right > innerWidth + 1; }).map(element => element.getAttribute("aria-label") || element.textContent) };
    });
    assert.ok(geometry.scrollWidth <= geometry.width + 1, JSON.stringify(geometry));
    assert.deepEqual(geometry.outside, [], JSON.stringify(geometry));
    results.push({ width, geometry });
  }
  await page.getByLabel("搜索网页或输入网址", { exact: true }).fill("忆涟 C++ & tools");
  await page.getByLabel("搜索或前往", { exact: true }).click();
  await page.getByRole("tab", { name: "Opened page" }).waitFor();
  const search = await page.evaluate(() => window.fixtureCalls.find(call => call.action === "open"));
  assert.equal(new URL(search.payload.url).searchParams.get("q"), "忆涟 C++ & tools");
  assert.equal(new URL(search.payload.url).origin, "https://www.bing.com");
  assert.equal(search.payload.ephemeral, false, "normal browsing must preserve the selected profile login");
  results.push("central search opens actual encoded Bing URL through browser action");

  await page.getByLabel("新建标签页", { exact: true }).click();
  await page.locator(".browser-start-site").filter({ hasText: "真实接口契约站点" }).click();
  const shortcut = await page.evaluate(() => window.fixtureCalls.filter(call => call.action === "open").at(-1));
  assert.equal(shortcut.payload.url, "http://localhost:3000/saved");
  assert.equal(shortcut.payload.site_id, "saved");
  assert.equal(shortcut.payload.workspace_id, "project");
  results.push("API favorite opens saved URL with project/site association");

  await page.setViewportSize({width:1366,height:768});
  await page.getByLabel("关闭网页 Existing page", {exact:true}).click();
  assert.equal(await page.getByRole("tab",{name:"Existing page",exact:true}).count(),0);
  assert.equal(await page.getByRole("tab",{selected:true}).count(),1);
  assert.match(await page.getByLabel("网址",{exact:true}).inputValue(),/saved/);
  results.push("background close preserves current selection without a destructive login-loss confirmation");

  await page.keyboard.press("Control+l");
  assert.equal(await page.getByLabel("网址",{exact:true}).evaluate(element=>document.activeElement===element),true);
  await page.keyboard.press("Control+t");
  await page.getByRole("heading",{name:"搜索与浏览"}).waitFor();
  await page.getByLabel("临时浏览",{exact:true}).check();
  await page.getByLabel("搜索网页或输入网址",{exact:true}).fill("https://example.com/temporary");
  await page.getByLabel("搜索或前往",{exact:true}).click();
  await page.waitForFunction(()=>document.querySelector('input[aria-label="网址"]')?.value==="https://example.com/temporary");
  assert.equal(await page.evaluate(()=>window.fixtureCalls.filter(call=>call.action==="open").at(-1).payload.ephemeral),true);
  await page.keyboard.press("Control+w");
  await page.getByRole("dialog",{name:"结束临时会话"}).waitFor();
  const beforeModal=await page.evaluate(()=>window.fixtureCalls.filter(call=>call.action==="close").length);
  await page.keyboard.press("Control+w");
  assert.equal(await page.evaluate(()=>window.fixtureCalls.filter(call=>call.action==="close").length),beforeModal);
  await page.getByRole("button",{name:"关闭并丢弃会话",exact:true}).click();
  await page.getByRole("dialog").waitFor({state:"detached"});
  assert.equal(await page.getByLabel("网址",{exact:true}).inputValue(),"http://localhost:3000/saved");
  results.push("DOM shortcuts focus address/create/close with explicit temporary mode and modal protection");

  await page.getByLabel("网址",{exact:true}).fill("https://example.com/unsubmitted-draft");
  await page.getByLabel("浏览器工具",{exact:true}).click();
  await page.getByRole("button",{name:"收藏当前页面",exact:true}).click();
  const bookmarkDialog=page.getByRole("dialog",{name:"登记项目站点"});
  assert.equal(await bookmarkDialog.getByLabel("网址",{exact:true}).inputValue(),"http://localhost:3000/saved");
  await page.getByLabel("关闭登记项目站点",{exact:true}).click();
  results.push("bookmark uses actual native page URL instead of unsubmitted omnibox text");

  await page.getByLabel("浏览历史",{exact:true}).click();
  const history=page.getByRole("dialog",{name:"浏览历史"});
  await history.getByText("仅脱敏历史",{exact:true}).waitFor();
  assert.equal(await history.getByRole("button",{name:"打开登记站点",exact:true}).count(),1);
  await history.getByLabel("搜索访问历史",{exact:true}).fill("仅脱敏");
  assert.equal(await history.getByRole("button",{name:"打开登记站点",exact:true}).count(),0);
  await history.getByLabel("搜索访问历史",{exact:true}).fill("no-match-fixture");
  await history.getByText("没有匹配的访问记录",{exact:true}).waitFor();
  await page.getByLabel("关闭浏览历史",{exact:true}).click();
  results.push("history filters real returned records and never replays redacted-only URLs");

  await page.evaluate(()=>window.fixtureEmit("browser:shortcut",{action:"focus-address",tab_id:window.fixtureTabs.at(-1).tab_id}));
  assert.equal(await page.getByLabel("网址",{exact:true}).evaluate(element=>document.activeElement===element),true);
  await page.getByLabel("浏览器工具",{exact:true}).click();
  await page.getByRole("button",{name:"放大网页",exact:true}).click();
  await page.getByRole("status").filter({hasText:"110%"}).waitFor();
  await page.getByRole("button",{name:"清除当前浏览数据…",exact:true}).click();
  assert.equal(await page.getByRole("button",{name:"确认清除浏览数据",exact:true}).isEnabled(),false);
  await page.getByLabel("我理解网站登录和本地网站数据将被清除",{exact:true}).check();
  assert.equal(await page.getByRole("button",{name:"确认清除浏览数据",exact:true}).isEnabled(),true);
  await page.evaluate(()=>{
    const tab={session_id:"other-project-session",tab_id:"new-tab-other-project",navigation_id:1,url:"https://example.com/other-project",title:"Other project page",mode:"embedded",workspace_id:"other-project",ephemeral:false};
    window.fixtureTabs=[...window.fixtureTabs,tab];
    window.fixtureEmit("browser:opened",{tab_id:tab.tab_id});
  });
  await page.waitForFunction(()=>document.querySelector('input[aria-label="网址"]')?.value==="https://example.com/other-project");
  assert.equal(await page.getByRole("button",{name:"确认清除浏览数据",exact:true}).count(),0,"clearing another target must require a fresh confirmation");
  assert.equal(await page.getByLabel("我理解网站登录和本地网站数据将被清除",{exact:true}).count(),0);
  assert.equal(await page.evaluate(()=>window.fixtureCommands.some(item=>item.command==="clear_data")),false);
  await page.getByRole("button",{name:"清除当前浏览数据…",exact:true}).click();
  assert.equal(await page.getByLabel("我理解网站登录和本地网站数据将被清除",{exact:true}).isChecked(),false);
  assert.equal(await page.getByRole("button",{name:"确认清除浏览数据",exact:true}).isEnabled(),false);
  await page.getByRole("button",{name:"取消",exact:true}).click();
  assert.equal(await page.evaluate(()=>window.fixtureCommands.some(item=>item.command==="clear_data")),false);
  await page.getByLabel("关闭浏览器工具",{exact:true}).click();
  await page.keyboard.press("Control+f");
  await page.getByLabel("在网页中查找",{exact:true}).fill("native text");
  await page.getByRole("button",{name:"查找下一项",exact:true}).click();
  await page.getByRole("status").filter({hasText:"1 / 1"}).waitFor();
  assert.ok(await page.evaluate(()=>window.fixtureCommands.some(item=>item.command==="find"&&item.payload.query==="native text")));
  await page.getByLabel("关闭网页查找",{exact:true}).click();
  results.push("native-forwarded focus, real zoom command and nonmodal native find controls");
  results.push("background browser:opened target switch remounts Tools and resets checked clear-data confirmation without sending clear_data");
  assert.equal(await page.evaluate(()=>window.fixtureListenerRegistrations.filter(item=>item.event==="browser:shortcut").length),shortcutRegistrations,"route query changes must not tear down the native shortcut listener");

  await page.evaluate(()=>{const tab=window.fixtureTabs.at(-1);window.fixtureUi.requests=[{id:"popup-fixture",kind:"popup",...tab,message:"网页请求打开新窗口",url:"https://example.com/a-very-long-popup-destination",navigation_id:tab.navigation_id}];});
  await page.getByRole("button",{name:"查看网页请求（1）",exact:true}).click();
  await page.getByRole("dialog",{name:"网页请求"}).waitFor();
  await page.setViewportSize({width:390,height:768});
  await page.screenshot({path:join(evidence,"popup-request-390.png"),animations:"disabled"});
  const popupGeometry=await page.getByRole("dialog",{name:"网页请求"}).evaluate(element=>({width:element.clientWidth,scrollWidth:element.scrollWidth}));
  assert.ok(popupGeometry.scrollWidth<=popupGeometry.width+1,JSON.stringify(popupGeometry));
  await page.getByRole("button",{name:"打开页面",exact:true}).click();
  await page.getByText("没有待处理的网页请求。").waitFor();
  assert.ok(await page.evaluate(()=>window.fixtureCommands.some(item=>item.id==="popup-fixture"&&item.decision==="open")));
  await page.getByLabel("关闭网页请求",{exact:true}).click();
  await page.setViewportSize({width:1366,height:768});
  results.push("structured popup displays trusted destination and resolves the exact request identity");

  await page.evaluate(()=>{const tab=window.fixtureTabs.at(-1);window.fixtureResolveCancelled=true;window.fixtureUi.requests=[{id:"download-fixture",kind:"download",...tab,message:"网页请求下载",suggested_name:"controlled.txt",url:"https://example.com/download"}];});
  await page.getByRole("button",{name:"查看网页请求（1）",exact:true}).click();
  await page.getByRole("button",{name:"选择保存位置",exact:true}).click();
  await page.getByText("已取消保存，下载请求仍保留，可重新选择位置。",{exact:true}).waitFor();
  assert.equal(await page.getByRole("button",{name:"选择保存位置",exact:true}).isEnabled(),true);
  await page.evaluate(()=>{window.fixtureResolveCancelled=false;window.fixtureResolveError="controlled save failed";});
  await page.getByRole("button",{name:"选择保存位置",exact:true}).click();
  await page.getByText("controlled save failed",{exact:true}).waitFor();
  assert.equal(await page.getByRole("button",{name:"选择保存位置",exact:true}).isEnabled(),true);
  await page.evaluate(()=>{window.fixtureResolveError="";});
  await page.getByRole("button",{name:"拒绝下载",exact:true}).click();
  await page.getByText("没有待处理的网页请求。",{exact:true}).waitFor();
  await page.getByLabel("关闭网页请求",{exact:true}).click();
  results.push("cancelled and failed native save preserves request and recoverable controls");

  await page.evaluate(()=>window.fixtureEmit("browser:new-tab",{}));
  await page.getByRole("heading",{name:"搜索与浏览"}).waitFor();
  assert.equal(await page.getByLabel("搜索网页或输入网址",{exact:true}).inputValue(),"");
  await page.waitForTimeout(2300);
  assert.equal(await page.getByRole("heading",{name:"搜索与浏览"}).count(),1);
  assert.equal(await page.getByRole("tab",{selected:true}).count(),0);
  assert.equal(await page.getByTestId("router-location").textContent(),"/browser");
  results.push("global browser:new-tab event consumes new nonce route and remains on the draft across polling");
  await page.getByRole("tab").last().click();

  await page.evaluate(() => { window.fixtureCalls = []; window.unmountBrowserFixture(); });
  await page.waitForFunction(() => window.fixtureCalls.some(call => call.action === "present" && call.payload.mode === "hidden" && call.target.tab_id.startsWith("new-tab-")));
  await page.waitForFunction(()=>Object.keys(window.fixtureListeners).length===0);
  results.push("component unmount hides its owned selected native tab");

  await page.goto(`${base}?detached`, { waitUntil: "commit" });
  await page.getByRole("tab", { name: "Existing page" }).waitFor();
  assert.equal(await page.getByLabel("新建标签页").count(), 0);
  assert.equal(await page.getByLabel("主页", { exact: true }).count(), 0);
  assert.equal(await page.getByLabel("系统浏览器", { exact: true }).count(), 0);
  assert.equal(await page.getByRole("button", { name: "回到主窗口", exact: true }).count(), 1);
  assert.equal(await page.getByLabel("关闭网页 Existing page", { exact: true }).count(), 1);
  assert.equal(await page.getByLabel("关闭当前会话", { exact: true }).count(), 0);
  results.push("System browser action exists in main chrome and is absent from the unsupported detached shell");
  await page.getByRole("navigation", { name: "项目站点快捷方式" }).getByRole("button").first().click();
  await page.waitForFunction(() => window.fixtureCalls.some(call => call.action === "navigate"));
  assert.equal(await page.evaluate(() => window.fixtureCalls.some(call => call.action === "open")), false);
  results.push("detached shortcuts navigate existing target without creating a new session");
  await page.getByLabel("网址",{exact:true}).focus();
  await page.keyboard.press("Control+t");
  await page.waitForFunction(()=>window.fixtureCommands.some(item=>item.command==="new_tab"));
  const detachedNewTabs=await page.evaluate(()=>window.fixtureCommands.filter(item=>item.command==="new_tab"));
  assert.equal(detachedNewTabs.length,1);
  assert.equal(detachedNewTabs[0].target.tab_id,"old-tab");
  assert.equal(detachedNewTabs[0].target.session_id,"old-session");
  await page.getByLabel("浏览器工具",{exact:true}).click();
  await page.keyboard.press("Control+t");
  assert.equal(await page.evaluate(()=>window.fixtureCommands.filter(item=>item.command==="new_tab").length),1);
  await page.getByLabel("关闭浏览器工具",{exact:true}).click();
  results.push("detached chrome Ctrl+T sends one targeted native new_tab command and is suppressed by its modal");
  await page.evaluate(()=>window.unmountBrowserFixture());
  await page.waitForFunction(()=>Object.keys(window.fixtureListeners).length===0);

  await page.goto(`${base}?strict&listenDelay=100`, { waitUntil: "commit" });
  await page.getByRole("tab",{name:"Existing page"}).waitFor();
  await page.waitForFunction(()=>window.fixtureListenerRegistrations.length===6&&window.fixturePendingListeners===0&&Object.keys(window.fixtureListeners).length===3);
  const strictListeners=await page.evaluate(()=>Object.values(window.fixtureListeners).map(item=>item.event).sort());
  assert.deepEqual(strictListeners,["browser:new-tab","browser:opened","browser:shortcut"]);
  await page.evaluate(()=>window.fixtureEmit("browser:opened",{tab_id:"old-tab"}));
  await page.waitForFunction(()=>document.querySelector('[data-testid="router-location"]').textContent==="/browser?tab=old-tab");
  assert.equal(await page.evaluate(()=>window.fixtureListenerRegistrations.length),6);
  await page.evaluate(()=>window.unmountBrowserFixture());
  await page.waitForFunction(()=>Object.keys(window.fixtureListeners).length===0);
  assert.equal(await page.evaluate(()=>window.fixtureListenerRemovals.length),6);
  results.push("StrictMode with delayed native registration leaves one listener per event, preserves subscriptions across query navigation and releases every registration exactly once");

  await page.goto(`${base}?route=${encodeURIComponent("/browser?tab=old-tab&workspace=project&new=controlled-nonce")}`, { waitUntil: "commit" });
  await page.getByRole("heading",{name:"搜索与浏览"}).waitFor();
  await page.waitForFunction(()=>document.querySelector('[data-testid="router-location"]').textContent==="/browser?workspace=project");
  await page.getByRole("tab",{name:"Existing page"}).click();
  await page.getByLabel("新建标签页",{exact:true}).click();
  assert.equal(await page.getByTestId("router-location").textContent(),"/browser?workspace=project");
  results.push("new nonce routing and direct new-tab share draft setup while preserving the workspace query");
  await page.evaluate(()=>window.unmountBrowserFixture());
  await page.waitForFunction(()=>Object.keys(window.fixtureListeners).length===0);

  await page.goto(`${base}?listenHold`, { waitUntil: "commit" });
  await page.getByRole("tab",{name:"Existing page"}).waitFor();
  await page.waitForFunction(()=>window.fixturePendingListeners===3);
  await page.evaluate(()=>{window.unmountBrowserFixture();window.fixtureReleaseListeners();});
  await page.waitForFunction(()=>window.fixturePendingListeners===0&&window.fixtureListenerRemovals.length===3);
  assert.equal(await page.evaluate(()=>Object.keys(window.fixtureListeners).length),0);
  results.push("unmount before native registration resolves releases all late listeners without duplicate unlisten");
  assert.deepEqual(errors, []);
  await writeFile(join(evidence, "results.json"), JSON.stringify({ kind: "controlled React + IPC/API fixture; not native validation", results, errors }, null, 2));
  console.log(JSON.stringify({ status: "PASS", evidence, results }, null, 2));
} finally {
  await browser?.close();
  await server.close();
}

