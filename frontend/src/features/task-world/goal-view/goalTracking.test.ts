import { beforeEach, describe, expect, it, vi } from "vitest";
import { loadGoalTracking } from "./goalTracking";
import * as proof from "../../../api/proofloop";
import * as task from "../../../api/taskWorld";
import * as evidence from "../../../api/evidence";
vi.mock("../../../api/proofloop", () => ({ getGraphGoal: vi.fn(), listGoalEvaluations: vi.fn(), getEvaluationDetail: vi.fn(), evaluateGoal: vi.fn() }));
vi.mock("../../../api/taskWorld", () => ({ getTaskGraphDetail: vi.fn() }));
vi.mock("../../../api/evidence", () => ({ listArtifactResults: vi.fn() }));

const detail = { graph_id: "g", revision: 4, nodes: [{ id: "n", status: "succeeded", latest_execution: { execution_id: "e", status: "succeeded" } }] } as task.TaskGraphDetail;
const binding = { goal_id: "goal", goal_version: 1, graph_id: "g", contract: { goal_id: "goal", version: 1, deliverables: [{ id: "file" }] } } as proof.GraphGoalBinding;
const report = { report_id: "r", graph_id: "g", graph_revision: 4, goal_version: 1, checked_at_ms: 1 } as proof.AcceptanceReport;
const snapshot = { graph_id: "g", executions: [{ graph_id: "g", node_id: "n", execution_id: "e" }], deliverables: [{ deliverable_id: "file", artifact_id: "a" }] } as proof.EvidenceSnapshot;

beforeEach(() => {
  vi.resetAllMocks();
  vi.mocked(task.getTaskGraphDetail).mockResolvedValue({ ok: true, data: detail });
  vi.mocked(proof.getGraphGoal).mockResolvedValue({ ok: true, data: binding });
  vi.mocked(proof.listGoalEvaluations).mockResolvedValue({ ok: true, data: { reports: [report] } });
  vi.mocked(proof.getEvaluationDetail).mockResolvedValue({ ok: true, data: { report, snapshot, current_evidence: [] } });
  vi.mocked(evidence.listArtifactResults).mockResolvedValue([]);
});

describe("authoritative shared goal reads", () => {
  it("keeps refresh and background observation read-only even without an acceptance report", async () => {
    vi.mocked(proof.listGoalEvaluations).mockResolvedValue({ ok: true, data: { reports: [] } });
    await loadGoalTracking("g");
    await loadGoalTracking("g", { observeOnly: true, force: true });
    expect(proof.evaluateGoal).not.toHaveBeenCalled();
  });
  it("keeps stale attempts readable for recovery without attempting invalid evaluation", async () => {
    vi.mocked(task.getTaskGraphDetail).mockResolvedValue({ ok: true, data: { ...detail, nodes: [{ ...detail.nodes[0], latest_execution: { ...detail.nodes[0].latest_execution!, status: "stale" } }] } });
    vi.mocked(proof.listGoalEvaluations).mockResolvedValue({ ok: true, data: { reports: [] } });
    const state = await loadGoalTracking("g");
    expect(state.detail.nodes[0].latest_execution?.status).toBe("stale");
    expect(proof.evaluateGoal).not.toHaveBeenCalled();
  });
  it("reads binding, latest attempts and actual artifact records again on refresh", async () => {
    await loadGoalTracking("g"); await loadGoalTracking("g");
    expect(task.getTaskGraphDetail).toHaveBeenCalledTimes(2);
    expect(proof.getEvaluationDetail).toHaveBeenCalledTimes(2);
    expect(evidence.listArtifactResults).toHaveBeenCalledWith(expect.objectContaining({ execution_id: "e" }));
  });
  it("does not treat a network failure as an unbound goal", async () => {
    vi.mocked(proof.getGraphGoal).mockResolvedValue({ ok: false, error: { status: 503, code: "unavailable", message: "数据库不可用" } });
    await expect(loadGoalTracking("g")).rejects.toThrow("数据库不可用");
  });
  it("does not create an evaluation while execution is running", async () => {
    vi.mocked(task.getTaskGraphDetail).mockResolvedValue({ ok: true, data: { ...detail, nodes: [{ ...detail.nodes[0], latest_execution: { ...detail.nodes[0].latest_execution!, status: "running" } }] } });
    vi.mocked(proof.listGoalEvaluations).mockResolvedValue({ ok: true, data: { reports: [] } });
    await loadGoalTracking("g");
    expect(proof.evaluateGoal).not.toHaveBeenCalled();
  });
  it("revalidates a changed attempt only when validation is requested", async () => {
    vi.mocked(proof.getEvaluationDetail).mockResolvedValueOnce({ ok: true, data: { report, snapshot: { ...snapshot, executions: [] } } });
    vi.mocked(proof.evaluateGoal).mockResolvedValue({ ok: true, data: { ...report, report_id: "r-new" } });
    const state = await loadGoalTracking("g", { force: true });
    expect(proof.evaluateGoal).toHaveBeenCalledWith("goal", { goal_version: 1, graph_revision: 4, execution_ids: ["e"] }, undefined);
    expect(state.report?.report_id).toBe("r-new");
  });
});
