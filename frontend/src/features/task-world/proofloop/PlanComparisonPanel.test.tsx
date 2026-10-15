import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import PlanComparisonPanel from "./PlanComparisonPanel";
import type { ProofDraft } from "../../../api/proofloop";

const draft: ProofDraft = {
  draft_id: "draft-1",
  revision: 4,
  graph_id: "graph-1",
  contract: {
    schema_version: 1,
    goal_id: "goal-1",
    version: 1,
    workspace_id: "ws-1",
    user_goal: "输出一份本地资料综述",
    deliverables: [{ id: "report", title: "资料综述.md", mime_type: "text/markdown" }],
    criteria: [],
    constraints: [],
    allowed_capability_ids: ["read_file"],
    budget: { max_model_calls: 3, max_total_tokens: 5000, max_wall_time_ms: 60000, max_repair_rounds: 1 },
  },
  snapshot: { snapshot_id: "snapshot-1", captured_at_ms: 1, capabilities: [] },
  candidate: null,
  binding_confirmations: [],
  compilation: { status: "invalid", graph: null, diagnostics: [], pending_inputs: [] },
  status: "invalid",
  confirmed_graph_revision: null,
  planning_review: {
    contract_version: 1,
    strategy: "ranked",
    snapshot_id: "snapshot-1",
    routes: [],
    options: [
      {
        candidate_id: "candidate-a",
        candidate: { schema_version: 1, nodes: [{ id: "a", title: "读取本地资料", instruction: "读取资料", capability_id: "read_file", literal_input: { path: "notes.md" }, bindings: [] }], edges: [], deliverable_nodes: [{ deliverable_id: "report", node_id: "a" }] },
        compilation: { status: "invalid", graph: null, diagnostics: [{ code: "proofloop_capability_unavailable", message: "读取工具不可用" }], pending_inputs: [] },
        score: { coverage_bp: 10000, type_bp: 9000, evidence_bp: 6000, risk_bp: 2000, cost_bp: 0, cost_known: false, total: 82, rejected_codes: ["capability_unavailable"] },
      },
      {
        candidate_id: "candidate-b",
        candidate: { schema_version: 1, nodes: [{ id: "b", title: "整理资料摘要", instruction: "按主题整理", capability_id: "read_file", literal_input: {}, bindings: [] }], edges: [], deliverable_nodes: [{ deliverable_id: "report", node_id: "b" }] },
        compilation: { status: "ready", graph: null, diagnostics: [], pending_inputs: [] },
        score: { coverage_bp: 10000, type_bp: 8000, evidence_bp: 7000, risk_bp: 1000, cost_bp: 8500, cost_known: true, total: 80, rejected_codes: [] },
      },
    ],
    recommended_candidate_id: "candidate-a",
    selected_candidate_id: null,
    recommendations: [],
    scoring_version: "v1",
  },
};

describe("PlanComparisonPanel", () => {
  it("shows both plans and prevents selecting an invalid recommended plan", () => {
    const html = renderToStaticMarkup(<PlanComparisonPanel draft={draft} onDraftChange={vi.fn()} />);

    expect(html).toContain("方案比较");
    expect(html).toContain("推荐方案");
    expect(html).toContain("读取工具不可用");
    expect(html).toContain("整理资料摘要");
    expect(html).toMatch(/<button(?=[^>]*aria-label="选择方案 1")(?=[^>]*disabled="")[^>]*>/);
    expect(html).toMatch(/<button(?=[^>]*aria-label="选择方案 2")[^>]*>/);
    expect(html).toContain("预计成本未知");
  });
});
