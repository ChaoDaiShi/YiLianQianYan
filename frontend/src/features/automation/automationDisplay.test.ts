import { afterEach, describe, expect, it } from "vitest";
import type { AutomationOccurrence } from "../../api/automations";
import { mergeOccurrencePages, occurrenceTime, personalStateGuidance, restorePendingPersonalStart, savePendingPersonalStart } from "./automationDisplay";

describe("personal management display and recovery", () => {
  const originalStorage = Object.getOwnPropertyDescriptor(globalThis, "sessionStorage");
  afterEach(() => { if (originalStorage) Object.defineProperty(globalThis, "sessionStorage", originalStorage); else delete (globalThis as { sessionStorage?: unknown }).sessionStorage; });
  it("merges overlapping pages without dropping history or stale status updates", () => {
    const item = (id: string, status = "running") => ({ id, status } as AutomationOccurrence);
    const first = [item("new"), item("overlap")];
    const merged = mergeOccurrencePages(first, [item("overlap", "completed"), item("older")]);
    expect(merged.map(row => row.id)).toEqual(["new", "overlap", "older"]);
    expect(merged[1].status).toBe("completed");
    expect(first[1].status).toBe("running");
  });
  it("retains an exact uncertain start request across view remounts", () => {
    const values = new Map<string, string>();
    Object.defineProperty(globalThis, "sessionStorage", { configurable: true, value: {
      getItem: (key: string) => values.get(key) || null,
      setItem: (key: string, value: string) => { values.set(key, value); },
      removeItem: (key: string) => { values.delete(key); },
    } });
    const request = { expected_revision: 3, request_id: "d7d88da4-088a-4a7b-8585-333333333333", reviewed_occurrence_id: "previous", acknowledged: true };
    savePendingPersonalStart("a", request);
    expect(restorePendingPersonalStart("a")).toEqual(request);
    expect(restorePendingPersonalStart("b")).toBeNull();
    savePendingPersonalStart("a", null);
    expect(restorePendingPersonalStart("a")).toBeNull();
    values.set("yilian.personal.start.v1:a", JSON.stringify({ ...request, expected_revision: -1 }));
    expect(restorePendingPersonalStart("a")).toBeNull();
  });
  it("explains waiting and archive states without suggesting unsafe reruns", () => {
    expect(personalStateGuidance("waiting_approval", false, false)).toContain("不要重新启动");
    expect(personalStateGuidance("waiting_input", false, false)).toContain("同一轮次");
    expect(personalStateGuidance("failed", true, false)).toContain("仍暂停");
    expect(personalStateGuidance("interrupted", false, false)).toContain("不会重放");
    expect(occurrenceTime(1, "invalid/timezone")).not.toContain("Invalid");
  });
});
