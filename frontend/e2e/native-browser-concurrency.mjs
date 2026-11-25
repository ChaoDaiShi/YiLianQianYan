import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { createServer } from "node:http";
import { mkdtemp, mkdir, writeFile, appendFile } from "node:fs/promises";
import { join, resolve } from "node:path";
import { tmpdir } from "node:os";
import { randomUUID } from "node:crypto";
import { chromium } from "playwright-core";
import { ownedBrowserWindow } from "./native-browser-windows.mjs";

// Real, owned WebView2 scheduling regressions. An explicitly feature-gated
// fixture binary and isolated data are mandatory; never attach to a user app.
assert.ok(process.env.YILIAN_NATIVE_BROWSER_BINARY, "Explicit fixture binary required");
const root = resolve(import.meta.dirname, "../..");
const isolated = await mkdtemp(join(tmpdir(), "yilian-browser-concurrency-"));
const evidence = join(isolated, "evidence");
await Promise.all([mkdir(evidence), mkdir(join(isolated, "workspace")), mkdir(join(isolated, "data"))]);
console.log(`[browser-concurrency] evidence ${evidence}`);
const children = [], servers = [], cases = [], logs = [];
let browser, main, app;
const selectedCase = process.env.YILIAN_BROWSER_CONCURRENCY_CASE || "core";
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
async function phase(name, data = {}) {
  const item = { at: Date.now(), name, ...data };
  await appendFile(join(evidence, "phases.jsonl"), JSON.stringify(item) + "\n");
  console.log(`[browser-concurrency-phase] ${name}`);
}
function bounded(work, ms, label) {
  let timer;
  return Promise.race([work, new Promise((_, reject) => timer = setTimeout(() => reject(new Error(`${label} timed out`)), ms))]).finally(() => clearTimeout(timer));
}
async function until(read, predicate, label, ms = 30000) {
  const end = Date.now() + ms;
  let last;
  while (Date.now() < end) {
    last = await bounded(Promise.resolve().then(read), 12000, label);
    if (predicate(last)) return last;
    await sleep(80);
  }
  throw new Error(`${label}: ${JSON.stringify(last)}`);
}
async function listen(server) {
  servers.push(server);
  await new Promise(resolve => server.listen(0, "127.0.0.1", resolve));
  return server.address().port;
}
async function freePort() {
  const server = createServer();
  await new Promise(resolve => server.listen(0, "127.0.0.1", resolve));
  const port = server.address().port;
  await new Promise(resolve => server.close(resolve));
  return port;
}
function launch(command, args, env, label) {
  const child = spawn(command, args, { cwd: label === "vite" ? join(root, "frontend") : root,
    env: { ...process.env, ...env }, windowsHide: label !== "tauri", stdio: ["ignore", "pipe", "pipe"] });
  for (const stream of [child.stdout, child.stderr]) stream.on("data", data => {
    const line = `[${label}] ${data}`; logs.push(line); void appendFile(join(evidence, "services-live.log"), line);
  });
  children.push(child);
  return child;
}
async function closeChild(child) {
  if (child.exitCode !== null || child.signalCode !== null) return;
  child.kill();
  await Promise.race([new Promise(resolve => child.once("exit", resolve)), sleep(4000)]);
  if (child.exitCode === null && child.signalCode === null) {
    child.kill("SIGKILL"); await new Promise(resolve => child.once("exit", resolve));
  }
}
async function invoke(command, args = {}) {
  return bounded(main.evaluate(({ command, args }) => window.__TAURI_INTERNALS__.invoke(command, args), { command, args }), 35000, command);
}
const schedule = args => invoke("native_browser_fixture_schedule", args);
const status = () => schedule({ action: "status" });
const tabs = () => invoke("native_browser_tabs");
const nativeState = tabId => invoke("native_browser_fixture_native_state", { tabId });
const target = tab => ({ session_id: tab.session_id, tab_id: tab.tab_id, navigation_id: tab.navigation_id });
function execute(action, payload = {}, tab, operation_id = randomUUID()) {
  return invoke("native_browser_execute", { request: { operation_id, action, target: tab ? target(tab) : null, payload, owner: null } });
}
async function leaveAutomaticPresenter() {
  await main.evaluate(() => {
    history.pushState({}, "", "/more");
    window.dispatchEvent(new PopStateEvent("popstate"));
  });
  await until(() => main.locator(".browser-page").count(), count => count === 0, "browser presenter unmounted");
  await until(status, value => value.pending_workers === 0, "previous presentation workers settled");
}
async function caseResult(name, run) {
  const shellCase = name === "quiesce_cleans_a_late_created_detached_shell";
  if (selectedCase === "core" ? shellCase : selectedCase === "shell" ? !shellCase : selectedCase !== name) return;
  try {
    const proof = await run();
    cases.push({ name, ...proof });
  } catch (error) {
    cases.push({ name, passed: false, failure_kind: "harness_or_unexpected_error", error: String(error.stack || error) });
  }
  await writeFile(join(evidence, "cases.json"), JSON.stringify(cases, null, 2));
  console.log(`[browser-concurrency] ${name}: ${JSON.stringify(cases.at(-1))}`);
}

