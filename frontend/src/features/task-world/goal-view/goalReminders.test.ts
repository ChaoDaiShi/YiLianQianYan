import { describe, expect, it } from "vitest";
import { GoalReminderLedger, observeGoalRead, subscribeGoalReads, type GoalNoticeSnapshot } from "./goalReminders";
import type { GoalTrackingData } from "./goalTracking";
const state = (phase: GoalNoticeSnapshot["phase"], signature: string = phase): GoalNoticeSnapshot => ({ graphId: "g", title: "报告目标", phase, signature, attention: phase === "failed" ? "error" : phase === "completed" ? "success" : phase === "waiting_approval" ? "warning" : null, message: phase, href: "/task-world/g" });

describe("shared goal reminder policy", () => {
  it("does not replay an older in-flight snapshot after a newer result", () => {
    const ledger = new GoalReminderLedger();
    ledger.observe({ ...state("running"), observedAt: 1 });
    expect(ledger.observe({ ...state("completed"), observedAt: 3 })?.attention).toBe("success");
    expect(ledger.isCurrentObservation({ ...state("running"), observedAt: 2 })).toBe(false);
    expect(ledger.observe({ ...state("failed"), observedAt: 2 })).toBeNull();
    expect(ledger.observe({ ...state("completed"), observedAt: 4 })).toBeNull();
  });
  it("receives file-state changes discovered by page reads even without backend events", () => {
    const ledger = new GoalReminderLedger(); ledger.observe(state("completed"));
    const notices: GoalNoticeSnapshot[] = [];
    const stop = subscribeGoalReads(() => {
      const notice = ledger.observe({ ...state("needs_revalidation"), attention: "warning" });
      if (notice) notices.push(notice);
    });
    observeGoalRead({} as GoalTrackingData); observeGoalRead({} as GoalTrackingData);
    expect(notices).toHaveLength(1); stop();
    observeGoalRead({} as GoalTrackingData); expect(notices).toHaveLength(1);
  });
  it("is quiet on initial observation and unchanged polling", () => {
    const ledger = new GoalReminderLedger();
    expect(ledger.observe(state("waiting_approval"))).toBeNull();
    expect(ledger.observe(state("waiting_approval"))).toBeNull();
  });
  it("notifies once across surfaces for a meaningful transition", () => {
    const ledger = new GoalReminderLedger(); ledger.observe(state("running"));
    expect(ledger.observe(state("failed", "attempt-1"))?.attention).toBe("error");
    expect(ledger.observe(state("failed", "attempt-1"))).toBeNull();
    expect(ledger.observe(state("failed", "attempt-2"))?.attention).toBe("error");
  });
  it("can announce a first live decision event but never ordinary progress", () => {
    const ledger = new GoalReminderLedger();
    expect(ledger.observe(state("waiting_approval"), false, true)?.attention).toBe("warning");
    expect(ledger.observe(state("running"), false, true)).toBeNull();
  });
  it("updates its observed state while muted without replaying stale notices on unmute", () => {
    const ledger = new GoalReminderLedger(); ledger.observe(state("running"));
    expect(ledger.observe(state("completed"), true)).toBeNull();
    expect(ledger.observe(state("completed"), false)).toBeNull();
  });
});
