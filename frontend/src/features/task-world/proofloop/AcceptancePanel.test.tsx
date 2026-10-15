import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import AcceptancePanel from "./AcceptancePanel";
import type { AcceptanceReport, EvidenceSnapshot, GoalContract } from "../../../api/proofloop";
import { normalizeAcceptanceReport } from "./proofloopModel";

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
    deliverable_results: [
      {
        deliverable_id: "deliv_1",
        status: "pass",
        code: "verified",
        evidence_ids: ["ev_1"],
        reason: "交付物 'deliv_1' 的实际文件、来源和摘要已核验",
      },
    ],
    human_reviews: [],
    checked_at_ms: 1234567,
  };

  const sampleContract: GoalContract = {
    schema_version: 1,
    goal_id: "goal_1",
    version: 1,
    workspace_id: "workspace_1",
    user_goal: "生成并核验报告",
    deliverables: [{ id: "deliv_1", title: "验收报告", mime_type: "application/pdf" }],
    criteria: [],
    constraints: [],
    allowed_capability_ids: [],
    budget: { max_model_calls: 1, max_total_tokens: 100, max_wall_time_ms: 1000, max_repair_rounds: 0 },
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
        contract={sampleContract}
        evidenceSnapshot={sampleSnapshot}
        onOpenArtifact={vi.fn()}
      />,
    );

    expect(html).toContain("目标成果验收报告");
    expect(html).toContain("目标已达成");
    expect(html).toContain("交付成果检查");
    expect(html).toContain("质量与人工复核");
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
      deliverable_results: [
        {
          deliverable_id: "deliv_1",
          status: "fail",
          code: "artifact_missing",
          evidence_ids: [],
          reason: "交付物 'deliv_1' 的实际文件不存在",
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

  it.each([
    ["missing", { ...sampleReport, deliverable_results: undefined } as unknown as AcceptanceReport],
    ["empty", { ...sampleReport, deliverable_results: [] }],
  ])("shows reevaluation for a passing legacy report with %s deliverable results", (_fieldState, legacyReport) => {
    const html = renderToStaticMarkup(
      <AcceptancePanel goalId="goal_1" report={legacyReport} contract={sampleContract} />,
    );

    expect(html).toContain("此报告缺少契约声明的交付物检查结果，需要重新验收。");
    expect(html).toContain("需重新验收");
    expect(html).not.toContain("目标已达成");
    expect(legacyReport.status).toBe("pass");
  });

  it("normalizes a missing legacy result field without changing its historical outcome", () => {
    const legacyReport = { ...sampleReport, deliverable_results: undefined } as unknown as AcceptanceReport;
    const normalized = normalizeAcceptanceReport(legacyReport);

    expect(normalized.deliverable_results).toEqual([]);
    expect(normalized.status).toBe("pass");
    expect(normalized.report_id).toBe(sampleReport.report_id);
    expect(normalized.results).toEqual(sampleReport.results);
  });

  it("keeps a passing report green when every declared deliverable has a check", () => {
    const html = renderToStaticMarkup(
      <AcceptancePanel goalId="goal_1" report={sampleReport} contract={sampleContract} />,
    );

    expect(html).toContain("目标已达成");
    expect(html).not.toContain("需重新验收");
  });

  it("does not require deliverable checks when the contract declares none", () => {
    const html = renderToStaticMarkup(
      <AcceptancePanel
        goalId="goal_1"
        report={{ ...sampleReport, deliverable_results: [] }}
        contract={{ ...sampleContract, deliverables: [] }}
      />,
    );

    expect(html).toContain("目标已达成");
    expect(html).toContain("当前契约未声明交付物。");
  });
});
