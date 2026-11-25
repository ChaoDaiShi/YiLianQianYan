import { describe, expect, it } from "vitest";
import { goalActionPolicy, goalNodeAction } from "./goalActionPolicy";
import type { GoalTrackingData } from "./goalTracking";

function data(status = "running", control = "running"): GoalTrackingData {
  return { detail: { graph_id: "g", revision: 1, control_state: control, edges: [], nodes: [{ id: "n", kind: "work", title: "生成报告", status, state: { error: null }, executor_ref: "workflow://writer", latest_execution: { execution_id: "e", status, failure_code: "provider_error", error: "提供方暂不可用" } }] }, binding: null, report: null, snapshot: null, currentEvidence: null, artifacts: [], artifactError: null } as unknown as GoalTrackingData;
}

describe("goal action decisions", () => {
  it("never offers resume or execution for cancelled goals", () => {
    const cancelled = data("cancelled", "cancelled");
    expect(goalActionPolicy(cancelled)).toMatchObject({ canResume: false, canPause: false, canCancel: false, canVerify: false });
    expect(goalNodeAction(cancelled.detail, cancelled.detail.nodes[0]).allowed).toBe(false);
  });
  it("keeps cancel available during a failed read while disabling resume and validation", () => {
    const policy = goalActionPolicy(data("running"), "状态读取失败");
    expect(policy).toMatchObject({ canCancel: true, canResume: false, canPause: false, canVerify: false });
  });
  it("routes an approval to the existing review screen without approving it", () => {
    const policy = goalActionPolicy(data("waiting_approval"));
    expect(policy.next).toMatchObject({ kind: "navigate", href: "/system?card=approvals", label: "处理审批" });
    expect(policy.canVerify).toBe(false);
  });
  it("routes a failed attempt to its exact node for deliberate recovery", () => {
    expect(goalActionPolicy(data("failed")).next).toMatchObject({ kind: "navigate", href: "/task-world/g?node=n" });
  });
  it("does not offer execution when dependencies or the executor are missing", () => {
    const graph = data("pending").detail;
    expect(goalNodeAction(graph, graph.nodes[0]).allowed).toBe(false);
    const runnable = { ...graph.nodes[0], status: "runnable" as const, latest_execution: null, executor_ref: null };
    expect(goalNodeAction(graph, runnable).allowed).toBe(false);
    expect(goalNodeAction({ ...graph, nodes: [runnable] }, { ...runnable, executor_ref: "workflow://writer" }).allowed).toBe(true);
  });
  it("allows a prepared rerun with an invalidated node and stale prior attempt", () => {
    const prepared = data("stale").detail;
    prepared.nodes[0].status = "invalidated";
    prepared.edges = [];
    expect(goalNodeAction(prepared, prepared.nodes[0]).allowed).toBe(true);
    const waiting = { ...prepared, edges: [{ from: "upstream", to: "n" }], nodes: [...prepared.nodes, { ...prepared.nodes[0], id: "upstream", status: "failed" as const }] };
    expect(goalNodeAction(waiting, waiting.nodes[0]).allowed).toBe(false);
  });
  it("explains a budget refusal instead of suggesting resume", () => {
    const paused = data("cancelled", "paused");
    paused.binding = { goal_id: "goal", goal_version: 1, graph_id: "g", budget_error: { code: "proofloop_budget_exhausted", message: "活动预算已用尽" }, contract: { schema_version: 1, goal_id: "goal", version: 1, workspace_id: "ws", user_goal: "报告", deliverables: [], criteria: [], constraints: [], allowed_capability_ids: [], budget: { max_model_calls: 1, max_total_tokens: 100, max_wall_time_ms: 1000, max_repair_rounds: 0 } } };
    expect(goalActionPolicy(paused)).toMatchObject({ canResume: false, blockedReason: "活动预算已用尽" });
  });
});
