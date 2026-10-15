import { describe, expect, it } from "vitest";
import { buildGoalViewState } from "./goalViewModel";
import type { AcceptanceReport, GoalContract } from "../../../api/proofloop";
import type { TaskGraphDetail } from "../../../api/taskWorld";

const contract: GoalContract = {
  schema_version: 1,
  goal_id: "goal-1",
  version: 2,
  workspace_id: "ws-1",
  user_goal: "生成验收过的综述",
  deliverables: [{ id: "report", title: "综述.md", mime_type: "text/markdown" }],
  criteria: [],
  constraints: [],
  allowed_capability_ids: [],
  budget: { max_model_calls: 3, max_total_tokens: 5000, max_wall_time_ms: 60000, max_repair_rounds: 1, time_basis: "active_work" },
};

const graph: TaskGraphDetail = {
  graph_id: "graph-1",
  graph: { id: "graph-1", schema_version: 1, revision: 3, node_count: 1, edge_count: 0 },
  revision: 3,
  nodes: [{
    id: "node-1", kind: "work", title: "生成综述", input: {}, retry_policy: { max_attempts: 1 },
    status: "succeeded", state: { status: "succeeded", attempts: 1, result_summary: "已完成", error: null, started_at: 1, finished_at: 2, updated_at: 2 },
    executor_ref: "capability://writer", instruction_summary: "生成", acceptance_criteria: [], resources: [],
    validation: { status: "accepted", issues: [] }, result_summary: "已完成",
    latest_execution: { execution_id: "exec-1", attempt: 1, status: "succeeded", executor_ref: "writer", validation: { status: "accepted", issues: [], checked_at: 2 }, approval_ref: null, failure_code: null, error: null, result_summary: "已完成", created_at: 1, updated_at: 2, started_at: 1, finished_at: 2 },
    execution_history: [],
  }],
  edges: [], revisions: [], checkpoints: [],
};

const passingReport = (graphRevision: number): AcceptanceReport => ({
  report_id: "report-1", goal_id: "goal-1", goal_version: 2, graph_id: "graph-1", graph_revision: graphRevision,
  snapshot_id: "snapshot-1", status: "pass", results: [],
  deliverable_results: [{ deliverable_id: "report", status: "pass", code: "verified", evidence_ids: [], reason: "内容已核验" }],
  human_reviews: [], checked_at_ms: 3,
});

const evidenceSnapshot = {
  snapshot_id: "snapshot-1",
  goal_id: "goal-1",
  goal_version: 2,
  graph_id: "graph-1",
  graph_revision: 3,
  executions: [],
  evidence: [],
  deliverables: [{ deliverable_id: "report", artifact_id: "artifact-1" }],
  sources: [],
};

describe("buildGoalViewState", () => {
  it("does not show completed when all graph work succeeded but no acceptance report exists", () => {
    const state = buildGoalViewState("graph-1", contract, graph, null, null, null);

    expect(state.phase).toBe("verifying");
    expect(state.completedNodes).toBe(1);
    expect(state.deliverables[0]).toMatchObject({ id: "report", state: "not_checked", artifactId: null });
  });

  it("marks a previous passing report stale after the graph revision changes", () => {
    const state = buildGoalViewState("graph-1", contract, graph, passingReport(2), evidenceSnapshot, null, [
      { deliverable_id: "report", artifact_id: "artifact-1", status: "current", reason: "产物字节与来源记录匹配。" },
    ]);

    expect(state.phase).toBe("needs_revalidation");
    expect(state.deliverables[0].state).toBe("stale");
  });

  it("does not complete a legacy pass report without current deliverable results", () => {
    const legacyReport = { ...passingReport(3), deliverable_results: [] };
    const state = buildGoalViewState("graph-1", contract, graph, legacyReport, evidenceSnapshot, null, [
      { deliverable_id: "report", artifact_id: "artifact-1", status: "current", reason: "产物字节与来源记录匹配。" },
    ]);

    expect(state.phase).toBe("needs_revalidation");
    expect(state.deliverables[0].state).toBe("stale");
  });

  it("marks the report stale when the mapped artifact is no longer current", () => {
    const report = passingReport(3);
    const state = buildGoalViewState("graph-1", contract, graph, report, evidenceSnapshot, null, [
      { deliverable_id: "report", artifact_id: "artifact-1", status: "stale", reason: "产物文件已删除。" },
    ]);

    expect(state.phase).toBe("needs_revalidation");
    expect(state.deliverables[0].state).toBe("stale");
    expect(report.status).toBe("pass");
  });
});
