import { describe, expect, it } from "vitest";
import { getExecutionDetailPlacement } from "./executionHistoryPlacement";

describe("execution history detail placement", () => {
  it("opens the detail panel to the left of the history card", () => {
    const placement = getExecutionDetailPlacement(
      { left: 1120, top: 180, right: 1360, bottom: 260 },
      { width: 1440, height: 900 },
    );

    expect(placement.left + placement.width).toBeLessThanOrEqual(1108);
    expect(placement.width).toBe(420);
    expect(placement.verticalEdge).toBe("top");
  });

  it("keeps the panel inside a narrow viewport", () => {
    const placement = getExecutionDetailPlacement(
      { left: 250, top: 620, right: 520, bottom: 700 },
      { width: 600, height: 720 },
    );

    expect(placement.left).toBeGreaterThanOrEqual(12);
    expect(placement.left + placement.width).toBeLessThanOrEqual(588);
    expect(placement.verticalEdge).toBe("bottom");
    expect(placement.verticalOffset).toBeGreaterThanOrEqual(12);
  });
});
