import assert from "node:assert/strict";
import { readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";

export async function runSharedGoalBehavior(context) {
  if (process.env.YILIAN_SHARED_GOAL_CONTROLS_ONLY === "1") return runBoundedControls(context);
  const { page, api, until, phase, visit, graphDetail, makeGoal, runNode, approve, workspace, providerCalls, base, frontend, controlToken, workspacePath, evidenceDir } = context;
  const steps = [], notices = [], requests = [], eventConnections = [];
  let stage = "initial", offline = false;
  const summary = id => page.locator(`.goal-tracking-summary[data-graph-id="${id}"]`).first();
  const toastCount = title => notices.filter(item => item.message.startsWith(`${title}：`)).length;
  const command = async (name, id) => api("POST", "/api/commands", { command: name, request_id: crypto.randomUUID(), source: "controlled-behavior-e2e", schema_version: 1, payload: { graph_id: id } });
  const quiesce = async (ms = 3500) => new Promise(resolveWait => setTimeout(resolveWait, ms));
  await page.exposeFunction("__recordGoalNotice", notice => notices.push({ stage, ...notice }));
  await page.addInitScript(() => {
    const seen = new WeakSet();
    new MutationObserver(() => {
      for (const card of document.querySelectorAll('[aria-label="系统轻通知"]>div')) {
        if (!seen.has(card) && [...card.querySelectorAll("button")].some(button => button.textContent === "查看任务")) {
          seen.add(card); void window.__recordGoalNotice({ message: card.querySelector("span")?.textContent ?? "", at: Date.now() });
        }
      }
    }).observe(document, { childList: true, subtree: true });
  });
  page.on("request", request => { const path = new URL(request.url()).pathname; if (path.startsWith("/api/")) requests.push({ stage, method: request.method(), path, at: Date.now() }); });
  page.on("response", response => { if (new URL(response.url()).pathname === "/api/events") eventConnections.push({ stage, status: response.status(), at: Date.now() }); });
  try {
  await visit(`/chat/${context.conversation.id}`); await phase(context.goal.graph_id, "ready");
  await quiesce(1000);
  assert.equal(notices.length, 0, "historical first read must stay quiet");
  steps.push("historical_first_read_is_quiet");

  const title = "事件审批与暂停恢复验收";
  const goal = await makeGoal(workspace.id, title);
  const conversation = await api("POST", "/api/conversations", { title });
  await api("PUT", `/api/conversations/${conversation.id}/goal`, { graph_id: goal.graph_id });
  await visit(`/chat/${conversation.id}`); await phase(goal.graph_id, "ready");
  stage = "live_waiting_approval";
  const pending = await runNode(goal.graph_id);
  await phase(goal.graph_id, "waiting_approval");
  await until(async () => toastCount(title), count => count === 1, "one live approval notification");
  assert.equal(providerCalls.length, 0, "event observation must not invoke provider");
  await summary(goal.graph_id).getByRole("link", { name: "处理审批", exact: true }).click();
  await page.waitForURL(/\/system\?card=approvals$/);
  await page.locator(`[data-approval-id="${pending.approval_id}"]`).waitFor();
  assert.equal(providerCalls.length, 0);
  steps.push("real_waiting_event_notifies_and_links_to_existing_approval_without_execution");

  stage = "cross_surface_dedupe";
  await page.goBack(); await phase(goal.graph_id, "waiting_approval");
  await page.getByRole("region", { name: "系统轻通知" }).getByRole("button", { name: "查看任务", exact: true }).click();
  // The reminder's action intentionally points directly to the existing approval center.
  await page.waitForURL(/\/system\?card=approvals$/); await page.goBack();
  await phase(goal.graph_id, "waiting_approval");
  await summary(goal.graph_id).getByRole("link", { name: "工作空间", exact: true }).click(); await phase(goal.graph_id, "waiting_approval");
  await summary(goal.graph_id).getByRole("link", { name: "打开任务画布", exact: true }).click(); await phase(goal.graph_id, "waiting_approval");
  for (let index = 0; index < 2; index++) await summary(goal.graph_id).getByRole("button", { name: "刷新状态", exact: true }).click();
  await quiesce();
  assert.equal(toastCount(title), 1, "same pending approval must be deduped across route changes and refresh");
  assert.equal(providerCalls.length, 0);
  steps.push("same_waiting_status_deduped_across_chat_workspace_canvas_refresh");

  stage = "historical_pending_read_quiet";
  const historyContext = await context.browser.newContext();
  const historyPage = await historyContext.newPage();
  try {
    await historyPage.addInitScript(() => sessionStorage.setItem("yilian.setup.deferred", "true"));
    await historyPage.goto(`${frontend}/task-world/${goal.graph_id}`, { waitUntil: "domcontentloaded" });
    await phase(goal.graph_id, "waiting_approval", historyPage);
    await quiesce(1000);
    assert.equal(await historyPage.getByRole("region", { name: "系统轻通知", exact: true }).count(), 0, "opening an existing pending approval must not replay its old notice");
  } finally { await historyContext.close(); await page.bringToFront(); }
  steps.push("fresh_application_historical_pending_approval_stays_quiet");

  stage = "permission_read_and_mute";
  const beforeMode = await api("GET", "/api/security/approval-mode");
  await summary(goal.graph_id).getByRole("button", { name: "权限与提醒", exact: true }).click();
  const policyDialog = page.getByRole("dialog", { name: "权限与提醒", exact: true });
  await policyDialog.getByText(/当前审批模式：/).waitFor();
  const modeCheckbox = policyDialog.getByRole("checkbox", { name: "本目标开启状态提醒", exact: true });
  assert.equal(await modeCheckbox.isChecked(), true);
  await modeCheckbox.uncheck();
  assert.equal(await page.evaluate(id => localStorage.getItem(`yilian.goal-reminders.muted:${id}`), goal.graph_id), "true");
  await page.keyboard.press("Escape"); await policyDialog.waitFor({ state: "hidden" });
  assert.deepEqual(await api("GET", "/api/security/approval-mode"), beforeMode);
  assert.equal(requests.filter(item => item.path === "/api/security/approval-mode" && item.method !== "GET").length, 0);
  steps.push("permission_dialog_is_read_only_and_mute_is_goal_scoped_preference");

  stage = "pause_resume_while_muted";
  await summary(goal.graph_id).getByRole("button", { name: "暂停任务", exact: true }).click(); await phase(goal.graph_id, "paused");
  assert.equal(await page.getByRole("button", { name: "运行选中节点", exact: true }).isDisabled(), true);
  const refused = await fetch(`${base}/api/approvals/${pending.approval_id}/approve`, { method: "POST", headers: { "Content-Type": "application/json", "X-Yilian-Control-Session": controlToken }, body: "{}" });
  assert.equal(refused.status, 409, "paused task must refuse old pending approval");
  assert.equal(providerCalls.length, 0);
  await summary(goal.graph_id).getByRole("button", { name: "恢复任务", exact: true }).click(); await phase(goal.graph_id, "waiting_approval");
  assert.equal(providerCalls.length, 0, "resume must not approve the old operation");
  await quiesce(); assert.equal(toastCount(title), 1, "muted goal must not announce resumed approval");
  await summary(goal.graph_id).getByRole("button", { name: "权限与提醒", exact: true }).click();
  await policyDialog.getByRole("checkbox", { name: "本目标开启状态提醒", exact: true }).check();
  await page.keyboard.press("Escape");
  await quiesce(500); assert.equal(toastCount(title), 1, "unmute must not replay old pending notice");
  steps.push("pause_resume_preserves_approval_control_and_muted_transitions_stay_quiet");

  stage = "explicit_approval_and_artifact_event";
  await approve(pending);
  const succeeded = await until(() => graphDetail(goal.graph_id), detail => detail.nodes[0].latest_execution?.status === "succeeded", "explicit old approval after resume");
  assert.equal(providerCalls.length, 1);
  const produced = await api("POST", "/api/artifacts/from-execution", { source: { kind: "node", graph_id: goal.graph_id, node_id: "report", execution_id: succeeded.nodes[0].latest_execution.execution_id, workspace_id: workspace.id }, name: "shared-goal-report.md" }, 201);
  await phase(goal.graph_id, "completed");
  await until(async () => notices.filter(item => item.message.startsWith(`${title}：`) && item.message.includes("通过当前验收")).length, count => count === 1, "one verified artifact completion notification");
  const evaluationsBeforeRefresh = requests.filter(item => item.method === "POST" && /\/evaluations$/.test(item.path)).length;
  await summary(goal.graph_id).getByRole("button", { name: "刷新状态", exact: true }).click(); await quiesce();
  assert.equal(requests.filter(item => item.method === "POST" && /\/evaluations$/.test(item.path)).length, evaluationsBeforeRefresh, "refresh status must be read-only");
  assert.equal(providerCalls.length, 1);
  steps.push("explicit_approval_executes_once_artifact_event_verifies_and_refresh_does_not_write");

  stage = "external_file_change";
  const artifactPath = join(workspacePath, ".yilian-artifacts", `${produced.artifact.id}.md`);
  const originalArtifact = await readFile(artifactPath);
  try {
    await writeFile(artifactPath, "Controlled external modification of isolated artifact\n");
    await summary(goal.graph_id).getByRole("button", { name: "刷新状态", exact: true }).click(); await phase(goal.graph_id, "needs_revalidation");
    await until(async () => notices.filter(item => item.stage === "external_file_change").length, count => count === 1, "one stale file notification from authoritative re-read");
    assert.equal(requests.filter(item => item.method === "POST" && /\/evaluations$/.test(item.path)).length, evaluationsBeforeRefresh, "file observation must not silently revalidate");
  } finally { await writeFile(artifactPath, originalArtifact); }
  await summary(goal.graph_id).getByRole("button", { name: "核验产物", exact: true }).click(); await phase(goal.graph_id, "completed");
  assert.equal(providerCalls.length, 1);
  steps.push("external_file_change_notifies_without_execution_and_explicit_verification_recovers");

  stage = "synthetic_background_visibility";
  const bgTitle = "后台提醒延后验收";
  const backgroundGoal = await makeGoal(workspace.id, bgTitle);
  await visit(`/task-world/${backgroundGoal.graph_id}`); await phase(backgroundGoal.graph_id, "ready");
  await page.evaluate(() => { Object.defineProperty(document, "visibilityState", { configurable: true, get: () => window.__fixtureHidden ? "hidden" : "visible" }); window.__fixtureHidden = true; document.dispatchEvent(new Event("visibilitychange")); });
  await runNode(backgroundGoal.graph_id); await phase(backgroundGoal.graph_id, "waiting_approval");
  await quiesce(1000); assert.equal(toastCount(bgTitle), 0, "hidden document must defer its real event reminder");
  await page.evaluate(() => { window.__fixtureHidden = false; document.dispatchEvent(new Event("visibilitychange")); });
  await until(async () => toastCount(bgTitle), count => count === 1, "deferred reminder on visible");
  await page.evaluate(() => { delete document.visibilityState; delete window.__fixtureHidden; });
  await summary(backgroundGoal.graph_id).getByRole("button", { name: "取消任务", exact: true }).click(); await phase(backgroundGoal.graph_id, "cancelled");
  assert.equal((await api("GET", "/api/approvals/pending")).approvals.filter(item => item.task_graph_id === backgroundGoal.graph_id).length, 0);
  assert.equal(providerCalls.length, 1);
  steps.push("background_visibility_fixture_defers_real_event_notice_and_cancel_clears_pending");

  stage = "failed_execution_notice_and_recovery";
  const failedTitle = "失败提醒与显式恢复验收";
  const failedGoal = await makeGoal(workspace.id, failedTitle);
  await visit(`/task-world/${failedGoal.graph_id}`); await phase(failedGoal.graph_id, "ready");
  context.setProviderFailure(true);
  await approve(await runNode(failedGoal.graph_id)); await phase(failedGoal.graph_id, "failed");
  await until(async () => notices.filter(item => item.message.startsWith(`${failedTitle}：`) && item.message.includes("CONTROLLED provider failure")).length, count => count === 1, "one actual provider failure reminder");
  assert.equal(providerCalls.length, 2);
  await summary(failedGoal.graph_id).getByRole("link", { name: "定位失败并恢复", exact: true }).click();
  await page.waitForURL(/\?node=report$/);
  await page.locator("summary").filter({ hasText: "状态与执行记录" }).click();
  await page.getByRole("button", { name: "从此节点重跑", exact: true }).click();
  await until(() => graphDetail(failedGoal.graph_id), detail => detail.nodes[0].latest_execution?.status === "stale", "recovery explicitly prepared");
  await until(() => page.getByRole("button", { name: "运行选中节点", exact: true }).isDisabled(), disabled => !disabled, "prepared recovery must allow deliberate execution");
  context.setProviderFailure(false);
  await page.getByRole("button", { name: "运行选中节点", exact: true }).click();
  await approve(await context.approval(failedGoal.graph_id));
  const recovered = await until(() => graphDetail(failedGoal.graph_id), detail => detail.nodes[0].latest_execution?.status === "succeeded", "explicit retry succeeds");
  assert.equal(recovered.nodes[0].latest_execution.attempt, 2);
  assert.equal(providerCalls.length, 3);
  await api("POST", "/api/artifacts/from-execution", { source: { kind: "node", graph_id: failedGoal.graph_id, node_id: "report", execution_id: recovered.nodes[0].latest_execution.execution_id, workspace_id: workspace.id }, name: "shared-goal-report.md" }, 201);
  await phase(failedGoal.graph_id, "completed");
  steps.push("actual_failure_notifies_once_and_deliberate_node_retry_still_works");

  stage = "narrow_live_notice_layout";
  const narrowTitle = "窄窗口真实审批提醒验收";
  const narrowGoal = await makeGoal(workspace.id, narrowTitle);
  await page.setViewportSize({ width: 390, height: 844 });
  await visit(`/task-world/${narrowGoal.graph_id}`); await phase(narrowGoal.graph_id, "ready");
  await runNode(narrowGoal.graph_id); await phase(narrowGoal.graph_id, "waiting_approval");
  await until(async () => toastCount(narrowTitle), count => count === 1, "narrow live approval toast");
  const toast = page.getByRole("region", { name: "系统轻通知", exact: true });
  const toastBox = await toast.boundingBox();
  assert.ok(toastBox && toastBox.x >= 0 && toastBox.x + toastBox.width <= 391, `live notice clipped at 390: ${JSON.stringify(toastBox)}`);
  await page.screenshot({ path: join(evidenceDir, "live-notice-390.png"), fullPage: true, animations: "disabled" });
  await summary(narrowGoal.graph_id).getByRole("button", { name: "取消任务", exact: true }).click(); await phase(narrowGoal.graph_id, "cancelled");
  assert.equal(providerCalls.length, 3);
  await page.setViewportSize({ width: 1366, height: 768 });
  steps.push("real_pending_notice_and_its_action_fit_narrow_viewport");

  stage = "cdp_offline_reconnect";
  const reconnectGoal = await makeGoal(workspace.id, "实时断线重连验收");
  await visit(`/workspaces/${workspace.id}?tab=tasks&graph=${reconnectGoal.graph_id}`); await phase(reconnectGoal.graph_id, "ready");
  await runNode(reconnectGoal.graph_id); await phase(reconnectGoal.graph_id, "waiting_approval");
  const cdp = await page.context().newCDPSession(page);
  await cdp.send("Network.enable");
  const connectionCount = eventConnections.length;
  try {
    offline = true;
    await cdp.send("Network.emulateNetworkConditions", { offline: true, latency: 0, downloadThroughput: -1, uploadThroughput: -1 });
    await context.stopBackend();
    await page.getByText("实时连接正在恢复，仍会定时核对任务状态。", { exact: true }).waitFor();
    await context.restartBackend();
    assert.equal((await command("task.pause", reconnectGoal.graph_id)).status, "succeeded");
    offline = false;
    await cdp.send("Network.emulateNetworkConditions", { offline: false, latency: 0, downloadThroughput: -1, uploadThroughput: -1 });
    await phase(reconnectGoal.graph_id, "paused");
    await until(async () => eventConnections.length, count => count > connectionCount, "SSE connection restored");
    await page.getByText("实时连接正在恢复，仍会定时核对任务状态。", { exact: true }).waitFor({ state: "hidden" });
  } finally {
    if (offline) await cdp.send("Network.emulateNetworkConditions", { offline: false, latency: 0, downloadThroughput: -1, uploadThroughput: -1 });
    await cdp.detach();
  }
  await summary(reconnectGoal.graph_id).getByRole("button", { name: "取消任务", exact: true }).click(); await phase(reconnectGoal.graph_id, "cancelled");
  assert.equal(providerCalls.length, 3);
  steps.push("real_network_disconnect_reconnect_rereads_missed_authoritative_state_without_execution");
  await page.screenshot({ path: join(evidenceDir, "final-cancelled-controls.png"), fullPage: true, animations: "disabled" });
  return { steps, notices, requests, eventConnections, expectedConsoleBoundary: "CDP offline causes intentional network errors; document visibility override only simulates foreground/background lifecycle" };
  } finally { await writeFile(join(evidenceDir, "behavior-trace.json"), JSON.stringify({ stage, steps, notices, requests, eventConnections }, null, 2)); }
}

async function runBoundedControls(context) {
  const { page, api, phase, visit, runNode, goal, conversation, providerCalls, evidenceDir } = context;
  await page.setViewportSize({ width: 390, height: 844 });
  await runNode(goal.graph_id);
  const measurements = [];
  for (const [entry, path] of [["chat", `/chat/${conversation.id}`], ["canvas", `/task-world/${goal.graph_id}`]]) {
    await visit(path); const summary = await phase(goal.graph_id, "waiting_approval");
    for (const label of ["暂停任务", "取消任务", "权限与提醒"]) {
      const measurement = await summary.getByRole("button", { name: label, exact: true }).evaluate(element => {
        const rect = element.getBoundingClientRect();
        const summaryRect = element.closest(".goal-tracking-summary").getBoundingClientRect();
        const clip = { left: Math.max(0, summaryRect.left), top: Math.max(0, summaryRect.top), right: Math.min(innerWidth, summaryRect.right), bottom: Math.min(innerHeight, summaryRect.bottom) };
        const ancestors = [];
        for (let parent = element.parentElement; parent; parent = parent.parentElement) {
          const style = getComputedStyle(parent), parentRect = parent.getBoundingClientRect();
          if (style.display === "contents") continue;
          if (/(hidden|clip|auto|scroll)/.test(style.overflowX)) { clip.left = Math.max(clip.left, parentRect.left + parent.clientLeft); clip.right = Math.min(clip.right, parentRect.left + parent.clientLeft + parent.clientWidth); }
          if (/(hidden|clip|auto|scroll)/.test(style.overflowY)) { clip.top = Math.max(clip.top, parentRect.top + parent.clientTop); clip.bottom = Math.min(clip.bottom, parentRect.top + parent.clientTop + parent.clientHeight); ancestors.push({ className: parent.className, top: parentRect.top, clientHeight: parent.clientHeight }); }
        }
        return { rect: { left: rect.left, right: rect.right, top: rect.top, bottom: rect.bottom }, clip, ancestors };
      });
      measurements.push({ entry, label, ...measurement });
      const { rect, clip } = measurement;
      assert.ok(rect.left >= clip.left - 1 && rect.right <= clip.right + 1 && rect.top >= clip.top - 1 && rect.bottom <= clip.bottom + 1, `${entry} ${label} clipped by ancestor: ${JSON.stringify(measurement)}`);
    }
    await page.screenshot({ path: join(evidenceDir, `${entry}-waiting-controls-390.png`), fullPage: true, animations: "disabled" });
  }
  const summary = page.locator(`.goal-tracking-summary[data-graph-id="${goal.graph_id}"]`).first();
  await summary.getByRole("button", { name: "取消任务", exact: true }).click();
  await phase(goal.graph_id, "cancelled");
  assert.equal((await api("GET", "/api/approvals/pending")).approvals.filter(item => item.task_graph_id === goal.graph_id).length, 0);
  assert.equal(providerCalls.length, 0);
  await writeFile(join(evidenceDir, "clip-measurements.json"), JSON.stringify(measurements, null, 2));
  return { steps: ["initial_waiting_controls_fully_inside_all_clipping_ancestors_in_chat_and_canvas", "visible_cancel_button_performs_real_cancellation_and_clears_pending_without_provider_call"], notices: [], measurements };
}
