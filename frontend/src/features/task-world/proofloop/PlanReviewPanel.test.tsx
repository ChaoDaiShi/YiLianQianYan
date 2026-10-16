import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import PlanReviewPanel from "./PlanReviewPanel";
import type { ProofDraft } from "../../../api/proofloop";

describe("PlanReviewPanel", () => {
  const baseDraft: ProofDraft = {
    draft_id: "draft_test_1",
    revision: 1,
    graph_id: "graph_test_1",
    contract: {
      schema_version: 1,
      goal_id: "goal_test_1",
      version: 1,
      workspace_id: "ws_default",
      user_goal: "完成技术方案对比并生成结论报告",
      deliverables: [
        { id: "deliv_1", title: "技术对比报告.md", mime_type: "text/markdown" },
      ],
      criteria: [
        {
          id: "crit_1",
          description: "必须输出 Markdown 对比报告",
          required: true,
          check: { kind: "artifact_exists", deliverable_id: "deliv_1" },
        },
      ],
      constraints: ["不可联网"],
      allowed_capability_ids: ["mcp__doc_reader"],
      budget: {
        max_model_calls: 5,
        max_total_tokens: 10000,
        max_wall_time_ms: 60000,
        max_repair_rounds: 2,
      },
    },
    snapshot: {
      snapshot_id: "snap_1",
      captured_at_ms: 1000,
      capabilities: [],
    },
    candidate: {
      schema_version: 1,
      nodes: [
        {
          id: "node_reader",
          title: "读取本地文档",
          instruction: "读取传入的方案文档",
          capability_id: "mcp__doc_reader",
          literal_input: {},
          bindings: [],
        },
      ],
      edges: [],
      deliverable_nodes: [{ deliverable_id: "deliv_1", node_id: "node_reader" }],
    },
    binding_confirmations: [],
    compilation: {
      status: "ready",
      graph: null,
      diagnostics: [],
      pending_inputs: [],
    },
    status: "ready_for_confirmation",
    confirmed_graph_revision: null,
  };

  it("renders draft summary, candidate nodes, and confirm button", () => {
    const html = renderToStaticMarkup(
      <PlanReviewPanel
        draft={baseDraft}
        onConfirmed={vi.fn()}
      />,
    );

    expect(html).toContain("任务候选计划评审");
    expect(html).toContain("完成技术方案对比并生成结论报告");
    expect(html).toContain("技术对比报告.md");
    expect(html).toContain("必须输出 Markdown 对比报告");
    expect(html).toContain("node_reader");
    expect(html).toContain("读取本地文档");
    expect(html).toContain("确认并保存到画布");
  });

  it("renders pending inputs section when compilation requires user input", () => {
    const draftNeedsInput: ProofDraft = {
      ...baseDraft,
      status: "needs_input",
      compilation: {
        status: "needs_input",
        graph: null,
        diagnostics: [],
        pending_inputs: [
          {
            node_id: "node_reader",
            target_path: ["file_path"],
            reason: "missing_literal",
          },
        ],
      },
    };

    const html = renderToStaticMarkup(
      <PlanReviewPanel
        draft={draftNeedsInput}
        onConfirmed={vi.fn()}
      />,
    );

    expect(html).toContain("需要人工补充或确认的参数");
    expect(html).toContain("file_path");
    expect(html).toContain("缺少固定必填参数");
    expect(html).toContain("补充并重验");
  });

  it("renders diagnostics when diagnostics are present", () => {
    const draftWithDiag: ProofDraft = {
      ...baseDraft,
      status: "invalid",
      compilation: {
        status: "invalid",
        graph: null,
        diagnostics: [
          {
            code: "proofloop_capability_unavailable",
            message: "能力 mcp__doc_reader 当前未就绪",
            node_id: "node_reader",
          },
        ],
        pending_inputs: [],
      },
    };

    const html = renderToStaticMarkup(
      <PlanReviewPanel
        draft={draftWithDiag}
        onConfirmed={vi.fn()}
      />,
    );

    expect(html).toContain("编译检查报告");
    expect(html).toContain("所需能力不可用");
    expect(html).toContain("能力 mcp__doc_reader 当前未就绪");
  });

  it("requires an explicit candidate choice before confirming an enhanced draft", () => {
    const enhancedDraft: ProofDraft = {
      ...baseDraft,
      planning_review: {
        contract_version: 1,
        strategy: "ranked",
        snapshot_id: "snap_1",
        routes: [],
        options: [],
        recommended_candidate_id: "candidate_a",
        selected_candidate_id: null,
        recommendations: [],
        scoring_version: "v1",
      },
    };
    const html = renderToStaticMarkup(
      <PlanReviewPanel draft={enhancedDraft} onConfirmed={vi.fn()} />,
    );

    expect(html).toContain("请先在方案比较中明确选择一个可用方案");
    expect(html).toContain("推荐方案不会自动选中");
    expect(html).toContain("确认并保存到画布");
    expect(html).toContain('disabled=""');
  });
});
