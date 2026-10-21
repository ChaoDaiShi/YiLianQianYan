import { describe, expect, it } from "vitest";
import { localDateInput, usageQuery } from "./usageRange";
describe("monitor local date ranges", () => {
  it("uses local midnight through the next local day and sends east-positive offset", () => {
    const params = new URLSearchParams(usageQuery("2026-10-20", "2026-10-21", "model actual").slice(1));
    const start = new Date("2026-10-20T00:00:00"); const next = new Date("2026-10-22T00:00:00");
    expect(params.get("from")).toBe(String(start.getTime())); expect(params.get("to")).toBe(String(next.getTime()));
    expect(params.get("timezone_offset_minutes")).toBe(String(-start.getTimezoneOffset())); expect(params.get("model_id")).toBe("model actual");
    expect(localDateInput(start)).toBe("2026-10-20");
  });
  it("rejects reverse and oversized ranges", () => { expect(() => usageQuery("2026-10-22", "2026-10-20")).toThrow(); expect(() => usageQuery("2020-01-01", "2026-01-01")).toThrow(); });
});
