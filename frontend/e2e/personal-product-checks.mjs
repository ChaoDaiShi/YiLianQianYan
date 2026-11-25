// Product-state acceptance over the isolated real backend started by
// personal-assistant.mjs. Only transport faults and explicit history metadata
// are fixtures; successful commands, files and state transitions use real APIs.
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { existsSync } from "node:fs";
import { mkdir, readFile, readdir, writeFile } from "node:fs/promises";
import { join, resolve, sep } from "node:path";
import { randomUUID } from "node:crypto";

export async function runPersonalProductChecks(context) {
  const { page, api, history, until, waitStatus, create, workspace, data, ws, base, frontend, evidence, calls, steps, setPhase, expectHttpFailure } = context;
  const routes = [];
  const detailPath = id => `/api/automations/${id}/personal`;
  const startPath = id => `${detailPath(id)}/start`;
  const getTask = async id => (await api("GET", "/api/automations")).automations.find(item => item.id === id);
  const state = id => api("GET", detailPath(id));
  const rowFor = task => page.locator(".management-automation-row").filter({ hasText: task.name });
  const dialog = () => page.getByRole("dialog");
  const tab = name => dialog().getByRole("tab", { name, exact: true });
  async function scenario(name, work) { await work(); steps.push(name); }
  async function stopDetailAndWait(id, button) {
    // Cancellation intent is persisted before the server drains the worker and
    // clears waiters. Assert completion after the actual stop response.
    const response = page.waitForResponse(value => value.url() === `${base}/api/automations/${id}/stop` && value.request().method() === "POST");
    await button.click();
    const completed = await response;
    assert.equal(completed.status(), 200, await completed.text());
  }
  async function list(task) {
    await page.goto(`${frontend}/automations`);
    await page.getByLabel("搜索任务", { exact: true }).fill(task ? task.name : "");
    if (task) await rowFor(task).getByRole("button", { name: "任务详情", exact: true }).waitFor();
  }
  async function openDetail(task, loaded = true) {
    await list(task);
    await rowFor(task).getByRole("button", { name: "任务详情", exact: true }).click();
    await dialog().waitFor({ state: "visible" });
    if (loaded) await dialog().getByText(task.target.goal, { exact: true }).waitFor();
  }
  async function closeDetail() {
    if (await dialog().count()) await dialog().getByRole("button", { name: "关闭详情", exact: true }).click();
  }
  async function installRoute(url, handler) { routes.push({ url, handler }); await page.route(url, handler); }
  async function removeRoute(handler) {
    const index = routes.findIndex(value => value.handler === handler);
    if (index >= 0) { const [route] = routes.splice(index, 1); await page.unroute(route.url, route.handler); }
  }
  const exactRoute = url => candidate => candidate.toString() === url;
  function fault(handlerName) {
    return async route => {
      expectHttpFailure(route.request().url());
      await route.fulfill({ status: 500, contentType: "application/json", body: JSON.stringify({ error: `Controlled ${handlerName} transport failure` }) });
    };
  }
  async function capture(name) { await page.waitForTimeout(350); await page.screenshot({ path: join(evidence, name) }); }
  async function geometry(label) {
    const results = [];
    for (const width of [1366, 960, 390]) {
      await page.setViewportSize({ width, height: 768 });
      const dimensions = await page.evaluate(() => {
        const modal = document.querySelector('[role="dialog"]');
        return { width: document.documentElement.clientWidth, scroll: document.documentElement.scrollWidth,
          modalWidth: modal?.clientWidth, modalScroll: modal?.scrollWidth };
      });
      assert.ok(dimensions.scroll <= dimensions.width + 2, `${label}: document overflow ${JSON.stringify(dimensions)}`);
      assert.ok(!dimensions.modalWidth || dimensions.modalScroll <= dimensions.modalWidth + 2, `${label}: modal overflow ${JSON.stringify(dimensions)}`);
      results.push({ viewport: width, ...dimensions });
      await capture(`product-${label}-${width}.png`);
    }
    await writeFile(join(evidence, `product-${label}-geometry.json`), JSON.stringify(results, null, 2));
    await page.setViewportSize({ width: 1366, height: 768 });
  }

  try {
    await scenario("Management detail exposes real tool/verification records and a scope-checked generated-file preview; 1366/960/390 layouts remain usable", async () => {
      const tasks = (await api("GET", "/api/automations")).automations;
      const task = tasks.find(item => item.name === "本地个人助手验收");
      assert.ok(task, "original real report task is available");
      await openDetail(task);
      await tab("执行过程").click();
      await dialog().getByText("write_file", { exact: true }).first().waitFor();
      await dialog().getByText("read_file", { exact: true }).first().waitFor();
      await capture("product-execution-records.png");
      await tab("报告与文件").click();
      const detail = await state(task.id);
      const output = detail.files.find(file => file.path.endsWith("report.md"));
      assert.ok(output); assert.equal(output.execution_status, "succeeded");
      await dialog().getByRole("button", { name: "预览文件", exact: true }).first().click();
      const preview = dialog().locator('section[aria-label="生成文件只读预览"]');
      await preview.waitFor();
      assert.ok((await preview.innerText()).includes("Controlled local project notes summarized."));
      await geometry("generated-preview");
      await closeDetail();
    });

    await scenario("Generated-file preview rejects current read denial and externally modified output, and recovers after the fixture grant/content is restored", async () => {
      const task = (await api("GET", "/api/automations")).automations.find(item => item.name === "本地个人助手验收");
      const output = (await state(task.id)).files.find(file => file.path.endsWith("report.md"));
      const report = join(workspace, "report.md"), original = await readFile(report, "utf8");
      assert.ok(resolve(report).startsWith(`${resolve(workspace)}${sep}`));
      let deny;
      await openDetail(task); await tab("报告与文件").click();
      const previewUrl = `${base}${detailPath(task.id)}/file?tool_call_id=${encodeURIComponent(output.tool_call_id)}`;
      expectHttpFailure(previewUrl); setPhase("intentional_management_failure");
      try {
        deny = await api("POST", "/api/security/grants", { permission_id: "filesystem.read", effect: "deny", resource: { type: "filesystem", root: report, recursive: false } });
        await dialog().getByRole("button", { name: "预览文件", exact: true }).first().click();
        await dialog().getByRole("alert").filter({ hasText: "文件预览失败" }).waitFor();
        assert.match(await dialog().getByRole("alert").filter({ hasText: "文件预览失败" }).innerText(), /权限|授权/);
        await api("DELETE", `/api/security/grants/${deny.id}`); deny = null;
        await writeFile(report, "Controlled external modification.\n");
        await dialog().getByRole("button", { name: "预览文件", exact: true }).first().click();
        await dialog().getByRole("alert").filter({ hasText: "修改" }).waitFor();
        await capture("product-preview-modified.png");
      } finally {
        if (deny) await api("DELETE", `/api/security/grants/${deny.id}`);
        await writeFile(report, original); setPhase("normal");
      }
      await dialog().getByRole("button", { name: "预览文件", exact: true }).first().click();
      await dialog().locator('section[aria-label="生成文件只读预览"]').waitFor();
      await closeDetail();
    });

    await scenario("Search has an honest no-match state; personal archive preserves detail/history and restore remains paused without running a model", async () => {
      const task = await create("管理归档验收", "MANAGEMENT_ARCHIVE: a controlled text-only delegation", ws.id);
      const before = calls.length;
      await list();
      await page.getByLabel("搜索任务", { exact: true }).fill("no-such-controlled-task-0182");
      await page.getByText("没有匹配的任务", { exact: true }).waitFor();
      await page.getByLabel("搜索任务", { exact: true }).fill(task.name);
      await rowFor(task).locator("summary").click();
      await rowFor(task).getByRole("button", { name: `归档 ${task.name}`, exact: true }).click();
      await dialog().getByRole("button", { name: "归档", exact: true }).click();
      await until(() => getTask(task.id), value => value.archived && !value.enabled, "archive stored");
      await page.getByRole("button", { name: "已归档", exact: true }).click();
      await rowFor(task).getByRole("button", { name: "任务详情", exact: true }).click();
      await dialog().getByText(task.target.goal, { exact: true }).waitFor();
      await tab("全部历史").click();
      await dialog().getByText("尚无运行记录", { exact: true }).waitFor();
      await tab("概览与操作").click();
      assert.equal(await dialog().getByRole("button", { name: "运行一轮", exact: true }).count(), 0);
      await dialog().getByRole("button", { name: "恢复归档（保持暂停）", exact: true }).click();
      await until(() => getTask(task.id), value => !value.archived && !value.enabled && value.next_run_at === null, "restore stays paused");
      assert.equal(calls.length, before);
      await closeDetail();
    });

    await scenario("Failure recovery requires acknowledging the exact prior occurrence; it starts one fresh turn and retains the original failure", async () => {
      const task = await create("管理失败恢复验收", "MANAGEMENT_RECOVERY: fail the first controlled turn, then finish a new one", ws.id);
      await openDetail(task);
      await dialog().getByRole("button", { name: "运行一轮", exact: true }).click();
      const failed = (await waitStatus(task.id, "failed")).occurrences.find(item => item.status === "failed");
      await dialog().getByRole("button", { name: "刷新详情", exact: true }).click();
      const resume = dialog().getByRole("button", { name: "核实后继续新一轮", exact: true });
      await resume.waitFor(); assert.equal(await resume.isEnabled(), false);
      assert.ok((await dialog().innerText()).includes(failed.id));
      await dialog().getByLabel("已核实本轮文件和执行记录，继续新一轮", { exact: true }).check();
      await resume.click(); await waitStatus(task.id, "completed");
      const stored = (await history(task.id)).occurrences;
      assert.equal(stored.length, 2); assert.equal(stored.find(item => item.id === failed.id).status, "failed");
      await dialog().getByRole("button", { name: "刷新详情", exact: true }).click();
      await tab("全部历史").click();
      await dialog().getByText(failed.id, { exact: false }).first().waitFor();
      await capture("product-reviewed-recovery.png");
      await closeDetail();
    });

    await scenario("A lost accepted start response is reconciled with the same persisted request UUID and creates only one occurrence", async () => {
      const task = await create("管理响应丢失验收", "MANAGEMENT_LOST_RESPONSE: one controlled text-only turn", ws.id);
      const url = `${base}${startPath(task.id)}`, payloads = [];
      let dropped = false;
      const handler = async route => {
        payloads.push(route.request().postDataJSON());
        if (!dropped) {
          dropped = true; const response = await route.fetch();
          assert.equal(response.status(), 200);
          expectHttpFailure(route.request().url()); await route.abort("failed");
        } else await route.continue();
      };
      setPhase("intentional_management_failure"); await installRoute(exactRoute(url), handler);
      try {
        await openDetail(task);
        await dialog().getByRole("button", { name: "运行一轮", exact: true }).click();
        await dialog().getByRole("button", { name: "核对上次启动请求", exact: true }).waitFor();
        await waitStatus(task.id, "completed");
        await closeDetail(); await openDetail(task);
        await dialog().getByRole("button", { name: "核对上次启动请求", exact: true }).click();
        await until(() => history(task.id), value => value.occurrences.length === 1, "one idempotent occurrence");
        await until(async () => dialog().getByRole("button", { name: "核对上次启动请求", exact: true }).count(), value => value === 0, "accepted request cleared");
        assert.equal(payloads.length, 2); assert.deepEqual(payloads[1], payloads[0]);
        assert.equal((await history(task.id)).occurrences.length, 1);
      } finally { await removeRoute(handler); setPhase("normal"); }
      await closeDetail();
    });

    await scenario("A definitive stale-version rejection clears the unaccepted request, then a refreshed new request can run", async () => {
      const task = await create("管理版本冲突验收", "MANAGEMENT_STALE: a controlled text-only turn", ws.id);
      await openDetail(task);
      const snapshot = await state(task.id);
      const frozen = async route => route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify(snapshot) });
      const url = `${base}${detailPath(task.id)}`;
      await installRoute(exactRoute(url), frozen);
      setPhase("intentional_management_failure"); expectHttpFailure(`${base}${startPath(task.id)}`);
      try {
        await api("POST", `/api/automations/${task.id}/enabled`, { enabled: false, expected_revision: task.revision });
        await dialog().getByRole("button", { name: "运行一轮", exact: true }).click();
        await dialog().getByRole("alert").filter({ hasText: "版本" }).waitFor();
        assert.equal((await history(task.id)).occurrences.length, 0);
        assert.equal(await dialog().getByRole("button", { name: "核对上次启动请求", exact: true }).count(), 0);
      } finally { await removeRoute(frozen); setPhase("normal"); }
      await dialog().getByRole("button", { name: "刷新详情", exact: true }).click();
      await until(async () => dialog().getByRole("button", { name: "运行一轮", exact: true }).isEnabled(), Boolean, "refreshed start enabled");
      await dialog().getByRole("button", { name: "运行一轮", exact: true }).click();
      await waitStatus(task.id, "completed"); assert.equal((await history(task.id)).occurrences.length, 1);
      await closeDetail();
    });

    await scenario("Waiting input and approval expose the original conversation and forbid duplicate starts; stop clears waiting without creating the pending file", async () => {
      const ask = await create("管理等待回答验收", "ASK: ask a controlled management question before continuing", ws.id);
      await api("POST", `/api/automations/${ask.id}/run`); await waitStatus(ask.id, "waiting_input");
      await openDetail(ask);
      assert.equal(await dialog().getByRole("button", { name: "运行一轮", exact: true }).isEnabled(), false);
      await dialog().getByText("待回答：报告要面向谁？", { exact: true }).waitFor();
      await dialog().getByRole("button", { name: "打开原对话处理等待项", exact: true }).click();
      await page.waitForURL(`${frontend}/chat/${ask.target.conversation_id}`);
      await page.getByText("报告要面向谁？", { exact: true }).first().waitFor();
      await openDetail(ask); await dialog().getByRole("button", { name: "停止委托", exact: true }).click();
      await waitStatus(ask.id, "cancelled"); await closeDetail();
      await api("PUT", "/api/security/approval-mode", { mode: "full_approval" });
      try {
        const pending = await create("管理等待审批验收", "APPROVAL: create pending.md only after explicit permission", ws.id);
        await api("POST", `/api/automations/${pending.id}/run`); await waitStatus(pending.id, "waiting_approval");
        assert.equal(existsSync(join(workspace, "pending.md")), false);
        await openDetail(pending);
        assert.equal(await dialog().getByRole("button", { name: "运行一轮", exact: true }).isEnabled(), false);
        await dialog().getByText(/待审批：write_file/).waitFor();
        await stopDetailAndWait(pending.id, dialog().getByRole("button", { name: "停止委托", exact: true }));
        await waitStatus(pending.id, "cancelled");
        assert.equal((await state(pending.id)).pending_approvals.length, 0);
        assert.equal(existsSync(join(workspace, "pending.md")), false);
        await closeDetail();
      } finally { await api("PUT", "/api/security/approval-mode", { mode: "partial_approval" }); }
    });

    await scenario("Running work disables editing/restart; stale and first-load detail failures still allow a real stop-and-pause request", async () => {
      for (const initialFailure of [false, true]) {
        const task = await create(`管理${initialFailure ? "首次" : "过期"}读取失败停止验收`, `MANAGEMENT_HOLD: ${initialFailure ? "first load" : "stale load"} cancellation`, ws.id);
        if (!initialFailure) await openDetail(task);
        const handler = fault(initialFailure ? "first-detail" : "stale-detail");
        setPhase("intentional_management_failure"); await installRoute(exactRoute(`${base}${detailPath(task.id)}`), handler);
        try {
          await api("POST", startPath(task.id), { expected_revision: task.revision, request_id: randomUUID() });
          await until(() => api("GET", `/api/conversations/${task.target.conversation_id}`), value => value.run_status === "running", "held management turn running");
          if (initialFailure) await openDetail(task, false);
          else await dialog().getByRole("button", { name: "刷新详情", exact: true }).click();
          await dialog().getByRole("alert").filter({ hasText: "读取详情失败" }).waitFor();
          if (!initialFailure) assert.equal(await dialog().getByRole("button", { name: "运行一轮", exact: true }).isEnabled(), false);
          const stop = dialog().getByRole("button", { name: "停止并暂停委托", exact: true });
          assert.equal(await stop.isEnabled(), true); await stop.click();
          await waitStatus(task.id, "cancelled"); assert.equal((await getTask(task.id)).enabled, false);
          await capture(`product-${initialFailure ? "first" : "stale"}-read-stop.png`);
        } finally { await removeRoute(handler); setPhase("normal"); }
        await closeDetail();
      }
      const held = await create("管理运行状态验收", "MANAGEMENT_HOLD: running controls", ws.id);
      await api("POST", startPath(held.id), { expected_revision: held.revision, request_id: randomUUID() });
      await until(() => api("GET", `/api/conversations/${held.target.conversation_id}`), value => value.run_status === "running", "running controls");
      await list(held); await rowFor(held).locator("summary").click();
      assert.equal(await rowFor(held).getByRole("button", { name: `编辑 ${held.name}`, exact: true }).isEnabled(), false);
      assert.equal(await rowFor(held).getByRole("button", { name: "立即运行", exact: true }).count(), 0);
      await rowFor(held).getByRole("button", { name: "任务详情", exact: true }).click();
      await dialog().getByText(held.target.goal, { exact: true }).waitFor();
      await dialog().getByRole("button", { name: "暂停后续触发", exact: true }).click();
      await until(() => getTask(held.id), value => !value.enabled, "schedule paused");
      assert.equal((await api("GET", `/api/conversations/${held.target.conversation_id}`)).run_status, "running");
      await dialog().getByRole("button", { name: "停止委托", exact: true }).click();
      await waitStatus(held.id, "cancelled"); await closeDetail();
    });

    await scenario("History transport failure stays an error rather than an empty history; retry loads the real empty state", async () => {
      const task = await create("管理历史错误验收", "MANAGEMENT_HISTORY_ERROR: no model call expected", ws.id, false);
      await openDetail(task);
      const handler = fault("history");
      const route = url => url.origin === new URL(base).origin && url.pathname === `${detailPath(task.id)}/history`;
      setPhase("intentional_management_failure"); await installRoute(route, handler);
      try {
        await tab("全部历史").click();
        await dialog().getByRole("alert").filter({ hasText: "读取历史失败" }).waitFor();
        assert.equal(await dialog().getByText("尚无运行记录", { exact: true }).count(), 0);
        await capture("product-history-error.png");
      } finally { await removeRoute(handler); setPhase("normal"); }
      const retry = dialog().getByRole("button", { name: "重试读取历史", exact: true });
      if (await retry.count()) await retry.click();
      await dialog().getByText("尚无运行记录", { exact: true }).waitFor();
      await closeDetail();
    });

    await scenario("Real SQLite cursor pagination reaches 127 metadata-fixture records beyond the former 100-row limit and preserves status filtering", async () => {
      const task = await create("管理长期分页验收", "MANAGEMENT_PAGING: fixture metadata only, not 127 model runs", ws.id, false);
      const profiles = await readdir(join(data, "profiles"), { withFileTypes: true });
      const databases = profiles.filter(entry => entry.isDirectory()).map(entry => join(data, "profiles", entry.name, "app.db")).filter(existsSync);
      assert.equal(databases.length, 1, "one isolated profile database required");
      const script = `import json,pathlib,sqlite3,sys,uuid\nroot=pathlib.Path(sys.argv[1]).resolve();db=pathlib.Path(sys.argv[2]).resolve();db.relative_to(root)\na=json.loads(sys.argv[3]);now=a['created_at']+1\nconn=sqlite3.connect(str(db),timeout=10)\nwith conn:\n for i in range(127):\n  ident='product-fixture-'+str(uuid.uuid4());status='completed' if i==0 else 'skipped'\n  item={'id':ident,'automation_id':a['id'],'automation_revision':a['revision'],'timezone':'UTC','occurrence_key':ident,'trigger':'manual','scheduled_at':now,'workspace_id':a['workspace_id'],'workflow_graph_id':'','run_id':None,'robot_run_id':None,'robot_channel_id':None,'conversation_id':a['target']['conversation_id'],'assistant_turn_id':None,'report':'Controlled metadata report' if i==0 else None,'status':status,'error':None,'created_at':now,'updated_at':now}\n  conn.execute('INSERT INTO automation_occurrences(id,automation_id,occurrence_key,payload,created_at) VALUES(?,?,?,?,?)',(ident,a['id'],ident,json.dumps(item),now))\nconn.close()\n`;
      execFileSync("python", ["-c", script.replaceAll("\\n", "\n"), data, databases[0], JSON.stringify(task)], { windowsHide: true, encoding: "utf8" });
      const first = await api("GET", `${detailPath(task.id)}/history`); assert.equal(first.total, 127); assert.equal(first.occurrences.length, 25);
      await openDetail(task); await tab("全部历史").click();
      await dialog().getByText(/已显示 25 \/ 127 条记录/).waitFor();
      for (let count = 50; count <= 125; count += 25) {
        await dialog().getByRole("button", { name: "加载更多历史", exact: true }).click();
        await dialog().getByText(new RegExp(`已显示 ${count} / 127 条记录`)).waitFor();
      }
      await dialog().getByRole("button", { name: "加载更多历史", exact: true }).click();
      await dialog().getByText(/已显示 127 \/ 127 条记录/).waitFor();
      assert.equal(await dialog().getByRole("button", { name: "加载更多历史", exact: true }).count(), 0);
      await tab("概览与操作").click(); await tab("全部历史").click();
      await dialog().getByText(/已显示 127 \/ 127 条记录/).waitFor();
      await dialog().getByLabel("历史状态", { exact: true }).selectOption("completed");
      await dialog().getByText(/已显示 1 \/ 1 条记录/).waitFor();
      await dialog().getByText("Controlled metadata report", { exact: true }).waitFor();
      await geometry("history");
      await closeDetail();
    });

    await scenario("Budget exhaustion prevents another run until the bounded total is explicitly edited; increasing it permits exactly one additional controlled turn", async () => {
      const task = (await api("POST", "/api/automations", { name: "管理预算配置验收", workflow_graph_id: "", workspace_id: ws.id,
        target: { kind: "personal_assistant", goal: "MANAGEMENT_BUDGET: a controlled bounded turn", allow_file_writes: false, max_runs: 1 },
        timezone: "UTC", schedule: { kind: "daily", time: "09:00" }, enabled: false }, 201)).automation;
      await api("POST", startPath(task.id), { expected_revision: task.revision, request_id: randomUUID() });
      await waitStatus(task.id, "completed"); await openDetail(task);
      await dialog().getByText(/委托轮次预算已用完/).waitFor();
      assert.equal(await dialog().getByRole("button", { name: "运行一轮", exact: true }).isEnabled(), false);
      await closeDetail();
      await rowFor(task).locator("summary").click();
      await rowFor(task).getByRole("button", { name: `编辑 ${task.name}`, exact: true }).click();
      await dialog().getByLabel("委托最多执行轮次", { exact: true }).fill("2");
      await dialog().getByRole("button", { name: "保存任务", exact: true }).click();
      await dialog().waitFor({ state: "hidden" });
      await openDetail(task); await dialog().getByRole("button", { name: "运行一轮", exact: true }).click();
      await until(() => history(task.id), value => value.occurrences.length === 2 && value.occurrences.every(item => item.status === "completed"), "one more budgeted turn");
      await dialog().getByRole("button", { name: "刷新详情", exact: true }).click();
      await dialog().getByText("已用 2 / 2 轮", { exact: true }).waitFor();
      assert.equal(await dialog().getByRole("button", { name: "运行一轮", exact: true }).isEnabled(), false);
      await capture("product-budget-exhausted.png"); await closeDetail();
    });

    await scenario("Discovery without a recursive read grant is visible in readiness and blocked before any model call or automatic permission creation", async () => {
      const inputRoot = join(data, "ungranted-input"); await mkdir(inputRoot);
      const scopedWorkspace = await api("POST", "/api/workspaces", { name: "未授权资料夹具", root_path: inputRoot });
      const task = (await api("POST", "/api/automations", { name: "管理权限准备验收", workflow_graph_id: "", workspace_id: scopedWorkspace.id,
        target: { kind: "personal_assistant", goal: "MANAGEMENT_UNGRANTED: discovery must not start", allow_file_writes: false, allow_file_discovery: true, max_runs: 5 },
        timezone: "UTC", schedule: { kind: "daily", time: "09:00" }, enabled: false }, 201)).automation;
      const before = calls.length;
      await openDetail(task);
      const readiness = dialog().locator('section[aria-label="运行准备检查"]');
      await readiness.getByText(/目录递归读取授权/).waitFor();
      assert.equal(await dialog().getByRole("button", { name: "运行一轮", exact: true }).isEnabled(), false);
      assert.equal((await history(task.id)).occurrences.length, 0); assert.equal(calls.length, before);
      assert.equal((await state(task.id)).actions.can_start, false);
      await capture("product-readiness-permission.png"); await closeDetail();
    });

    await scenario("A bounded real invocation timeout is persisted as failure and requires explicit reviewed recovery rather than automatic replay", async () => {
      const settings = await api("GET", "/api/settings");
      try {
        await api("PUT", "/api/settings", { ...settings, model: { ...settings.model, invoke_timeout_ms: 1000 } });
        const task = await create("管理模型超时验收", "MANAGEMENT_TIMEOUT: controlled local provider does not respond", ws.id);
        await api("POST", startPath(task.id), { expected_revision: task.revision, request_id: randomUUID() });
        await waitStatus(task.id, "failed");
        await openDetail(task);
        const detail = await state(task.id); assert.equal(detail.actions.requires_review, true);
        assert.ok(detail.activity.latest.error || detail.conversation.run_error);
        assert.equal(await dialog().getByRole("button", { name: "核实后继续新一轮", exact: true }).isEnabled(), false);
        await capture("product-invocation-timeout.png");
        await closeDetail();
      } finally { await api("PUT", "/api/settings", settings); }
    });
    await scenario("Editor rejects invalid configuration, retains input after server validation, and makes changed directory authority visible with permissions reset", async () => {
      const newRoot = join(workspace, "explicit-other-workspace"); await mkdir(newRoot);
      const other = await api("POST", "/api/workspaces", { name: "管理范围切换夹具", root_path: newRoot });
      const task = (await api("GET", "/api/automations")).automations.find(item => item.name === "授权资料发现验收")
        || (await api("GET", "/api/automations")).automations.find(item => item.name === "本地个人助手验收");
      await list(task); await rowFor(task).locator("summary").click();
      await rowFor(task).getByRole("button", { name: `编辑 ${task.name}`, exact: true }).click();
      await dialog().getByLabel("工作空间上下文", { exact: true }).selectOption(other.id);
      assert.ok((await dialog().innerText()).includes(newRoot));
      assert.equal(await dialog().getByLabel("允许查找授权目录中的资料", { exact: false }).isChecked(), false);
      assert.equal(await dialog().getByLabel("允许在授权目录创建新文件", { exact: false }).isChecked(), false);
      await dialog().getByRole("button", { name: "取消", exact: true }).click();
      assert.equal((await getTask(task.id)).target.root_path, task.target.root_path);
      await page.getByRole("button", { name: "新建任务", exact: true }).first().click();
      await dialog().getByLabel("任务类型", { exact: true }).selectOption("personal_assistant");
      await dialog().getByLabel("持续跟进的目标", { exact: true }).fill("MANAGEMENT_VALIDATION: keep input on rejected configuration");
      await dialog().getByLabel("工作空间上下文", { exact: true }).selectOption(ws.id);
      const save = dialog().getByRole("button", { name: "保存任务", exact: true });
      assert.equal(await save.isEnabled(), false);
      await dialog().getByLabel("任务名称", { exact: true }).fill("管理配置保留验收");
      await dialog().getByLabel("IANA 时区", { exact: true }).fill("Invalid/Controlled");
      assert.equal(await save.isEnabled(), false);
      await dialog().getByLabel("IANA 时区", { exact: true }).fill("UTC");
      await dialog().getByLabel("委托最多执行轮次", { exact: true }).fill("21");
      assert.equal(await save.isEnabled(), false);
      await dialog().getByLabel("委托最多执行轮次", { exact: true }).fill("5");
      await dialog().getByLabel("重复方式", { exact: true }).selectOption("file_changed");
      await dialog().getByLabel("监听文件绝对路径", { exact: true }).fill("relative.txt");
      assert.equal(await save.isEnabled(), false);
      await dialog().getByLabel("监听文件绝对路径", { exact: true }).fill(join(workspace, "missing-controlled-input.txt"));
      setPhase("intentional_management_failure"); expectHttpFailure(`${base}/api/automations`);
      try {
        await save.click(); await dialog().getByRole("alert").waitFor();
        assert.equal(await dialog().getByLabel("任务名称", { exact: true }).inputValue(), "管理配置保留验收");
        assert.ok((await dialog().getByLabel("持续跟进的目标", { exact: true }).inputValue()).includes("MANAGEMENT_VALIDATION"));
        assert.equal((await api("GET", "/api/automations")).automations.some(item => item.name === "管理配置保留验收"), false);
        await capture("product-validation-retained-input.png");
      } finally { setPhase("normal"); await dialog().getByRole("button", { name: "取消", exact: true }).click(); }
    });
  } finally {
    for (const route of routes.splice(0)) await page.unroute(route.url, route.handler).catch(() => {});
    setPhase("normal");
    await page.setViewportSize({ width: 1366, height: 768 });
    if (await dialog().count()) await dialog().getByRole("button", { name: "关闭详情", exact: true }).click().catch(() => {});
    await page.goto(`${frontend}/automations`);
    await page.getByLabel("搜索任务", { exact: true }).fill("");
    await page.getByRole("button", { name: "全部", exact: true }).click();
  }
}
