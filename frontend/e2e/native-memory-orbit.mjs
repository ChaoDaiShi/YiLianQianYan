import assert from "node:assert/strict";

function installMeshRendererProbe() {
  const originalGetContext = HTMLCanvasElement.prototype.getContext;
  const instrumented = new WeakSet();
  const stats = {
    drawCalls: 0, triangleVertices: 0, depthDraws: 0, depthBits: 0,
    probe: false, waiting: false, disabled: false, pixelSample: null,
  };
  window.memoryMeshProbe = stats;
  HTMLCanvasElement.prototype.getContext = function (type, ...options) {
    if (stats.disabled && type.startsWith("webgl") && this.matches("[data-testid='memory-space-webgl']")) return null;
    const context = originalGetContext.call(this, type, ...options);
    if (!context || !type.startsWith("webgl") || !this.matches("[data-testid='memory-space-webgl']") || instrumented.has(context)) {
      return context;
    }
    instrumented.add(context);
    stats.version = context.getParameter(context.VERSION);
    stats.depthBits = context.getParameter(context.DEPTH_BITS);
    const methods = [["drawArrays", 2], ["drawElements", 1]];
    for (const [method, count] of methods) {
      const original = context[method].bind(context);
      context[method] = (...args) => {
        const result = original(...args);
        stats.drawCalls++;
        if (args[0] === context.TRIANGLES) stats.triangleVertices += args[count];
        if (context.isEnabled(context.DEPTH_TEST)) stats.depthDraws++;
        if (!stats.probe || stats.waiting) return result;
        stats.waiting = true;
        // The microtask samples the drawn framebuffer before its next presentation clears it.
        queueMicrotask(() => {
          const width = context.drawingBufferWidth;
          const height = context.drawingBufferHeight;
          const pixels = new Uint8Array(width * height * 4);
          context.readPixels(0, 0, width, height, context.RGBA, context.UNSIGNED_BYTE, pixels);
          const step = Math.max(2, Math.floor(Math.min(width, height) / 160));
          const colors = new Set();
          let covered = 0;
          let checksum = 2166136261;
          for (let y = 0; y < height; y += step) for (let x = 0; x < width; x += step) {
            const i = (x + y * width) * 4;
            const color = (pixels[i] << 24) | (pixels[i + 1] << 16) | (pixels[i + 2] << 8) | pixels[i + 3];
            if (pixels[i + 3] > 32 && color !== ((pixels[0] << 24) | (pixels[1] << 16) | (pixels[2] << 8) | pixels[3])) {
              covered++;
              colors.add(color);
            }
            checksum = Math.imul(checksum ^ color, 16777619);
          }
          stats.pixelSample = { covered, colors: colors.size, checksum: checksum >>> 0 };
          stats.probe = false;
          stats.waiting = false;
        });
        return result;
      };
    }
    return context;
  };
}

