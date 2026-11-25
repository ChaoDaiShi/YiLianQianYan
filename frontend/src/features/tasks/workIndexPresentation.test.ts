import { describe, expect, it } from "vitest";
import type { WorkIndexItem } from "../../api/workIndex";
import { matchesWork, workDisplayTitle, workKey, workStatusLabels } from "./workIndexPresentation";
const item: WorkIndexItem = { id: "same", kind: "conversation", title: "季度报告", workspace_id: "space", status: "waiting_approval", attention_required: true, href: "/chat/same", updated_at: 0, source_ids: {} };
describe("unified work presentation", () => {
  it("keeps ids in different source namespaces distinct", () => { expect(workKey(item)).not.toBe(workKey({ ...item, kind: "task_graph" })); });
  it("finds a legacy draft through its Chinese status without treating it as running or complete", () => { const draft = { ...item, kind: "legacy_task" as const, status: "draft", attention_required: false }; expect(matchesWork(draft, "草稿", "all", "all")).toBe(true); expect(matchesWork(draft, "", "active", "all")).toBe(false); expect(matchesWork(draft, "", "completed", "all")).toBe(false); });
  it("keeps waiting work active and discoverable through attention", () => { expect(matchesWork(item, "报告", "active", "all")).toBe(true); expect(matchesWork(item, "", "attention", "conversation")).toBe(true); expect(matchesWork(item, "", "completed", "all")).toBe(false); });
  it("does not present unknown states as completed or infer attention", () => { const unknown = { ...item, status: "future_state", attention_required: false }; expect(matchesWork(unknown, "future_state", "all", "all")).toBe(true); expect(matchesWork(unknown, "", "completed", "all")).toBe(false); expect(matchesWork(unknown, "", "attention", "all")).toBe(false); });
  it("filters each source without merging personal and chat histories", () => { expect(matchesWork({ ...item, kind: "personal_delegation" }, "", "all", "conversation")).toBe(false); });
  it("keeps goal titles and gives unnamed or technical titles a readable label", () => {
    const graph = { ...item, kind: "task_graph" as const };
    expect(workDisplayTitle(graph)).toBe("季度报告");
    expect(workDisplayTitle({ ...graph, title: `Task graph ${graph.id}` })).toBe("未命名任务画布");
    expect(workDisplayTitle({ ...item, title: "Write-Output 'ok'" })).toBe("新任务");
    expect(workDisplayTitle({ ...item, kind: "personal_delegation", title: "" })).toBe("未命名个人委托");
    expect(matchesWork({ ...graph, title: `Task graph ${graph.id}` }, "未命名任务画布", "all", "all")).toBe(true);
  });
  it("translates scheduled and skipped personal work", () => {
    expect(workStatusLabels.scheduled).toBe("已安排");
    expect(workStatusLabels.skipped).toBe("已跳过");
  });
  it("recognizes active task graph runs", () => {
    const graph = { ...item, kind: "task_graph" as const, status: "working" };
    expect(matchesWork(graph, "进行中", "active", "task_graph")).toBe(true);
  });
});
