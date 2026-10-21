import { createRequire } from "node:module";
import { mkdir, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { resolve } from "node:path";

const directory = fileURLToPath(new URL(".", import.meta.url));
const require = createRequire(new URL("../../../../frontend/package.json", import.meta.url));
const { chromium } = require("playwright-core");
const temp = resolve(directory, ".render-temp");
await mkdir(temp, { recursive: true });
process.env.TEMP = temp;
process.env.TMP = temp;
const browser = await chromium.launch({
  executablePath: "C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe",
  headless: true,
});
const page = await browser.newPage({ viewport: { width: 1440, height: 958 }, deviceScaleFactor: 1 });
const errors = [], screenshots = [], checks = {};
page.on("pageerror", error => errors.push(error.message));
async function capture(file) {
  await page.evaluate(() => document.fonts.ready);
  const geometry = await page.evaluate(() => {
    const root = document.querySelector(".app").getBoundingClientRect();
    const overflow = [...document.querySelectorAll(".sidebar,.page,.canvas,.node,.dialog,.dialog-footer,.composer,.call-panel,.monitor-float,.navigation-card,.pool-panel,.pool-notice,.other-services")]
      .filter(element => element.getClientRects().length)
      .filter(element => {
        const rect = element.getBoundingClientRect();
        return rect.left < root.left || rect.top < root.top || rect.right > root.right + 1 || rect.bottom > root.bottom + 1;
      }).map(element => element.className);
    return { width: root.width, height: root.height, railWidth: document.querySelector(".sidebar").getBoundingClientRect().width, overflow };
  });
  await page.locator(".app").screenshot({ path: resolve(directory, file) });
  screenshots.push({ file, ...geometry });
}
async function show(view, theme = "light") {
  await page.evaluate(({ view, theme }) => window.preview.setView(view, theme), { view, theme });
}
try {
  await page.goto(new URL("index.html", import.meta.url).href);
  for (const [view, theme, file] of [
    ["connectors", "light", "01-connectors-light.png"],
    ["call", "light", "02-call-light.png"],
    ["capabilities", "light", "03-capabilities-light.png"],
    ["settings", "light", "04-settings-light.png"],
    ["security", "light", "05-security-light.png"],
    ["monitor", "light", "06-monitor-light.png"],
    ["connectors", "dark", "07-connectors-dark.png"],
    ["settings", "dark", "08-settings-dark.png"],
    ["more", "light", "09-more-navigation-light.png"],
  ]) {
    await show(view, theme);
    await capture(file);
  }
  await page.evaluate(()=>window.preview.setModelRouteState("fallback"));
  await capture("15-model-pool-fallback-light.png");
  checks.poolSwitchNotice=await page.locator(".pool-notice.switched").isVisible()&&(await page.locator(".pool-notice").textContent()).includes("连接超时");
  checks.actualModelDesign=await page.locator(".pool-tier.current").getAttribute("data-tier")==="tier-b";
  await show("pluginfeature");
  await capture("16-plugin-feature-light.png");
  await show("settings");
  await page.locator("[data-add-provider]").click();
  await capture("17-provider-account-light.png");
  await page.locator("#provider-name").selectOption("MiniMax");
  await page.locator("#provider-alias").fill("语音与对话账户");
  await page.locator("[data-editor-save]").click();
  checks.providersCoexist=await page.locator(".provider-account").count()===4;
  await page.locator('[data-tier-up="tier-b"]').click();
  checks.poolOrderEditable=await page.locator(".pool-tier").first().getAttribute("data-tier")==="tier-b";
  await page.locator("[data-add-tier]").click();
  await page.locator("#tier-model").fill("备用对话档案");
  await page.locator("[data-editor-save]").click();
  checks.poolTierAdded=await page.locator(".pool-tier").count()===4;
  await page.reload();
  await show("connectors");
  checks.compactGap = await page.evaluate(() => {
    const agent=document.querySelector(".node.agent").getBoundingClientRect();
    const left=document.querySelector(".node.conv").getBoundingClientRect();
    const right=document.querySelector(".node.resource").getBoundingClientRect();
    return agent.left-left.right===56 && right.left-agent.right===56;
  });
  const keys=await page.locator("[data-node]").evaluateAll(nodes=>nodes.map(n=>n.dataset.node));
  checks.allNodeEditors=true;
  for(const key of keys){
    await page.locator(`[data-node="${key}"]`).click();
    checks.allNodeEditors &&= await page.locator(".editor-layer .editing-dialog").isVisible();
    await page.locator("[data-editor-close]").click();
  }
  await page.locator('[data-node="model"]').click();
  await capture("10-node-editor-light.png");
  await page.locator("#node-title").fill("写作模型");
  await page.locator("[data-editor-save]").click();
  checks.nodeEditUpdatesSummary=await page.locator('[data-node="model"] h3').textContent()==="写作模型";
  await page.locator('[data-filter="conversations"]').click();
  checks.conversationFilter=await page.locator(".node:visible").count()===4 && await page.locator(".wires path").count()===3;
  await capture("11-conversations-only-light.png");
  await page.locator("[data-display]").click();
  checks.displayEditor=await page.locator(".display-options label").count()===8;
  await page.locator("[data-editor-cancel]").click();
  await page.locator('[data-filter="all"]').click();
  checks.filterRestoresNodes=await page.locator(".node:visible").count()===keys.length;
  await page.locator("[data-manage]").click();
  await capture("12-canvas-manager-light.png");
  await page.locator('[data-delete-board="canvas-02"]').click();
  checks.deleteTargetIsClickedRow=(await page.locator(".delete-target").textContent()).includes("canvas-02");
  await capture("13-delete-canvas-light.png");
  await page.locator("[data-confirm-delete]").click();
  checks.nonCurrentDeleteKeepsCurrent=await page.locator('[data-board-row="canvas-02"]').count()===0 && await page.locator("[data-board-choice]").inputValue()==="canvas-01";
  await page.locator('[data-delete-board="canvas-01"]').click();
  await page.locator("[data-confirm-delete]").click();
  checks.currentDeleteSelectsNext=await page.locator("[data-board-choice]").inputValue()==="canvas-03";
  await page.locator('[data-delete-board="canvas-03"]').click();
  await page.locator("[data-confirm-delete]").click();
  checks.lastDeleteShowsEmpty=await page.locator(".empty-canvas").isVisible();
  await page.locator('[data-manager-kind="history"]').click();
  await page.locator('[data-delete-board="TG-19E8B882"]').click();
  checks.historyDeleteTarget=(await page.locator(".delete-target").textContent()).includes("TG-19E8B882");
  await page.locator("[data-confirm-delete]").click();
  checks.historyDeleteRowRemoved=await page.locator('[data-board-row="TG-19E8B882"]').count()===0;
  await page.reload();
  await show("connectors");
  await page.locator('.rail-primary [data-view="more"]').click();
  checks.moreIsNavigation=await page.locator(".topbar h1").textContent()==="更多" && await page.locator(".navigation-card").count()===7;
  checks.moreGridCentered=await page.evaluate(()=>{const main=document.querySelector(".main").getBoundingClientRect(),grid=document.querySelector(".navigation-grid").getBoundingClientRect();return Math.abs((grid.left+grid.right)-(main.left+main.right))<2;});
  await page.locator('[data-destination="plugin"]').click();
  checks.pluginFeatureEntry=await page.locator(".topbar h1").textContent()==="文档工作台";
  await page.locator('.page-title-group [data-view="more"]').click();
  await page.locator('[data-destination="monitor"]').click();
  checks.moreOpensMonitor=await page.locator(".topbar h1").textContent()==="监控";
  await page.locator('.page-title-group [data-view="more"]').click();
  checks.returnToMore=await page.locator(".topbar h1").textContent()==="更多";
  await show("capabilities");
  checks.capabilityGridWithoutDialog=await page.locator(".navigation-card").count()===6 && !(await page.locator("#primary-dialog").isVisible());
  checks.capabilityGridCentered=await page.evaluate(()=>{const main=document.querySelector(".main").getBoundingClientRect(),grid=document.querySelector(".navigation-grid").getBoundingClientRect();return Math.abs((grid.left+grid.right)-(main.left+main.right))<2;});
  await page.locator('[data-open-cap="Skill"]').click();
  await capture("14-skill-dialog-light.png");
  await page.locator("[data-more]").click();
  checks.readingDialogOpened=await page.locator(".reading-dialog").isVisible();
  await page.locator(".reading-dialog button").click();
  await page.locator("[data-close]").click();
  await page.locator('[data-open-cap="工作流"]').click();
  checks.categoryDialogMatches=await page.locator(".cap-detail h2").textContent()==="演示准备流程";
  await show("call");
  await page.locator("[data-collapse]").click();
  checks.callCollapsed=!(await page.locator(".call-panel").isVisible());
  await page.locator("[data-expand]").click();
  checks.callExpanded=await page.locator(".call-panel").isVisible();
  const report={
    scope:"Document preview visuals and local interaction controls; no production API or database execution",
    reference:{railWidth:80,more:"centered navigation page with plugin feature",capabilities:"centered 3 columns x 2 rows",compactGap:56,models:"multiple provider accounts and ordered tiers"},
    screenshots,previewControls:checks,errors,
  };
  await writeFile(resolve(directory,"render-check.json"),JSON.stringify(report,null,2)+"\n");
  console.log(JSON.stringify(report,null,2));
  if(errors.length || screenshots.some(s=>s.overflow.length||s.railWidth!==80) || Object.values(checks).some(v=>v!==true))process.exitCode=1;
}finally{
  await browser.close();
}
