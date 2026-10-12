import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import AcceptancePanel from "./AcceptancePanel";
import type { AcceptanceReport, EvidenceSnapshot } from "../../../api/proofloop";

describe("AcceptancePanel", () => {
  const sampleReport: AcceptanceReport = {
    report_id: "report_1",
    goal_id: "goal_1",
    goal_version: 1,
    graph_id: "graph_1",
    graph_revision: 1,
    snapshot_id: "snap_1",
    status: "pass",
    results: [
      {
        criterion_id: "crit_artifact",
        status: "pass",
        evidence_ids: ["ev_1"],
        reason: "已生成有效产物报告并匹配 SHA-256",
      },
      {
        criterion_id: "crit_sections",
        status: "pass",
        evidence_ids: ["ev_1"],
        reason: "报告包含所有必需一级大纲",
      },
    ],
    human_reviews: [],
    checked_at_ms: 1234567,
  };

  const sampleSnapshot: EvidenceSnapshot = {
    snapshot_id: "snap_1",
    goal_id: "goal_1",
    goal_version: 1,
    graph_id: "graph_1",
    graph_revision: 1,
    executions: [
      {
        graph_id: "graph_1",
        graph_revision: 1,
        node_id: "node_1",
        execution_id: "exec_1",
      },
    ],
    evidence: [
      {
        evidence_id: "ev_1",
        origin: {
          graph_id: "graph_1",
          graph_revision: 1,
          node_id: "node_1",
          execution_id: "exec_1",
        },
        artifact_id: "art_doc_1",
        output_path: null,
        digest: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        observed_at_ms: 1234560,
      },
    ],
    deliverables: [{ deliverable_id: "deliv_1", artifact_id: "art_doc_1" }],
    sources: [
      {
        source_id: "src_1",
        evidence_id: "ev_1",
        uri: "https://tauri.app/v2/docs",
        retrieved_at_ms: 1234500,
        cited_by_deliverable_ids: ["deliv_1"],
      },
    ],
  };

  it("renders passing report with criteria and evidence links", () => {
    const html = renderToStaticMarkup(
      <AcceptancePanel
        goalId="goal_1"
        report={sampleReport}
        evidenceSnapshot={sampleSnapshot}
        onOpenArtifact={vi.fn()}
      />,
    );

    expect(html).toContain("目标成果验收报告");
    expect(html).toContain("目标已达成");
    expect(html).toContain("crit_artifact");
    expect(html).toContain("已生成有效产物报告并匹配 SHA-256");
    expect(html).toContain("打开产物");
    expect(html).toContain("https://tauri.app/v2/docs");
  });

  it("renders failed report with repair trigger option", () => {
    const failedReport: AcceptanceReport = {
      ...sampleReport,
      status: "fail",
      results: [
        {
          criterion_id: "crit_artifact",
          status: "fail",
          evidence_ids: [],
          reason: "交付物文件缺失",
        },
      ],
    };

    const html = renderToStaticMarkup(
      <AcceptancePanel
        goalId="goal_1"
        report={failedReport}
        onRequestRepair={vi.fn()}
      />,
    );

    expect(html).toContain("目标未达成");
    expect(html).toContain("交付物文件缺失");
    expect(html).toContain("查看修复建议");
  });

  it("renders human pending review prompt", () => {
    const pendingReport: AcceptanceReport = {
      ...sampleReport,
      status: "incomplete",
      results: [
        {
          criterion_id: "crit_human",
          status: "human_pending",
          evidence_ids: ["ev_1"],
          reason: "需要人工复核报告结论准确性",
        },
      ],
    };

    const html = renderToStaticMarkup(
      <AcceptancePanel
        goalId="goal_1"
        report={pendingReport}
        evidenceSnapshot={sampleSnapshot}
      />,
    );

    expect(html).toContain("待人工审核");
    expect(html).toContain("进行人工审核判断");
  });
});
