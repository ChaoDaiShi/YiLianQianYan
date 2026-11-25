import assert from "node:assert/strict";
import { writeFile } from "node:fs/promises";
import { join } from "node:path";

export async function verifyNativeMemoryText({page,appUrl,requireApi,evidence,appInfo}) {
  const text = "验收样本：星桥发布项目依赖接口验收。林晓偏好无衬线字体。接口验收没有依赖月度开票。";
  const memory = await requireApi("POST", "/api/memories", {content:text,category:"note",source:"manual"});
  const graph = () => requireApi("GET", "/api/memories/graph");
  const button = name => page.getByRole("button",{name,exact:true});
  await page.goto(`${appUrl}/memory`);
  await page.getByRole("button", {name:/验收样本.*星桥发布项目/}).click();
  const started = performance.now();
  await button("从此记忆提取关系").click();
  await page.getByRole("tab",{name:"待审阅",exact:true,selected:true}).waitFor({timeout:90000});
  await page.getByText("AI 提议 · 项目关系", { exact:true }).first().waitFor({timeout:30000});
  await page.getByText("AI 提议 · 偏好关系", { exact:true }).first().waitFor({timeout:30000});
  const durationMs = Math.round(performance.now()-started);
  const candidates = (await requireApi("GET","/api/memories/candidates")).entries.filter(item => item.source_memory_id === memory.id);
  assert.ok(candidates.length >= 2, `real provider proposed at least two source-backed relationships; found ${candidates.map(item => item.relationship?.kind).join(", ") || "none"}`);
  assert.ok(candidates.every(item => item.source_kind === "memory_version" && item.source_memory_revision === 1 && 
    item.relationship.certainty === "inferred" && text.includes(item.source_excerpt) && !item.content.includes("月度开票")));
  const project = candidates.find(item => item.relationship.kind === "project");
  const preference = candidates.find(item => item.relationship.kind === "preference");
  assert.ok(project && preference, "project and preference were independently extracted");
  assert.equal((await graph()).extracted_relations.length,0);
  await page.screenshot({path:join(evidence,"01-text-candidates-full.png"),animations:"disabled"});
  for (const candidate of [project,preference]) {
    const action = page.getByRole("button").filter({hasText:candidate.content});
    await action.click();
    const dialog = page.getByRole("dialog",{name:"审阅记忆候选"});
    await dialog.getByRole("link",{name:/查阅来源记忆/}).waitFor();
    if (candidate.id === project.id) await dialog.getByLabel("关系依据",{exact:true}).selectOption("confirmed");
    await dialog.getByRole("button",{name:"确认并保存"}).click();
    await dialog.waitFor({state:"hidden"});
  }
  const reviewed = await graph();
  assert.equal(reviewed.memories.length,1,"review creates no duplicate memory");
  assert.equal(reviewed.histories[memory.id].length,1,"relationship review does not bump memory version");
  assert.equal(reviewed.extracted_relations.length,2);
  await button("查看关系图").click();
  await page.getByRole("combobox",{name:"关系图视图"}).selectOption("relations");
  const graphRegion = page.getByRole("region",{name:"记忆关系图",exact:true});
  assert.equal(await graphRegion.getByRole("button",{name:/偏好 林晓/}).count(),0);
  await page.getByRole("checkbox",{name:"显示待确认"}).check();
  await graphRegion.getByRole("button",{name:/偏好 林晓/}).waitFor();
  const confirmed = reviewed.extracted_relations.find(row => row.relationship.kind === "project");
  await page.screenshot({path:join(evidence,"02-derived-typed-graph-full.png"),animations:"disabled"});
  await graphRegion.getByRole("button",{name:/项目与事项 星桥发布项目/}).click();
  await graphRegion.getByText("直接关系列表",{exact:true}).click();
  await graphRegion.getByRole("button",{name:/查看关系依据/}).click();
  const detail = page.getByRole("dialog",{name:"关系来源与依据"});
  await detail.getByText([...text].slice(confirmed.evidence_start,confirmed.evidence_end).join(""),{exact:true}).waitFor();
  await page.screenshot({path:join(evidence,"03-derived-source-full.png"),animations:"disabled"});
  await detail.getByRole("button",{name:"提交关系纠正审阅"}).click();
  await page.getByRole("tab",{name:"待审阅",selected:true}).waitFor();
  const pending = (await requireApi("GET","/api/memories/candidates")).entries;
  assert.ok(pending.some(item=>item.source_memory_id===memory.id),"review of a saved edge was staged");
  await requireApi("PUT",`/api/memories/${memory.id}`,{content:"验收样本已更正：星桥发布项目已经结项。"});
  assert.equal((await graph()).extracted_relations.length,0,"source edit invalidates previous projection");
  assert.ok((await requireApi("GET","/api/memories/candidates")).entries.filter(item=>item.source_kind==="memory_version").every(item=>item.source_removed));
  await requireApi("DELETE",`/api/memories/${memory.id}`);
  assert.equal((await graph()).extracted_relations.length,0);
  assert.equal((await requireApi("GET","/api/memories/candidates")).entries.filter(item=>item.source_kind==="memory_version").length,0);
  await writeFile(join(evidence,"result.json"),JSON.stringify({...appInfo,actualProvider:true,testText:"synthetic",durationMs,candidates:candidates.length,
    confirmedProject:true,pendingPreference:true,exactOpeningEvidence:true,derivedInvalidatedAndPurged:true,reviewDidNotDuplicateOrBumpMemory:true},null,2));
  console.log("Native saved-memory text, real-model extraction, review, provenance and purge passed.");
}
