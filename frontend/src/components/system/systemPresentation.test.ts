import { describe, expect, it } from "vitest";
import { formatLogLevel, formatLogTimestamp, formatSystemStatus } from "./systemPresentation";

describe("system presentation", () => {
  it("maps known status and keeps unknown status honest", () => {
    expect(formatSystemStatus("healthy")).toEqual({ label: "正常", tone: "success" });
    expect(formatSystemStatus("degraded")).toEqual({ label: "降级", tone: "warning" });
    expect(formatSystemStatus("unavailable")).toEqual({ label: "不可用", tone: "danger" });
    expect(formatSystemStatus("other")).toEqual({ label: "other", tone: "default" });
  });

  it("formats real log level and timestamp values", () => {
    expect(formatLogLevel("error").label).toBe("ERROR");
    expect(formatLogLevel("trace").tone).toBe("default");
    expect(formatLogTimestamp(0)).toContain("1970");
  });
});
