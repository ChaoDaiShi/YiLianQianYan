import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import RepairReviewPanel from "./RepairReviewPanel";
import type { RepairProposal } from "../../../api/proofloop";

describe("RepairReviewPanel", () => {
  const sampleProposal: RepairProposal = {
    schema_version: 1,
    repair_id: "repair_123",
    goal_id: "goal_123",
    goal_version: 1,
    expected_graph_revision: 3,
    source_report_id: "report_1",
    failure_execution_ids: ["exec_err_1"],
    round: 1,
    operations: [
      {
        kind: "replace_literal",
        node_id: "node_b",
        target_path: ["format"],
        value: "markdown",
      },
      {
        kind: "rerun_node",
        node_id: "node_b",
      },
    ],
    rerun_root_node_id: "node_b",
    affected_node_ids: ["node_b", "node_c"],
    rationale: "由于下游节点要求 markdown 格式输出，将 node_b 的 format 修正为 markdown 并重跑下游分支",
  };

  it("renders repair proposal details and operations", () => {
    const html = renderToStaticMarkup(
      <RepairReviewPanel
        proposal={sampleProposal}
        currentGraphRevision={3}
        onApplyRepair={vi.fn()}
      />,
    );

    expect(html).toContain("受控局部修复审查");
    expect(html).toContain("第 1 轮修复");
    expect(html).toContain("node_b");
    expect(html).toContain("node_b → node_c");
    expect(html).toContain("修改固定参数");
    expect(html).toContain("单节点重跑");
    expect(html).toContain("确认并应用局部修复");
  });

  it("warns about revision conflict and disables apply when revision mismatches", () => {
    const html = renderToStaticMarkup(
      <RepairReviewPanel
        proposal={sampleProposal}
        currentGraphRevision={4} // Different from expected_graph_revision = 3
        onApplyRepair={vi.fn()}
      />,
    );

    expect(html).toContain("任务图版本已变更");
    expect(html).toContain("提案基于版本 r3 生成，但当前图版本已更新为 r4");
    expect(html).toContain("disabled");
  });
});
