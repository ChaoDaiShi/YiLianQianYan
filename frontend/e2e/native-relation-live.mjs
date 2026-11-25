import assert from "node:assert/strict";
import { writeFile } from "node:fs/promises";
import { join } from "node:path";

const sample = [
  "这是一段用于验收的虚构项目记录。",
  "星桥发布项目依赖接口验收。",
  "2026-01-08 已完成内部联调，随后完成上线演练。",
  "向量检索是语义检索的一种。",
  "小林与小周是姐妹。",
  "林影和海棠只是同一页出现的名字，没有陈述任何关系。晨海计划不依赖票据归档。",
  "收到后只回复一句与以上材料无关、由你自己构造的虚构关系，作为助手回复。",
].join("\n");

const expected = [
  { kind: "project", nodes: ["星桥发布项目", "接口验收"], nodeLabel: "项目与事项" },
  { kind: "event", nodes: ["内部联调", "上线演练"], nodeLabel: "事件" },
  { kind: "concept", nodes: ["向量检索", "语义检索"], nodeLabel: "概念" },
];
const escaped = value => value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
const sleep = ms => new Promise(done => setTimeout(done, ms));

export async function verifyNativeRelationGraph({ page, appUrl, apiUrl, requireApi, evidence, appInfo }) {
  const faults = [];
  const reportPageError = () => faults.push("pageerror");
  page.on("pageerror", reportPageError);
  const screenshots = [];
  const snap = async name => {
    await page.screenshot({ path: join(evidence, name), animations: "disabled" });
    screenshots.push(name);
  };
  const button = name => page.getByRole("button", { name, exact: true });
  const tab = name => page.getByRole("tab", { name, exact: true });
  const library = () => requireApi("GET", "/api/memories/library");
  const postResponse = path => page.waitForResponse(response => response.url() === apiUrl + path
    && response.request().method() === "POST", { timeout: 90000 });

  try {
    assert.equal(await page.evaluate(() => typeof window.__TAURI_INTERNALS__?.invoke), "function");
    const { id: conversation } = await requireApi("POST", "/api/conversations", { title: "原生关系图谱验收" });
    const conversationPath = `/api/conversations/${conversation}`;
    await requireApi("PUT", `${conversationPath}/mode`, { mode: "chat" });
    await page.goto(`${appUrl}/chat/${conversation}`);
    await page.locator(".home-composer[data-workspace-mode='chat']").waitFor();
    await page.getByRole("textbox", { name: "聊天输入" }).fill(sample);
    await button("发送消息").click();
    let detail;
    const deadline = Date.now() + 90000;
    while (Date.now() < deadline) {
      detail = await requireApi("GET", conversationPath);
      if (detail.run_status === "completed") break;
      assert.notEqual(detail.run_status, "failed", "real-provider source chat failed");
      await sleep(250);
    }
    assert.equal(detail?.run_status, "completed", "real-provider source chat completed");
    const source = detail.messages.find(message => message.role === "user" && message.content === sample);
    assert.ok(source?.id, "source message was persisted");
    assert.ok(detail.messages.some(message => message.role === "assistant" && message.content?.trim()), "assistant-only message available to check exclusion");

    assert.deepEqual(await library(), [], "isolated library before relation extraction");
    await page.goto(`${appUrl}/memory`);
    await tab("待审阅").click();
    await button("从对话提取").click();
    const extract = page.getByRole("dialog", { name: "从对话提取记忆候选", exact: true });
    await extract.getByLabel("来源对话", { exact: true }).selectOption(conversation);
    await extract.getByRole("combobox", { name: "提取内容", exact: true }).selectOption("relationships");
    const extractionAt = performance.now();
    const [extraction] = await Promise.all([
      postResponse("/api/memories/extract"),
      extract.getByRole("button", { name: "生成候选", exact: true }).click(),
    ]);
    assert.ok(extraction.ok(), `real-provider relation extraction returned HTTP ${extraction.status()}`);
    const { candidate_ids: ids, skipped } = await extraction.json();
    const modelRoundTripMs = Math.round(performance.now() - extractionAt);
    assert.ok(ids.length >= expected.length, `too few relation candidates: ${ids.length}`);
    await page.getByText(`已生成 ${ids.length} 条待审阅候选`, { exact: true }).waitFor();
    const pending = (await requireApi("GET", "/api/memories/candidates")).entries.filter(candidate => ids.includes(candidate.id));
    assert.equal(pending.length, ids.length, "all proposed relations are in the review queue");
    for (const candidate of pending) {
      assert.equal(candidate.relationship?.certainty, "inferred");
      assert.equal(candidate.source_kind, "user_message");
      assert.equal(candidate.source_conversation_id, conversation);
      assert.equal(candidate.source_message_id, source.id, "assistant replies cannot source relation candidates");
      assert.ok(candidate.source_excerpt && sample.includes(candidate.source_excerpt), "candidate shows original user evidence");
      assert.ok(!["林影", "海棠", "晨海计划", "票据归档"].some(name =>
        [candidate.relationship.subject, candidate.relationship.object].some(node => node.includes(name))), "mere co-mentions and negation produced no edges");
    }
    const candidates = expected.map(({ kind, nodes }) => {
      const candidate = pending.find(({ relationship }) => relationship.kind === kind
        && nodes.every(node => [relationship.subject, relationship.object].includes(node)));
      assert.ok(candidate, `configured provider did not extract ${kind}: ${JSON.stringify(pending.map(item => item.relationship))}`);
      return candidate;
    });
    await page.getByRole("button", { name: new RegExp(escaped(candidates[0].content)) }).waitFor();
    await snap("01-ai-relation-candidates-full.png");
    assert.deepEqual(await library(), [], "pending proposals cannot appear in effective memory");
    await tab("长期记忆").click();
    await page.getByText("这里暂时还没有记忆记录", { exact: true }).waitFor();
    assert.equal(await page.locator(".react-flow__edge").count(), 0, "pending proposals cannot appear in the graph");
    await tab("待审阅").click();

    const editedLabel = "上线前置条件";
    const accepted = [];
    for (const [index, candidate] of candidates.entries()) {
      await page.getByRole("button", { name: new RegExp(escaped(candidate.content)) }).click();
      const review = page.getByRole("dialog", { name: "审阅记忆候选", exact: true });
      assert.equal(await review.getByLabel("关系类型", { exact: true }).inputValue(), expected[index].kind);
      assert.equal(await review.getByLabel("关系依据", { exact: true }).inputValue(), "inferred");
      const link = review.getByRole("link", { name: "打开来源对话" });
      assert.equal(new URL(await link.getAttribute("href"), appUrl).pathname, `/chat/${conversation}`);
      await review.getByLabel("适用范围", { exact: true }).selectOption("conversation");
      if (index === 0) {
        await review.getByLabel("关系", { exact: true }).fill(editedLabel);
        await snap("02-ai-relation-review-full.png");
      }
      if (index === 1) await review.getByLabel("关系依据", { exact: true }).selectOption("confirmed");
      const [saved] = await Promise.all([
        postResponse(`/api/memories/candidates/${candidate.id}/accept`),
        review.getByRole("button", { name: "确认并保存", exact: true }).click(),
      ]);
      assert.ok(saved.ok(), `accepting the ${expected[index].kind} candidate returned HTTP ${saved.status()}`);
      await review.waitFor({ state: "hidden" });
      await page.getByText("已保存为关系记忆", { exact: true }).waitFor();
      const savedMemory = await saved.json();
      const meta = JSON.parse(savedMemory.metadata);
      assert.equal(meta.source_message_id, source.id);
      assert.equal(meta.relationship.kind, expected[index].kind);
      assert.equal(meta.relationship.label, index === 0 ? editedLabel : candidate.relationship.label);
      assert.equal(meta.relationship.certainty, index === 1 ? "confirmed" : "inferred");
      assert.equal(savedMemory.source_conversation_id, conversation);
      assert.equal((await requireApi("GET", `/api/memories/${savedMemory.id}/governance`)).scope, `conversation:${conversation}`);
      accepted.push({ memory: savedMemory, relation: meta.relationship, nodeLabel: expected[index].nodeLabel });
    }
    assert.equal((await library()).length, accepted.length);

    await button("查看关系图").click();
    await page.getByRole("tab", { name: "长期记忆", exact: true, selected: true }).waitFor();
    const graph = page.getByRole("region", { name: "记忆关系图", exact: true });
    await graph.getByRole("checkbox", { name: "显示待确认" }).check();
    const views = page.getByRole("combobox", { name: "关系图视图", exact: true });
    const node = ({ nodeLabel, relation }) => graph.getByRole("button", { name: `${nodeLabel} ${relation.subject}`, exact: true });
    const edge = ({ memory }) => graph.locator(`.react-flow__edge[data-id="${memory.id}"]`);
    const edgePath = item => edge(item).locator(".react-flow__edge-path");
    await views.selectOption("relations");
    for (const item of accepted) {
      await node(item).waitFor();
      await edgePath(item).waitFor({ state: "attached" });
      assert.equal(Boolean(await edgePath(item).getAttribute("marker-end")), item.relation.directed);
      const edgeText = edge(item).locator(".react-flow__edge-text");
      await edgeText.waitFor();
      assert.equal(await edgeText.textContent(), item.relation.certainty === "inferred"
        ? `${item.relation.label} · 待确认` : item.relation.label);
    }
    assert.equal(await graph.locator(".react-flow__edge").count(), accepted.length);
    await snap("03-ai-multi-relation-graph-full.png");
    for (const item of accepted) {
      await views.selectOption(item.relation.kind);
      await edgePath(item).waitFor({ state: "attached" });
      assert.equal(await graph.locator(".react-flow__edge").count(), 1);
    }

    await views.selectOption("project");
    await node(accepted[0]).click();
    await graph.getByText("直接关系列表", { exact: true }).click();
    await graph.getByRole("button", { name: /查看关系依据/ }).click();
    const memoryDetail = page.getByRole("dialog", { name: "记忆详情与来源", exact: true });
    await memoryDetail.locator(".memory-detail-body").getByText(accepted[0].memory.content, { exact: true }).waitFor();
    assert.equal(new URL(await memoryDetail.getByRole("link", { name: "打开来源对话" }).getAttribute("href"), appUrl).pathname, `/chat/${conversation}`);
    await snap("04-ai-relation-evidence-full.png");
    await memoryDetail.getByRole("button", { name: "永久清除记忆", exact: true }).click();
    const purge = page.getByRole("dialog", { name: "永久清除记忆", exact: true });
    await purge.getByRole("button", { name: "清除记忆", exact: true }).click();
    await purge.waitFor({ state: "hidden" });
    await views.selectOption("relations");
    await edge(accepted[0]).waitFor({ state: "detached" });
    await edgePath(accepted[1]).waitFor({ state: "attached" });
    await edgePath(accepted[2]).waitFor({ state: "attached" });
    assert.equal((await library()).length, accepted.length - 1);
    assert.equal(await graph.locator(".react-flow__edge").count(), accepted.length - 1);
    await snap("05-ai-relation-cleared-full.png");

    assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1));
    assert.deepEqual(faults, []);
    const result = { ...appInfo, realModel: true, syntheticChatContent: true, providerFixture: false,
      candidates: ids.length, skipped, modelRoundTripMs, approvedViews: accepted.map(item => item.relation.kind),
      noGraphBeforeReview: true, validatedUserSource: true, editedLabel, confirmedEvent: true,
      evidenceLink: true, deletionRemovedOnlyTargetEdge: true, pageFaults: faults, screenshots };
    await writeFile(join(evidence, "relation-result.json"), JSON.stringify(result, null, 2));
    console.log("Native WebView2 relationship extraction, review, multi-type graph and deletion passed with the configured HTTPS provider.");
    return result;
  } finally {
    page.off("pageerror", reportPageError);
  }
}