export async function verifyMemoryOrbit({ page, api, report, fixture, capture, appearance, openGraph, resizeNative, until, setStage }) {
  await page.addInitScript(installMeshRendererProbe);
  const memoryPath = "/api/memories";
  const graphPath = memoryPath + "/graph";
  const create = content => api("POST", memoryPath, { content, category: "note", source: "manual" });
  const edit = (entry, content) => api("PUT", `${memoryPath}/${entry.id}`, { content });
  const link = (source, target, label, status) => api("POST", memoryPath + "/relations", {
    source_memory_id: source.id, target_memory_id: target.id, label, status,
  });
  const button = name => page.getByRole("button", { name, exact: true });
  const graph = () => page.getByRole("region", { name: "记忆关系图", exact: true });
  const space = () => page.getByRole("region", { name: "记忆网络立体视图", exact: true });
  const active = id => space().locator(`[data-memory-id="${id}"][data-current="true"]`);
  const meshCanvas = () => space().getByTestId("memory-space-webgl");
  const graphState = () => api("GET", graphPath);
  const fitCores = async ({ samplePixels = false } = {}) => {
    if (samplePixels) await page.evaluate(() => { window.memoryMeshProbe.probe = true; window.memoryMeshProbe.pixelSample = null; });
    await button("适合画面").click();
    await until(() => space().locator("svg.memory-space-canvas").evaluate(svg => {
      const bounds = svg.getBoundingClientRect();
      const objects = [...svg.querySelectorAll('[data-current="true"][data-version-id] .memory-space-marker-dot')];
      return objects.length === 5 && objects.every(object => {
        const box = object.getBoundingClientRect();
        return box.left >= bounds.left && box.right <= bounds.right && box.top >= bounds.top && box.bottom <= bounds.bottom;
      });
    }), "fit keeps the five effective cores within the canvas");
    if (samplePixels) await until(() => page.evaluate(() => window.memoryMeshProbe.pixelSample?.covered > 0), "WebGL draws visible model pixels");
  };
  const assertMesh = async () => {
    await meshCanvas().waitFor();
    await until(async () => Number(await meshCanvas().getAttribute("data-mesh-frame")) > 0, "WebGL renderer submits at least one frame");
    const mesh = await meshCanvas().evaluate(canvas => ({
      frame: Number(canvas.dataset.meshFrame), triangles: Number(canvas.dataset.meshTriangles),
      spheres: Number(canvas.dataset.meshSpheres), tubes: Number(canvas.dataset.meshTubes),
      renderer: canvas.dataset.renderer, webgl: {...window.memoryMeshProbe},
    }));
    assert.equal(mesh.renderer, "webgl");
    assert.equal(mesh.spheres, 12, "only recorded versions have sphere meshes");
    assert.ok(mesh.tubes >= 11, "relationships and revision columns have tube meshes");
    assert.ok(mesh.triangles > 400 && mesh.webgl.triangleVertices > 0, "the native canvas submits tessellated geometry");
    assert.ok(mesh.webgl.depthBits > 0 && mesh.webgl.depthDraws > 0, "volume is drawn using the WebGL depth buffer");
    assert.ok(mesh.webgl.pixelSample?.covered > 10 && mesh.webgl.pixelSample?.colors > 8, "native framebuffer contains shaded model pixels");
    return mesh;
  };

  setStage("seed real memory records, independent versions and approved relationship");
  assert.equal((await graphState()).memories.length, 0);
  const project = await create("星桥发布 · 初版排期为周一");
  const check = await create("数据核验 · 初步核验字段");
  const checklist = await create("巡检清单 · 留存本次巡检记录");
  const site = await create("观测站点 · 记录水位");
  const observation = await create("观测记录 12 · 记录水位和天气");
  const primary = await link(project, check, "发布数据需核验");
  await link(project, site, "采用站点数据");
  await link(project, checklist, "依照巡检清单");
  await link(check, checklist, "核验记录对应");
  await link(site, observation, "包含观测记录");
  await edit(project, "星桥发布 · 修订排期至周三");
  await edit(project, "星桥发布 · 已确认周五发布");
  await edit(project, "星桥发布 · 增补对照材料与审批记录");
  await edit(check, "数据核验 · 已整理来源和字段");
  await edit(site, "观测站点 · 已补齐本周水位");
  const pending = await link(check, observation, "待核对记录", "pending");
  const beforeCorrection = await graphState();
  assert.deepEqual([project, check].map(({ id }) =>
    beforeCorrection.histories[id].find(version => version.version_id === beforeCorrection.current_version_ids[id]).layer), [3, 1]);
  await edit(project, "星桥发布 · 锁定周五排期，确认公开数据的校验范围");
  const correction = await api("POST", `${memoryPath}/relations/${primary.id}/correction-candidate`, {
    target_memory_id: check.id, content: "数据核验 · 已确认发布数据和校验范围",
    reason: "已核对发布排期与字段", expected_revision: 2, source_version_id: `${project.id}:5`,
  });
  assert.equal((await graphState()).histories[check.id].length, 2);
  await api("POST", `${memoryPath}/candidates/${correction.candidate_id}/accept`, {
    content: "数据核验 · 已确认发布数据和校验范围", category: "note", scope: "profile",
  });

  setStage("reindex isolated records with the loopback embedding fixture");
  const settings = await api("GET", "/api/settings");
  await api("PUT", "/api/settings", { ...settings, model: {
    ...settings.model, embedding_model: "qa-embedding", embedding_base_url: fixture.url,
    embedding_api_key_env: "YILIAN_MEMORY_SPACE_EMBEDDING_KEY",
  } });
  const reindex = await api("POST", memoryPath + "/reindex", { limit: 50 });
  assert.equal(reindex.succeeded, 5);
  assert.equal(fixture.count(), 5);
  const snapshot = await graphState();
  const entries = [project, check, checklist, site, observation];
  assert.deepEqual(entries.map(entry => snapshot.histories[entry.id].length), [5, 3, 1, 2, 1]);
  assert.ok(snapshot.change_events.some(event => event.version_id === `${check.id}:3` && event.related_version_id === `${project.id}:5`));
  assert.equal(snapshot.semantic.indexed_memory_ids.length, 5);
  assert.equal(snapshot.relations.filter(relation => relation.status === "confirmed").length, 5);

  setStage("light 2D curved edges without boundaries");
  await appearance("light");
  await openGraph();
  await until(async () => (await graph().locator(".react-flow__node").count()) === 5, "five actual memories render in the 2D canvas");
  assert.equal(await graph().locator(".memory-orbit-plane").count(), 0);
  assert.equal(await graph().locator(".memory-network").evaluate(element => getComputedStyle(element).borderWidth), "0px");
  const flatRevisions = await Promise.all(entries.map(async entry => (
    await graph().locator(`.react-flow__node[data-id="memory:${entry.id}"] .memory-star-core`).textContent()
  )?.trim()));
  assert.deepEqual(flatRevisions, ["V5", "V3", "V1", "V2", "V1"]);
  assert.equal(await graph().locator(`.react-flow__edge[data-id="${pending.id}"]`).count(), 0);
  await until(async () => (await graph().locator(".react-flow__edge-path").count()) === 5, "all five supported edge paths render");
  const flatPaths = await graph().locator(".react-flow__edge-path").evaluateAll(edges => edges.map(edge => edge.getAttribute("d")));
  assert.ok(flatPaths.every(path => /C/i.test(path)), "each visible 2D relation uses a cubic curve");
  await capture("01-light-curved-memory-network.png");
  await page.evaluate(() => {
    const frames = [];
    window.memorySpaceRevealProbe = frames;
    const trace = new MutationObserver(() => {
      const space = document.querySelector(".memory-space-workspace");
      if (!space) return;
      const { spaceProgress, spaceBase, spaceGrowth, spacePhase } = space.dataset;
      if (frames.at(-1)?.progress === Number(spaceProgress)) return;
      const link = space.querySelector("[data-base-relationship='true']");
      frames.push({ progress: Number(spaceProgress), base: Number(spaceBase), growth: Number(spaceGrowth),
        baseRelationshipOpacity: Number(link?.getAttribute("opacity")) });
      if (spacePhase === "active") trace.disconnect();
    });
    trace.observe(document.body, { subtree: true, childList: true, attributes: true,
      attributeFilter: ["data-space-progress", "data-space-phase"] });
  });
  await button("立体查看").click();
  await until(() => space().getAttribute("data-space-phase").then(phase => phase === "active"), "3D entry complete");

  setStage("light 3D base network and independent version geometry");
  assert.equal(await space().locator("[data-version-id]").count(), 12);
  assert.equal(await space().locator("[data-memory-base='true']").count(), 5);
  assert.equal(await space().locator("[data-semantic-anchor]").count(), 0);
  assert.ok(await space().locator("[data-base-relationship]").count() >= 5);
  const revealFrames = await page.evaluate(() => window.memorySpaceRevealProbe);
  assert.ok(revealFrames.some(frame => frame.growth === 0 && frame.baseRelationshipOpacity > .5), "visible historical base before any upward growth");
  assert.ok(revealFrames.some(frame => frame.growth > 0 && frame.growth < 1), "animated real version heights between start and end");
  report.revealFrames = revealFrames;
  for (const entry of entries) {
    assert.equal(await active(entry.id).getAttribute("data-version-id"), snapshot.current_version_ids[entry.id]);
  }
  assert.equal(await space().locator("[data-memory-plane]").count(), 0);
  assert.equal(await space().locator("svg.memory-space-canvas polygon").count(), 0);
  assert.equal(await space().locator("[data-event-edge]").count(), 1);
  const spatialPaths = await space().locator("[data-edge-id] path").evaluateAll(edges => edges.map(edge => edge.getAttribute("d")));
  assert.ok(spatialPaths.length >= 6 && spatialPaths.every(path => /[CQ]/i.test(path) || /L/i.test(path) && path.split(/L/i).length > 6),
    "all 3D relations expose smooth sampled or Bézier interaction paths");
  const column = await space().locator(`[data-memory-id="${project.id}"][data-version-id]`).evaluateAll(items => items.map(item => ({
    x:item.getAttribute("data-world-x"), y:item.getAttribute("data-world-y"), layer:Number(item.getAttribute("data-layer")),
  })));
  assert.deepEqual(column.map(point => point.layer).sort((a,b) => a-b), [0,1,2,3,4]);
  assert.equal(await active(project.id).getAttribute("data-layer"), "4");
  assert.equal(await active(check.id).getAttribute("data-layer"), "2");
  assert.equal(new Set(column.map(point => `${point.x},${point.y}`)).size, 1, "history retains one shared XY anchor");
  const currentRevisions = await Promise.all(entries.map(async entry =>
    (await active(entry.id).getAttribute("aria-label"))?.match(/\bV\d+\b/)?.[0]));
  assert.deepEqual(currentRevisions, flatRevisions);
  await fitCores({ samplePixels: true });
  const history = space().locator(`[data-edge-id="${primary.id}:1"]`);
  await history.waitFor();
  const orientation = await space().locator(`[data-memory-id="${project.id}"]`).evaluateAll(nodes =>
    nodes.map(node => ({layer:Number(node.getAttribute("data-layer")),y:Number(node.getAttribute("data-screen-y"))}))).then(nodes=>nodes.sort((a,b)=>a.layer-b.layer));
  assert.ok(orientation.slice(1).every((point, i) => point.y < orientation[i].y), "new versions appear above their own older versions");
  await history.focus();
  await history.press("Enter");
  const oldEvidence = page.getByRole("dialog",{name:"历史关系依据"});
  await oldEvidence.getByText("星桥发布 · 初版排期为周一",{exact:true}).waitFor();
  await oldEvidence.getByText("数据核验 · 初步核验字段",{exact:true}).waitFor();
  await capture("02a-historical-relationship-evidence.png");
  await page.keyboard.press("Escape");
  await oldEvidence.waitFor({state:"hidden"});
  const lightMesh = await assertMesh();
  await capture("02-light-volumetric-memory.png");

  const canvas = space().locator("svg.memory-space-canvas");
  const box = await canvas.boundingBox();
  assert.ok(box);
  const beforeOrbit = await space().getAttribute("data-camera-azimuth");
  const x = box.x + box.width * .33, y = box.y + box.height * .56;
  await page.evaluate(() => { window.memoryMeshProbe.probe = true; window.memoryMeshProbe.pixelSample = null; });
  await page.mouse.move(x, y);
  await page.mouse.down({button: "middle"});
  await page.mouse.move(x + 44, y - 12, {steps: 6});
  await page.mouse.up({button: "middle"});
  await until(async () => (await space().getAttribute("data-camera-azimuth")) !== beforeOrbit, "middle button rotates the camera");
  await until(() => page.evaluate(old => window.memoryMeshProbe.pixelSample?.checksum && window.memoryMeshProbe.pixelSample.checksum !== old,
    lightMesh.webgl.pixelSample.checksum), "rotating the camera changes the drawn model pixels");
  assert.ok(Number(await meshCanvas().getAttribute("data-mesh-frame")) > lightMesh.frame);
  await capture("02b-light-volumetric-rotated.png");
  await button("返回二维").click();
  await space().waitFor({state:"detached"});

  setStage("dark native window in wide and narrow modes");
  await appearance("dark");
  await openGraph();
  await button("立体查看").click();
  await until(() => space().getAttribute("data-space-phase").then(phase => phase === "active"), "dark 3D entry complete");
  await fitCores({ samplePixels: true });
  await capture("03a-dark-volumetric-memory.png");
  await button("返回二维").click();
  await space().waitFor({state:"detached"});
  await resizeNative(900, 650);
  await page.evaluate(() => {
    window.narrowTransitionBounds = [];
    const observer = new MutationObserver(() => {
      const root = document.querySelector(".memory-space-workspace");
      if (!root) return;
      const {spacePhase, spaceProgress, spaceBase, spaceGrowth} = root.dataset;
      if (Number(spaceBase) <= 0 || window.narrowTransitionBounds.at(-1)?.progress === Number(spaceProgress)) return;
      const svg = root.querySelector(".memory-space-canvas");
      const stage = svg.getBoundingClientRect();
      const caption = root.querySelector(".memory-space-caption").getBoundingClientRect();
      const geometry = [...svg.querySelectorAll("[data-edge-id] path, [data-version-id] .memory-space-marker-dot")];
      let margin = Infinity;
      for (const element of geometry) {
        if (!(Number(element.parentElement.getAttribute("opacity")) > .015)) continue;
        const rect = element.getBoundingClientRect();
        margin = Math.min(margin, rect.left-stage.left, stage.right-rect.right, rect.top-stage.top, caption.top-rect.bottom);
      }
      window.narrowTransitionBounds.push({progress:Number(spaceProgress), growth:Number(spaceGrowth), margin});
      if (spacePhase === "active") observer.disconnect();
    });
    observer.observe(document.body,{subtree:true,attributes:true,attributeFilter:["data-space-progress","data-space-phase"]});
  });
  await page.evaluate(() => { window.memoryMeshProbe.probe = true; window.memoryMeshProbe.pixelSample = null; });
  await button("立体查看").click();
  await until(() => space().getAttribute("data-space-phase").then(value => value === "active"), "narrow 3D entry completes without manual fit");
  await until(() => page.evaluate(() => window.memoryMeshProbe.pixelSample?.covered > 0), "the default narrow camera draws WebGL pixels");
  const narrowMesh = await assertMesh();
  assert.equal(await space().locator("[data-version-id]").count(), 12);
  assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1), true);
  await capture("03-dark-narrow-volumetric-memory.png");
  const bounds = await page.evaluate(() => window.narrowTransitionBounds);
  report.narrowTransitionBounds = bounds;
  assert.ok(bounds.some(frame => frame.growth === 0) && bounds.some(frame => frame.growth > 0 && frame.growth < 1),
    "sample both the visible base and the upward growth during the narrow transition");
  const worst = bounds.reduce((a, b) => a.margin < b.margin ? a : b);
  assert.ok(worst.margin >= 0, `visible paths and version cores stay inside the narrow canvas while unfolding: ${JSON.stringify(worst)}`);
  const composition = await space().evaluate((root, memoryId) => {
    const stage = root.querySelector(".memory-space-stage");
    const height = stage.clientHeight - root.querySelector(".memory-space-caption").offsetHeight;
    const coordinates = selector => [...root.querySelectorAll(selector)].map(node =>
      [Number(node.dataset.screenX), Number(node.dataset.screenY)]);
    const base = coordinates("[data-memory-base='true']");
    const current = coordinates("[data-current='true'][data-version-id]");
    const column = coordinates(`[data-memory-id="${memoryId}"][data-version-id]`);
    const span = (points, axis) => Math.max(...points.map(point => point[axis])) - Math.min(...points.map(point => point[axis]));
    const nearest = points => Math.min(...points.flatMap(([x, y], i) => points.slice(i + 1).map(([a, b]) => Math.hypot(x - a, y - b))));
    const separation = nearest(base);
    return { baseWidth: span(base, 0), baseDepth: span(base, 1), tallestRise: span(column, 1), separation,
      currentSeparation: nearest(current), availableWidth: stage.clientWidth, availableHeight: height };
  }, project.id);
  report.narrowComposition = composition;
  assert.ok(composition.baseWidth >= composition.availableWidth * .40, JSON.stringify(composition));
  assert.ok(composition.tallestRise >= composition.availableHeight * .48 && composition.separation >= 30, JSON.stringify(composition));
  assert.ok(composition.currentSeparation >= 28, JSON.stringify(composition));

  setStage("narrow middle-button orbit directly from the default view");
  const narrowCanvas = space().locator("svg.memory-space-canvas");
  const narrowBox = await narrowCanvas.boundingBox();
  assert.ok(narrowBox);
  const origin = {x:narrowBox.x + narrowBox.width * .42, y:narrowBox.y + narrowBox.height * .53};
  const cameraKey = () => space().evaluate(root => ({
    azimuth:Number(root.dataset.cameraAzimuth), elevation:Number(root.dataset.cameraElevation), zoom:Number(root.dataset.cameraZoom),
  }));
  const defaultCamera = await cameraKey();
  await narrowCanvas.evaluate(svg => {
    const root = svg.closest(".memory-space-workspace");
    window.narrowOrbitBounds = [];
    const observer = new MutationObserver(() => {
      const {cameraAzimuth, cameraElevation, cameraZoom, orbiting} = root.dataset;
      if (orbiting !== "true") return;
      const stage = svg.getBoundingClientRect();
      const footer = root.querySelector(".memory-space-caption").getBoundingClientRect().top;
      let margin = Infinity;
      for (const element of svg.querySelectorAll("[data-edge-id] path, [data-version-id] .memory-space-marker-dot, .memory-space-current-ring")) {
        const box = element.getBoundingClientRect();
        margin = Math.min(margin, box.left-stage.left, stage.right-box.right, box.top-stage.top, footer-box.bottom);
      }
      const current = [...svg.querySelectorAll("[data-version-id][data-current='true']")];
      const labels = [...svg.querySelectorAll("[data-label-for]")];
      let largestTitleGap = 0;
      const titleOffsets = Object.fromEntries(current.flatMap(marker => {
        const label = labels.find(element => element.getAttribute("data-label-for") === marker.dataset.versionId);
        if (!label) return [];
        const box = label.querySelector("rect").getBoundingClientRect();
        const x = stage.left + Number(marker.dataset.screenX), y = stage.top + Number(marker.dataset.screenY);
        largestTitleGap = Math.max(largestTitleGap, Math.max(box.left-x,x-box.right,box.top-y,y-box.bottom));
        return [[marker.dataset.versionId, [box.left-stage.left-Number(marker.dataset.screenX), box.top-stage.top-Number(marker.dataset.screenY)]]];
      }));
      const labelBoxes = labels.map(label => label.querySelector("rect").getBoundingClientRect());
      let crossings = 0;
      for (const path of svg.querySelectorAll("[data-base-relationship='true'] path")) {
        const length = path.getTotalLength();
        const screen = path.getScreenCTM();
        for (let i = 1; i < 48; i++) {
          const point = path.getPointAtLength(length * i / 48).matrixTransform(screen);
          if (labelBoxes.some(box => point.x > box.left && point.x < box.right && point.y > box.top && point.y < box.bottom)) {
            crossings++;
            break;
          }
        }
      }
      window.narrowOrbitBounds.push({azimuth:Number(cameraAzimuth), elevation:Number(cameraElevation), zoom:Number(cameraZoom), margin,
        titleOffsets, missingTitles:current.length-Object.keys(titleOffsets).length, crossings, largestTitleGap});
    });
    observer.observe(root, {attributes:true,attributeFilter:["data-camera-azimuth","data-camera-elevation"]});
    window.stopNarrowOrbitProbe = () => observer.disconnect();
  });
  await page.mouse.move(origin.x, origin.y);
  await page.mouse.down({button:"middle"});
  for (let step = 1; step <= 32; step++) {
    await page.mouse.move(origin.x - 96 * step / 32, origin.y + 16 * step / 32);
    await page.waitForTimeout(12);
  }
  await page.mouse.up({button:"middle"});
  const rotatedCamera = await cameraKey();
  assert.notEqual(rotatedCamera.azimuth, defaultCamera.azimuth);
  assert.notEqual(rotatedCamera.elevation, defaultCamera.elevation);
  await capture("03b-dark-narrow-direct-orbit.png");
  await page.mouse.down({button:"middle"});
  for (let step = 1; step <= 32; step++) {
    await page.mouse.move(origin.x - 96 * (1-step/32),origin.y + 16 * (1-step/32));
    await page.waitForTimeout(12);
  }
  await page.mouse.up({button:"middle"});
  const orbitFrames = await page.evaluate(() => { window.stopNarrowOrbitProbe(); return window.narrowOrbitBounds; });
  report.narrowOrbitBounds = orbitFrames;
  assert.ok(orbitFrames.length > 14, "sample the intermediate native camera poses while orbiting");
  const worstOrbit = orbitFrames.reduce((a,b) => a.margin < b.margin ? a : b);
  assert.ok(worstOrbit.margin >= 0, `visible paths and version cores stay within the canvas during orbit: ${JSON.stringify(worstOrbit)}`);
  const zoomFloor = Math.min(...orbitFrames.map(frame => frame.zoom)) / defaultCamera.zoom;
  const stableFrames = orbitFrames.filter(frame => Math.abs(frame.zoom-defaultCamera.zoom) < .0002).length;
  const zoomStep = Math.max(...orbitFrames.map((frame,i) => i === 0 ? 0 : Math.abs(frame.zoom-orbitFrames[i-1].zoom))) / defaultCamera.zoom;
  report.narrowOrbitFraming = {zoomFloor,stableFrames,zoomStep,defaultZoom:defaultCamera.zoom};
  assert.ok(zoomFloor >= .875 && stableFrames >= 16 && zoomStep < .025, JSON.stringify(report.narrowOrbitFraming));
  const steps = orbitFrames.flatMap((frame, index) => {
    const prior = orbitFrames[index-1]?.titleOffsets ?? {};
    return Object.entries(frame.titleOffsets).flatMap(([id, [x,y]]) => prior[id] ? [Math.hypot(x-prior[id][0],y-prior[id][1])] : []);
  });
  const labelMotion = {
    missingTitles:orbitFrames.reduce((sum,frame) => sum+frame.missingTitles,0),
    lineCrossings:orbitFrames.reduce((sum,frame) => sum+frame.crossings,0),
    largestRelativeStep:Math.max(0,...steps),
    largestTitleGap:Math.max(...orbitFrames.map(frame => frame.largestTitleGap)),
  };
  report.narrowOrbitLabels = labelMotion;
  assert.ok(labelMotion.missingTitles === 0 && labelMotion.lineCrossings === 0 && labelMotion.largestRelativeStep < 24 && labelMotion.largestTitleGap < 42,
    `current titles remain attached and clear of base relationships: ${JSON.stringify(labelMotion)}`);
  const returned = await cameraKey();
  const turnDifference = returned.azimuth - defaultCamera.azimuth;
  assert.ok(Math.abs(Math.atan2(Math.sin(turnDifference), Math.cos(turnDifference))) < .005 &&
    Math.abs(returned.elevation-defaultCamera.elevation) < .005);
  assert.ok(Math.abs(returned.zoom-defaultCamera.zoom) < .005, "reversing orbit restores the original framing scale");

  setStage("explicit 2D fallback when WebGL is unavailable");
  await button("返回二维").click();
  await space().waitFor({state:"detached"});
  await page.evaluate(() => { window.memoryMeshProbe.disabled = true; });
  await button("立体查看").click();
  await space().getByText("当前环境无法绘制三维模型，可返回二维查看记忆。").waitFor();
  assert.equal(await meshCanvas().getAttribute("data-mesh-frame"), null);
  await button("返回二维").click();
  await space().waitFor({state:"detached"});
  await graph().locator(`.react-flow__node[data-id="memory:${project.id}"] .memory-star-core`).waitFor();
  const after = await graphState();
  assert.deepEqual(after.current_version_ids, snapshot.current_version_ids);
  assert.deepEqual(after.histories, snapshot.histories);
  assert.deepEqual(after.relations, snapshot.relations);
  report.scenario = "borderless curved 2D and solid WebGL 3D";
  report.mesh = { light: lightMesh, darkNarrow: narrowMesh };
  report.noWebgl = { simulatedUnavailable: true, returnedToUsable2d: true };
  report.counts = { memories: 5, versions: 12, indexed: 5, confirmedEdges: 5, reviewedEventEdges: 1 };
  report.checks.push("真实 SQLite 记忆、独立版本、已确认与待确认关系", "二维/三维实际生效版本相同", "二维与三维去掉参考轮廓并使用曲线", "实际 WebGL 球体与连管提交了深度测试和有光照差异的像素", "基准网络早于逐步抬升的版本出现，索引不会增加图节点", "原生中键旋转改变画面，切换不写入记忆", "深色窄窗无横向溢出", "模拟禁用 WebGL 后能返回正常二维图");
}