const frontendPort = await freePort(), backendPort = await freePort(), debugPort = await freePort();
const frontend = `http://127.0.0.1:${frontendPort}`;
const fixtureServer = createServer((req, res) => {
  res.writeHead(200, { "Content-Type": "text/html; charset=utf-8" });
  res.end('<!doctype html><html><head><title>Concurrency fixture</title></head><body><h1>Owned native concurrency fixture</h1><input aria-label="Draft" value="retained"><p>No external accounts or content.</p></body></html>');
});
const fixtureUrl = `http://127.0.0.1:${await listen(fixtureServer)}`;
await writeFile(join(evidence, "ports.json"), JSON.stringify({ frontendPort, backendPort, debugPort, fixtureUrl }));

try {
  launch(process.execPath, [join(root, "frontend/node_modules/vite/bin/vite.js"), "--host", "127.0.0.1", "--port", String(frontendPort), "--strictPort"], { VITE_API_BASE: `http://127.0.0.1:${backendPort}` }, "vite");
  await until(async () => { try { return (await fetch(frontend)).status; } catch { return 0; } }, value => value === 200, "isolated Vite");
  app = launch(process.env.YILIAN_NATIVE_BROWSER_BINARY, [], {
    OPENAI_API_KEY: "", RUST_LOG: "info", YILIAN_NATIVE_FIXTURE_ROOT: isolated,
    YILIAN_NATIVE_FIXTURE_KEY: "local-controlled-placeholder", YILIAN_DATA_DIR: join(isolated, "data"),
    YILIAN_WORKSPACE: join(isolated, "workspace"), YILIAN_HOST: `127.0.0.1:${backendPort}`,
    YILIAN_ALLOWED_ORIGINS: frontend, YILIAN_NATIVE_BROWSER_FRONTEND_URL: frontend,
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${debugPort}`,
  }, "tauri");
  await until(async () => { try { return (await fetch(`http://127.0.0.1:${debugPort}/json/version`)).status; } catch { return 0; } }, value => value === 200, "owned WebView2 CDP", 50000);
  browser = await chromium.connectOverCDP(`http://127.0.0.1:${debugPort}`);
  main = await until(() => browser.contexts().flatMap(context => context.pages()).find(page => page.url().startsWith(frontend)), Boolean, "trusted main renderer");
  await main.evaluate(() => sessionStorage.setItem("yilian.setup.deferred", "true"));
  await main.goto(frontend + "/more");
  await until(() => main.evaluate(() => typeof window.__TAURI_INTERNALS__?.invoke), value => value === "function", "actual native IPC");
  await until(async () => { try { return (await fetch(`http://127.0.0.1:${backendPort}/api/health`)).status; } catch { return 0; } }, value => value === 200, "isolated backend");

  await caseResult("cancel_open_during_real_native_creation", async () => {
    const id = randomUUID();
    await schedule({ action: "arm", stage: "creation_registered", key: id, holdMs: 20000 });
    await schedule({ action: "start_open", request: { operation_id: id, action: "open", payload: { url: fixtureUrl + "/cancelled", ephemeral: true, allowed_origins: [fixtureUrl] } } });
    const held = await until(status, value => value.stages.creation_registered?.entered, "real add_child paused before harden");
    const tabId = held.stages.creation_registered.tab_id;
    const beforeCancel = await nativeState(tabId);
    const initiallyPublic = (await tabs()).some(tab => tab.tab_id === tabId);
    await schedule({ action: "cancel_open", key: id });
    const cancelled = await until(status, value => value.results[id], "cancelled runtime Open receipt");
    await schedule({ action: "release", stage: "creation_registered" });
    const settled = await until(status, value => value.pending_workers === 0, "creation worker and cleanup settlement");
    const remaining = (await tabs()).find(tab => tab.tab_id === tabId);
    const afterCancel = await nativeState(tabId);
    let residualNavigation;
    if (remaining && afterCancel.registered) {
      await execute("present", { mode: "embedded", x: 120, y: 150, width: 800, height: 450 }, remaining);
      residualNavigation = await execute("navigate", { url: fixtureUrl + "/unexpected-navigation", allowed_origins: [fixtureUrl] }, remaining);
      await execute("close", {}, remaining);
    }
    await invoke("native_browser_acknowledge_unknown", { operationId: id }).catch(() => undefined);
    return { passed: !initiallyPublic && !remaining && !afterCancel.registered && settled.native_closed?.includes(tabId), failure_kind: "behavior_assertion", operation_id: id, tab_id: tabId,
      beforeCancel, cancelledReceipt: cancelled.results[id], afterCancel, publiclyListedAfterCancel: !!remaining,
      initiallyPublic, residualNavigation, pending_workers: settled.pending_workers, native_closed: settled.native_closed };
  });

  await caseResult("late_creation_after_response_timeout_is_really_disposed", async () => {
    const id = randomUUID(), started = Date.now();
    await schedule({ action: "arm", stage: "creation_registered", key: id, holdMs: 25000 });
    await schedule({ action: "start_open", request: { operation_id: id, action: "open", payload: { url: fixtureUrl + "/late-creation", ephemeral: true, allowed_origins: [fixtureUrl] } } });
    const held = await until(status, value => value.stages.creation_registered?.entered, "late native creation held");
    const tabId = held.stages.creation_registered.tab_id;
    const actualControllerBefore = await nativeState(tabId);
    const initiallyPublic = (await tabs()).some(tab => tab.tab_id === tabId);
    const receipt = await until(status, value => value.results[id], "15-second native response deadline", 20000);
    const elapsed_ms = Date.now() - started;
    await schedule({ action: "release", stage: "creation_registered" });
    const settled = await until(status, value => value.pending_workers === 0, "late native worker cleanup");
    const after = await nativeState(tabId);
    const publicAfter = (await tabs()).some(tab => tab.tab_id === tabId);
    await invoke("native_browser_acknowledge_unknown", { operationId: id }).catch(() => undefined);
    return { passed: !initiallyPublic && !publicAfter && !after.registered && settled.native_closed?.includes(tabId) && elapsed_ms >= 14000 && elapsed_ms < 20000,
      failure_kind: "behavior_assertion", operation_id: id, tab_id: tabId, elapsed_ms, initiallyPublic, publicAfter,
      actualControllerBefore, receipt: receipt.results[id], after, native_closed: settled.native_closed, pending_workers: settled.pending_workers };
  });

  await caseResult("older_show_cannot_override_newer_hidden", async () => {
    const opened = await execute("open", { url: fixtureUrl + "/presentation", ephemeral: true, allowed_origins: [fixtureUrl] });
    const tab = opened.data.tab;
    await until(() => main.locator(".browser-page").count(), count => count === 1, "open event presenter mounted");
    await leaveAutomaticPresenter();
    await execute("present", { mode: "embedded", x: 120, y: 150, width: 800, height: 450 }, tab);
    const before = await nativeState(tab.tab_id);
    assert.equal(before.native_visible, true);
    await schedule({ action: "arm", stage: "presentation_show", key: tab.tab_id, holdMs: 20000 });
    const oldShow = execute("present", { mode: "embedded", x: 120, y: 150, width: 800, height: 450 }, tab);
    await until(status, value => value.stages.presentation_show?.entered, "older show paused before native show");
    let hiddenSettled = false;
    const newerHidden = execute("present", { mode: "hidden" }, tab).then(value => { hiddenSettled = true; return value; });
    await Promise.race([newerHidden, sleep(1200)]);
    const hiddenCompletedBeforeRelease = hiddenSettled;
    const beforeRelease = await nativeState(tab.tab_id);
    await schedule({ action: "release", stage: "presentation_show" });
    const replies = await Promise.all([oldShow, newerHidden]);
    await until(status, value => value.pending_workers === 0, "both presentation workers settled");
    const after = await nativeState(tab.tab_id);
    await execute("close", {}, tab);
    return { passed: after.native_visible === false, failure_kind: "behavior_assertion", tab_id: tab.tab_id,
      before, hiddenCompletedBeforeRelease, beforeRelease, after, replies };
  });

  await caseResult("pending_detach_survives_old_main_presenter_cleanup", async () => {
    await phase("pending-detach:open-start");
    const opened = await execute("open", { url: fixtureUrl + "/pending-detach", ephemeral: true, allowed_origins: [fixtureUrl] });
    await phase("pending-detach:open-returned", { opened });
    assert.equal(opened.state, "verified", "setup Open must complete before detach regression");
    const tab = opened.data.tab;
    await until(() => main.locator(".browser-page").count(), count => count === 1, "detach presenter mounted");
    await leaveAutomaticPresenter();
    await phase("pending-detach:presenter-left");
    await execute("present", { mode: "embedded", x: 120, y: 150, width: 800, height: 450 }, tab);
    await schedule({ action: "arm", stage: "shell_creating", key: tab.tab_id, holdMs: 20000 });
    const detached = execute("present", { mode: "detached", x: 10, y: 150, width: 800, height: 450 }, tab);
    await until(status, value => value.stages.shell_creating?.entered, "detached shell creation held");
    await phase("pending-detach:shell-held");
    const oldMainHidden = await execute("present", { mode: "hidden" }, tab);
    await phase("pending-detach:old-hidden-returned", { oldMainHidden });
    await schedule({ action: "release", stage: "shell_creating" });
    await phase("pending-detach:shell-released");
    const detachedReply = await detached;
    await phase("pending-detach:detached-returned");
    await until(status, value => value.pending_workers === 0, "pending detach settled");
    const actual = await nativeState(tab.tab_id);
    const current = (await tabs()).find(item => item.tab_id === tab.tab_id);
    const windowProof = await ownedBrowserWindow({ appPid: app.pid, expectedBinary: process.env.YILIAN_NATIVE_BROWSER_BINARY, recordChild: child => children.push(child) });
    const passed = oldMainHidden.data.ignored_stale_presenter === true && current?.mode === "detached" && actual.native_visible === true && windowProof.visibleCount === 1;
    await phase("pending-detach:close-start");
    await execute("close", {}, tab);
    await phase("pending-detach:close-returned");
    return { passed, failure_kind: "behavior_assertion", tab_id: tab.tab_id, oldMainHidden, detachedReply, current, actual, windowProof };
  });

  await caseResult("new_main_hidden_wins_while_attach_has_not_moved_parent", async () => {
    const opened = await execute("open", { url: fixtureUrl + "/pending-attach", ephemeral: true, allowed_origins: [fixtureUrl] });
    assert.equal(opened.state, "verified");
    const tab = opened.data.tab;
    await until(() => main.locator(".browser-page").count(), count => count === 1, "attach setup presenter mounted");
    await leaveAutomaticPresenter();
    await execute("present", { mode: "detached", x: 10, y: 150, width: 800, height: 450 }, tab);
    await until(status, value => value.pending_workers === 0, "detached setup workers settled");
    // Allow the newly built trusted shell to finish its initial DOM layout;
    // the actual attach/hide ordering below is controlled by the native probe.
    await sleep(1200);
    await schedule({ action: "arm", stage: "presentation_show", key: tab.tab_id, holdMs: 20000 });
    const olderAttach = execute("present", { mode: "embedded", x: 120, y: 150, width: 800, height: 450 }, tab);
    await until(status, value => value.stages.presentation_show?.entered, "attach held before physical reparent");
    const hidden = await execute("present", { mode: "hidden" }, tab);
    const afterHidden = await nativeState(tab.tab_id);
    await schedule({ action: "release", stage: "presentation_show" });
    const attachReply = await olderAttach;
    await until(status, value => value.pending_workers === 0, "inverse transition workers settled");
    const after = await nativeState(tab.tab_id);
    await execute("close", {}, tab);
    return { passed: !hidden.data.ignored_stale_presenter && afterHidden.native_visible === false && after.native_visible === false,
      failure_kind: "behavior_assertion", tab_id: tab.tab_id, hidden, afterHidden, attachReply, after };
  });

  await caseResult("quiesce_waits_for_actual_ui_disposal", async () => {
    const opened = await execute("open", { url: fixtureUrl + "/disposal", ephemeral: true, allowed_origins: [fixtureUrl] });
    const tab = opened.data.tab;
    await until(() => main.locator(".browser-page").count(), count => count === 1, "disposal presenter mounted");
    await leaveAutomaticPresenter();
    await execute("present", { mode: "embedded", x: 120, y: 150, width: 800, height: 450 }, tab);
    const actualControllerBefore = await nativeState(tab.tab_id);
    assert.equal(actualControllerBefore.native_visible, true);
    await main.evaluate(async ({ tabId }) => {
      await window.__TAURI_INTERNALS__.invoke("native_browser_fixture_schedule", { action: "arm", stage: "disposal_ui", key: tabId, holdMs: 4000 });
      await window.__TAURI_INTERNALS__.invoke("native_browser_fixture_schedule", { action: "start_quiesce" });
    }, { tabId: tab.tab_id });
    const settled = await until(status, value => value.results.quiesce, "quiesce outcome after bounded real UI stall", 20000);
    const result = settled.results.quiesce;
    const noFalseCompletion = !(result.ok && result.ui_pause_active);
    let finalState = settled;
    if (!result.ok) {
      await schedule({ action: "start_quiesce" });
      finalState = await until(status, value => value.results.quiesce, "fresh quiesce retry after UI release");
    }
    return { passed: noFalseCompletion && finalState.results.quiesce.ok && finalState.native_closed?.includes(tab.tab_id) && finalState.content_labels.length === 0 && finalState.shell_labels.length === 0,
      failure_kind: "behavior_assertion", tab_id: tab.tab_id, actualControllerBefore, result,
      finalResult: finalState.results.quiesce, native_closed: finalState.native_closed,
      stages: finalState.stages, remainingContentLabels: finalState.content_labels, remainingShellLabels: finalState.shell_labels, pending_workers: finalState.pending_workers };
  });

  await caseResult("quiesce_cleans_a_late_created_detached_shell", async () => {
    const opened = await execute("open", { url: fixtureUrl + "/late-shell", ephemeral: true, allowed_origins: [fixtureUrl] });
    const tab = opened.data.tab, shellId = `browser-shell-${tab.tab_id}`;
    await until(() => main.locator(".browser-page").count(), count => count === 1, "late shell presenter mounted");
    await leaveAutomaticPresenter();
    await execute("present", { mode: "embedded", x: 120, y: 150, width: 800, height: 450 }, tab);
    await schedule({ action: "arm", stage: "shell_creating", key: tab.tab_id, holdMs: 20000 });
    const late = execute("present", { mode: "detached" }, tab).then(reply => ({ reply }), error => ({ error: String(error) }));
    await until(status, value => value.stages.shell_creating?.entered, "late shell creation held before real build");
    await schedule({ action: "start_quiesce" });
    const blocked = await until(status, value => value.results.quiesce, "bounded quiesce while worker is held");
    const heldResult = blocked.results.quiesce;
    await schedule({ action: "release", stage: "shell_creating" });
    const lateReply = await late;
    const settled = await until(status, value => value.pending_workers === 0, "late shell controller ownership disposed");
    await schedule({ action: "start_quiesce" });
    const finalState = await until(status, value => value.results.quiesce, "quiesce retry after late shell cleanup");
    const windowProof = await ownedBrowserWindow({ appPid: app.pid, expectedBinary: process.env.YILIAN_NATIVE_BROWSER_BINARY, recordChild: child => children.push(child) });
    return { passed: !heldResult.ok && finalState.results.quiesce.ok && finalState.native_closed?.includes(tab.tab_id) && finalState.native_closed?.includes(shellId) && finalState.content_labels.length === 0 && finalState.shell_labels.length === 0 && windowProof.visibleCount === 0,
      failure_kind: "behavior_assertion", tab_id: tab.tab_id, heldResult, lateReply, settled, finalState, windowProof };
  });

  assert.ok(cases.length > 0, "No native concurrency case matched YILIAN_BROWSER_CONCURRENCY_CASE");
  const result = { status: cases.every(item => item.passed) ? "passed" : "failed", evidence, isolated,
    boundary: "Actual isolated Tauri/WebView2, real native controller settings/visibility and UI-thread scheduling; local pages only", cases };
  await writeFile(join(evidence, "result.json"), JSON.stringify(result, null, 2));
  console.log(JSON.stringify({ status: result.status, evidence, cases: cases.length, passed: cases.filter(item => item.passed).length }));
  if (result.status !== "passed") process.exitCode = 1;
} finally {
  if (main && !main.isClosed()) {
    for (const stage of ["creation_registered", "presentation_show", "shell_creating", "disposal_ui"]) await bounded(schedule({ action: "release", stage }), 2000, "cleanup stage").catch(() => undefined);
    for (const tab of await bounded(tabs(), 2000, "cleanup tabs").catch(() => [])) await bounded(execute("close", {}, tab), 2000, "cleanup close").catch(() => undefined);
  }
  if (browser) await bounded(browser.close(), 4000, "owned CDP close").catch(() => undefined);
  const cleanup = await Promise.allSettled(children.slice().reverse().map(closeChild));
  for (const server of servers) { server.closeAllConnections(); await new Promise(resolve => server.close(resolve)); }
  await writeFile(join(evidence, "cleanup.json"), JSON.stringify(cleanup.map(item => ({ status: item.status, reason: item.status === "rejected" ? String(item.reason) : null })), null, 2));
  await writeFile(join(evidence, "services.log"), logs.join(""));
  if (cleanup.some(item => item.status === "rejected")) throw new Error("Owned fixture cleanup failed");
}
