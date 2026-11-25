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
    id: "node-1", kind: "work", title: "生成综述",
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
  executions: [{ graph_id: "graph-1", graph_revision: 3, node_id: "node-1", execution_id: "exec-1" }],
  evidence: [],
  deliverables: [{ deliverable_id: "report", artifact_id: "artifact-1" }],
  sources: [],
};

describe("buildGoalViewState", () => {
  it("keeps a historical failed chat in history after newer valid acceptance", () => {
    const state = buildGoalViewState("graph-1", contract, graph, passingReport(3), evidenceSnapshot, null,
      [{ deliverable_id: "report", artifact_id: "artifact-1", status: "current", reason: "匹配" }],
      [{ conversation_id: "chat", title: "先前执行", run_status: "failed", run_error: "先前失败", run_started_at: 1, run_finished_at: 2 }]);
    expect(state.phase).toBe("completed"); expect(state.blockers).toEqual([]);
  });
  it("never substitutes a completed chat for deliverable acceptance", () => {
    const state = buildGoalViewState("graph-1", contract, graph, null, null, null, null,
      [{ conversation_id: "chat", title: "目标讨论", run_status: "completed", run_error: null }]);
    expect(state.phase).toBe("verifying");
  });
  it.each(["failed", "interrupted"] as const)("keeps a newer chat %s visible despite a previous passing result", (run_status) => {
    const state = buildGoalViewState("graph-1", contract, graph, passingReport(3), evidenceSnapshot, null,
      [{ deliverable_id: "report", artifact_id: "artifact-1", status: "current", reason: "匹配" }],
      [{ conversation_id: "chat", title: "继续处理", run_status, run_error: "新执行失败", run_started_at: 10, run_finished_at: 20 }]);
    expect(state.phase).toBe("failed");
  });
  it.each(["running", "waiting_approval", "waiting_input", "interrupted"] as const)("shares a linked chat's %s state without pretending it is an artifact", (status) => {
    const detail = structuredClone(graph);
    detail.nodes[0].status = "runnable"; detail.nodes[0].latest_execution = null;
    const state = buildGoalViewState("graph-1", contract, detail, null, null, null, null, [{ conversation_id: "chat-1", title: "目标讨论", run_status: status, run_error: status === "interrupted" ? "应用退出前任务尚未完成" : null }]);
    expect(state.phase).toBe(status === "waiting_input" ? "needs_input" : status === "interrupted" ? "failed" : status);
    expect(state.deliverables[0].state).toBe("not_checked");
    if (status !== "running") expect(state.blockers[0].conversationId).toBe("chat-1");
  });
  it("shows persisted cancellation even if unfinished node states survive restart", () => {
    const detail = { ...graph, control_state: "cancelled" as const };
    const state = buildGoalViewState("graph-1", contract, detail, passingReport(3), evidenceSnapshot);
    expect(state.phase).toBe("cancelled");
    expect(state.blockers).toEqual([]);
  });

  it("shows an actual missing deliverable failure and its reason", () => {
    const report: AcceptanceReport = { ...passingReport(3), status: "fail", deliverable_results: [{ deliverable_id: "report", status: "fail", code: "artifact_missing", evidence_ids: [], reason: "没有生成综述文件" }] };
    const state = buildGoalViewState("graph-1", contract, graph, report, { ...evidenceSnapshot, deliverables: [] }, null, []);
    expect(state.phase).toBe("failed");
    expect(state.blockers).toContainEqual({ code: "artifact_missing", message: "没有生成综述文件", nodeId: null });
  });
  it.each(["running", "waiting_approval", "failed"] as const)("shows current %s instead of an obsolete report", (status) => {
    const detail = structuredClone(graph);
    detail.nodes[0].status = status === "waiting_approval" ? "running" : status;
    detail.nodes[0].latest_execution!.status = status;
    expect(buildGoalViewState("graph-1", contract, detail, passingReport(2), evidenceSnapshot).phase).toBe(status);
  });

  it("shows cancelled work with pending downstream nodes as cancelled", () => {
    const detail = structuredClone(graph);
    detail.nodes[0].status = "cancelled";
    detail.nodes[0].latest_execution!.status = "cancelled";
    detail.nodes.push({ ...structuredClone(graph.nodes[0]), id: "downstream", status: "pending", latest_execution: null });
    expect(buildGoalViewState("graph-1", contract, detail, null, null).phase).toBe("cancelled");
  });

  it("does not let a passing report for older attempts complete a new attempt", () => {
    const detail = structuredClone(graph);
    detail.nodes[0].latest_execution!.execution_id = "exec-new";
    const snapshot = { ...evidenceSnapshot, executions: [{ graph_id: "graph-1", graph_revision: 3, node_id: "node-1", execution_id: "exec-old" }] };
    const state = buildGoalViewState("graph-1", contract, detail, passingReport(3), snapshot, null, [
      { deliverable_id: "report", artifact_id: "artifact-1", status: "current", reason: "当前文件匹配" },
    ]);
    expect(state.phase).toBe("needs_revalidation");
  });
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
