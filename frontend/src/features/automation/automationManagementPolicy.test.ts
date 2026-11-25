import { describe, expect, it } from "vitest";
import type { Automation } from "../../api/automations";
import { automationManagementPolicy, matchesAutomation } from "./automationManagementPolicy";

function task(status = "completed", overrides: Partial<Automation> = {}): Automation {
  return { id: "a", revision: 1, name: "项目资料跟进", workflow_graph_id: "", workspace_id: "w", timezone: "UTC", schedule: { kind: "daily", time: "09:00" }, enabled: true, next_run_at: 1, created_at: 1, updated_at: 1,
    target: { kind: "personal_assistant", goal: "整理 Alpha 项目的资料", max_runs: 5, allow_file_writes: false },
    assistant_activity: { used_runs: 1, latest: { status } as NonNullable<Automation["assistant_activity"]>["latest"] }, ...overrides };
}

describe("personal management state policy", () => {
  it("keeps waiting in the original conversation and blocks duplicate starts and editing", () => {
    for (const status of ["dispatching", "running", "waiting_input", "waiting_approval"]) {
      const policy = automationManagementPolicy(task(status));
      expect(policy.canOpenRun).toBe(false); expect(policy.canEdit).toBe(false);
      expect(policy.canStop).toBe(true); expect(policy.canToggleSchedule).toBe(true);
    }
    expect(automationManagementPolicy(task("waiting_input")).waiting).toBe(true);
    expect(automationManagementPolicy(task("running", { enabled: false })).canToggleSchedule).toBe(false);
  });
  it("routes uncertain outcomes through review and refuses exhausted or archived starts", () => {
    for (const status of ["failed", "interrupted", "cancelled", "needs_review", "blocked"]) {
      const policy = automationManagementPolicy(task(status));
      expect(policy.requiresReview).toBe(true); expect(policy.runReason).toContain("核对");
      expect(policy.canOpenRun).toBe(true);
    }
    const exhausted = task("completed", { assistant_activity: { latest: null, used_runs: 5 } });
    expect(automationManagementPolicy(exhausted).canOpenRun).toBe(false);
    expect(automationManagementPolicy(exhausted).canEdit).toBe(true);
    const archived = automationManagementPolicy(task("completed", { archived: true, enabled: false }));
    expect(archived.canOpenRun).toBe(false); expect(archived.canEdit).toBe(false); expect(archived.canStop).toBe(false);
  });
  it("separates archived, attention and text search without hiding paused failures", () => {
    const failed = task("failed", { enabled: false });
    expect(matchesAutomation(failed, "attention", "alpha")).toBe(true);
    expect(matchesAutomation(failed, "enabled", "")).toBe(false);
    expect(matchesAutomation(task("completed", { archived: true }), "all", "")).toBe(false);
    expect(matchesAutomation(task("completed", { archived: true }), "archived", "项目")).toBe(true);
    expect(matchesAutomation(failed, "all", "unrelated")).toBe(false);
    expect(automationManagementPolicy(task("future-state")).canOpenRun).toBe(false);
  });
  it("includes unselected proposals, conflicting observations and blocked sources", () => {
    for (const status of ["proposed", "conflict", "blocked"] as const) {
      const item = task("completed", { attention: {
        status, reason: "Review this source", decision: null, checks_used: 1,
        retries_used: 0, last_checked_at: 1, last_observed_at: null,
        observation_id: null, proposal: null,
      } });
      expect(matchesAutomation(item, "attention", "")).toBe(true);
      expect(automationManagementPolicy(item).attentionBadge).toBeTruthy();
    }
  });
});
