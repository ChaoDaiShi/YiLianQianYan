import assert from "node:assert/strict";
import { writeFile } from "node:fs/promises";
import { join } from "node:path";

export async function canvasStability({ page, backendPort, frontendPort, controlToken, evidenceDir }) {
  const graphId = "canvas-stability";
  const path = `/api/task-world/graphs/${graphId}`;
  const records = [];
  async function api(method, endpoint, body) {
    const response = await fetch(`http://127.0.0.1:${backendPort}${endpoint}`, {
      method, headers: { "Content-Type": "application/json", "X-Yilian-Control-Session": controlToken },
      body: body === undefined ? undefined : JSON.stringify(body),
    });
    const result = await response.json();
    assert.ok(response.ok, `${method} ${endpoint}: ${response.status} ${JSON.stringify(result)}`);
    return result;
  }
  async function until(read, predicate, label) {
    const deadline = Date.now() + 15_000;
    let value;
    do {
      value = await read();
      if (predicate(value)) return value;
      await new Promise((resolve) => setTimeout(resolve, 100));
    } while (Date.now() < deadline);
    throw new Error(`${label}: ${JSON.stringify(value)}`);
  }
  const node = (id) => page.locator(`.react-flow__node[data-id="${id}"]`);
  const camera = () => page.locator(".react-flow__viewport").evaluate((el) => el.style.transform);
  const transform = (id) => node(id).evaluate((el) => el.style.transform);
  const trail = () => page.locator(".task-world-trail-item").evaluateAll((els) => els.map((el) => el.dataset.nodeId));
  const document = async () => (await api("GET", `${path}/canvas-view`)).view;
  const detail = async () => (await api("GET", `${path}/detail`)).detail;
  async function saveDocument(update) {
    const current = await document();
    return api("PUT", `${path}/canvas-view`, { ...update(current), expected_view_revision: current.view_revision });
  }
  async function snapshot(label) {
    const record = { label, viewport: await camera(), positions: {}, order: await trail() };
    for (const id of ["A", "B", "C"]) if (await node(id).count()) record.positions[id] = await transform(id);
    records.push(record);
    await writeFile(join(evidenceDir, "canvas-snapshots.json"), JSON.stringify(records, null, 2));
    return record;
  }
  async function editA(title) {
    const current = await detail();
    await api("PUT", `${path}/nodes/A`, { expected_revision: current.revision, ...definitions[0], title });
    await node("A").locator(".task-world-node-title").filter({ hasText: title }).waitFor();
  }

  const workflow = await api("POST", "/api/workflow-graphs", {
    name: "Canvas stability local output", definition: {
      schema_version: 1, entry_node_id: "out",
      nodes: [{ id: "out", kind: "output", config: { type: "output", template: "Canvas stability local result" } }], edges: [],
    },
  });
  assert.ok(workflow.id, "Real workflow graph must have an id");
  const definitions = ["A", "B", "C"].map((id) => ({
    id, kind: "work", title: id, input: { instruction: `Task ${id}`, executor_ref: `workflow://${workflow.id}` }, retry_policy: { max_attempts: 2 },
  }));
  await api("POST", "/api/task-world/graphs", { id: graphId, nodes: definitions, edges: [] });
  const savedViewport = { x: 340, y: -120, zoom: 1.35 };
  await saveDocument((view) => ({ ...view, viewport: savedViewport, node_layouts: definitions.map((n, index) => ({ node_id: n.id, x: 40 + index * 320, y: 140, width: 240, height: 128 })) }));
  await page.setViewportSize({ width: 1920, height: 1080 });
  await page.goto(`http://127.0.0.1:${frontendPort}/task-world/${graphId}`, { waitUntil: "domcontentloaded" });
  await node("A").waitFor();
  assert.equal(await camera(), "translate(340px, -120px) scale(1.35)");
  const initial = await snapshot("saved viewport hydrated exactly");
  await editA("A semantic refresh");
  assert.equal(await transform("A"), initial.positions.A);
  assert.equal(await camera(), initial.viewport);

  await page.getByRole("button", { name: "适应全部", exact: true }).click();
  await until(camera, (v) => v !== initial.viewport, "explicit Fit All must move camera");
  await snapshot("explicit fit all");

  let held = false;
  let release;
  const releaseSave = new Promise((resolve) => { release = resolve; });
  let pending;
  const pendingSave = new Promise((resolve) => { pending = resolve; });
  let staleResponses = 0;
  const staleListener = async (response) => {
    if (response.url().endsWith(`${path}/canvas-view`) && response.status() === 409) {
      const body = await response.json();
      if (body.code === "stale_view_revision" || body.error === "stale_view_revision") staleResponses++;
    }
  };
  page.on("response", staleListener);
  await page.route(`**${path}/canvas-view`, async (route) => {
    const request = route.request();
    const body = request.method() === "PUT" ? request.postDataJSON() : null;
    const a = body?.node_layouts.find((n) => n.node_id === "A");
    if (!held && a && Math.abs(a.x - 40) > 1) {
      held = true;
      pending();
      await releaseSave;
    }
    await route.continue();
  });
  const aBox = await node("A").boundingBox();
  assert.ok(aBox);
  await page.mouse.move(aBox.x + aBox.width / 2, aBox.y + aBox.height / 2);
  await page.mouse.down();
  await page.mouse.move(aBox.x + aBox.width / 2 + 80, aBox.y + aBox.height / 2 + 65, { steps: 12 });
  await page.mouse.up();
  await Promise.race([pendingSave, new Promise((_, reject) => setTimeout(() => reject(new Error("Drag did not submit layout")), 10_000))]);
  const dragged = await snapshot("drag optimistic while save is held");
  assert.notEqual(dragged.positions.A, initial.positions.A);
  await editA("A changed during pending drag save");
  assert.equal(await transform("A"), dragged.positions.A);
  assert.equal(await camera(), dragged.viewport);
  // Create a real server revision conflict while the browser still holds the
  // original PUT. No response is mocked; its real 409 must trigger a rebase.
  await saveDocument((view) => ({ ...view, viewport: { x: 12, y: 34, zoom: 0.9 } }));
  assert.equal(await transform("A"), dragged.positions.A);
  assert.equal(await camera(), dragged.viewport);
  release();
  const persisted = await until(document, (view) => view.node_layouts.find((n) => n.node_id === "A")?.x !== 40, "drag rebase must persist local position");
  await until(async () => staleResponses, (count) => count === 1, "real stale response must be observed");
  assert.equal(await transform("A"), dragged.positions.A);
  await page.unroute(`**${path}/canvas-view`);
  await snapshot("stale rebase saved without visual rollback");

  // Pan and zoom through real input, then select from the trail. No test-only
  // ReactFlow hooks or production globals are involved.
  const canvas = await page.locator(".react-flow").boundingBox();
  assert.ok(canvas);
  await page.mouse.move(canvas.x + canvas.width / 2, canvas.y + canvas.height - 100);
  await page.mouse.down();
  await page.mouse.move(canvas.x + canvas.width / 2 + 60, canvas.y + canvas.height - 150, { steps: 8 });
  await page.mouse.up();
  await page.getByRole("button", { name: "Zoom In", exact: true }).click();
  await until(document, (view) => view.viewport.x !== persisted.viewport.x && view.viewport.zoom !== persisted.viewport.zoom, "pan and zoom must save");
  // ReactFlow control zoom has a short transition. Wait for the saved viewport
  // to match the currently rendered transform rather than sleeping blindly.
  await until(async () => ({ view: await document(), camera: await camera() }), ({ view, camera }) => {
    const values = camera.match(/-?[\d.]+/g)?.map(Number);
    return values?.length === 3 && [view.viewport.x, view.viewport.y, view.viewport.zoom].every((value, index) => Math.abs(value - values[index]) < 0.001);
  }, "camera move must settle and persist");
  const userCamera = await camera();
  const aPosition = await transform("A");
  if (await page.getByRole("button", {name:"执行轨迹",exact:true}).count()) await page.getByRole("button", {name:"执行轨迹",exact:true}).click();
  await page.locator('.task-world-trail-item[data-node-id="B"] > button').first().click();
  await until(document, (v) => v.selection.includes("B"), "selection must save");
  assert.equal(await camera(), userCamera);
  assert.equal(await transform("A"), aPosition);
  const beforeExecution = await snapshot("user pan zoom and B selected");
  const cardSize = await node("B").locator(".task-world-node").evaluate((el) => ({ width: el.offsetWidth, height: el.offsetHeight }));
  const current = await detail();
  await api("POST", `${path}/nodes/B/executions`, { expected_revision: current.revision });
  await node("B").locator('[data-execution-status="succeeded"]').waitFor({ timeout: 15_000 });
  assert.equal(await camera(), userCamera);
  assert.equal(await transform("A"), aPosition);
  assert.equal(await transform("B"), beforeExecution.positions.B);
  assert.deepEqual(await trail(), ["A", "B", "C"]);
  assert.deepEqual(await node("B").locator(".task-world-node").evaluate((el) => ({ width: el.offsetWidth, height: el.offsetHeight })), cardSize);
  assert.deepEqual(cardSize, { width: 240, height: 128 });
  await snapshot("real output workflow succeeded with stable nodes viewport and trail");

  await editA("A after execution");
  assert.equal(await camera(), userCamera);
  await page.setViewportSize({ width: 1800, height: 1000 });
  assert.equal(await camera(), userCamera);
  await page.getByRole("button", { name: "任务中心", exact: true }).click();
  await page.goto(`http://127.0.0.1:${frontendPort}/task-world/${graphId}`, { waitUntil: "domcontentloaded" });
  await node("A").waitFor();
  assert.equal(await camera(), userCamera);
  assert.equal(await transform("A"), aPosition);
  await snapshot("remount restored saved viewport and drag");
  if (await page.getByRole("button", {name:"执行轨迹",exact:true}).count()) await page.getByRole("button", {name:"执行轨迹",exact:true}).click();
  await page.getByRole("button", { name: "定位节点：C", exact: true }).click();
  await until(camera, (v) => v !== userCamera, "explicit Locate must move camera");
  const locatedCamera = await camera();
  await snapshot("explicit locate moved camera");
  await page.getByRole("button", { name: "自动布局", exact: true }).click();
  await until(() => transform("A"), (v) => v !== aPosition, "explicit Auto Layout must move nodes");
  assert.equal(await camera(), locatedCamera);
  await until(document, (v) => v.node_layouts.find((n) => n.node_id === "A")?.y === 40, "auto layout must save");
  const layout = await snapshot("explicit auto layout preserved camera");
  await editA("A refreshed after explicit layout");
  assert.equal(await transform("A"), layout.positions.A);
  assert.equal(await camera(), locatedCamera);
  await page.getByRole("button", { name: "100%", exact: true }).click();
  await until(camera, (v) => v.endsWith("scale(1)"), "100% control");
  await page.getByRole("button", { name: "适应全部", exact: true }).click();
  const beforeGroups = await snapshot("before group interactions");
  await page.locator('.task-world-trail-item[data-node-id="B"] > button').first().click();
  await page.keyboard.down("Control");
  await node("C").click();
  await page.keyboard.up("Control");
  await until(document, (v) => v.selection.includes("B") && v.selection.includes("C"), "multi-select must remain available");
  assert.equal(await camera(), beforeGroups.viewport);
  if (await page.getByRole("button", {name:"更多",exact:true}).count()) await page.getByRole("button", {name:"更多",exact:true}).click();
  await page.getByRole("button", { name: "创建分组", exact: true }).click();
  await page.getByLabel("画布视图工具").getByRole("button", { name: "折叠", exact: true }).click();
  await node("B").waitFor({ state: "detached" });
  await node("C").waitFor({ state: "detached" });
  assert.equal(await transform("A"), beforeGroups.positions.A);
  assert.equal(await camera(), beforeGroups.viewport);
  await page.getByLabel("画布视图工具").getByRole("button", { name: "展开", exact: true }).click();
  await node("B").waitFor();
  for (const id of ["A", "B", "C"]) assert.equal(await transform(id), beforeGroups.positions[id]);
  assert.equal(await camera(), beforeGroups.viewport);
  await snapshot("group collapse expand retained positions and camera");

  const beforeAdd = await detail();
  await api("POST", `${path}/nodes`, { expected_revision: beforeAdd.revision, node: { ...definitions[0], id: "D", title: "D" } });
  await node("D").waitFor();
  for (const id of ["A", "B", "C"]) assert.equal(await transform(id), beforeGroups.positions[id]);
  assert.equal(await camera(), beforeGroups.viewport);
  // Keyboard focus and selection must not trigger ReactFlow's default auto-pan.
  await node("D").focus();
  await page.keyboard.press("Enter");
  assert.equal(await camera(), beforeGroups.viewport);
  await until(document, (v) => v.selection.includes("D"), "keyboard selection must persist");
  const deletion = page.waitForResponse((response) => response.request().method() === "DELETE" && response.url().endsWith(`${path}/nodes/D`), { timeout: 10_000 });
  await page.keyboard.down("Delete");
  try {
    const response = await deletion;
    assert.ok(response.ok(), `Keyboard Delete returned ${response.status()}`);
  } finally {
    await page.keyboard.up("Delete");
  }
  await node("D").waitFor({ state: "detached" });
  for (const id of ["A", "B", "C"]) assert.equal(await transform(id), beforeGroups.positions[id]);
  assert.equal(await camera(), beforeGroups.viewport);
  await snapshot("new node placement keyboard selection and Delete left old nodes stable");
  await page.screenshot({ path: join(evidenceDir, "canvas-stability.png"), fullPage: true });
  page.off("response", staleListener);
  return { status: "passed", graph_id: graphId, nodes: 3, snapshots: records.length, stale_responses: staleResponses, real_execution: "succeeded", initial_viewport: savedViewport };
}
