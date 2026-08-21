import { describe, expect, it } from "vitest";
import historySource from "./ExecutionHistory.tsx?raw";

describe("execution history presentation", () => {
  it("uses semantic action summaries in the primary layer", () => {
    expect(historySource).toContain(
      "formatToolActionSummary(record.name, record.args)",
    );
    expect(historySource).not.toContain(
      "record.startedAt !== undefined\n                  ? formatElapsed",
    );
  });

  it("supports accessible expandable technical details", () => {
    expect(historySource).toContain("aria-expanded={expanded}");
    expect(historySource).toContain("Tool Call ID");
    expect(historySource).toContain("Arguments");
    expect(historySource).toContain("错误日志");
  });

  it("keeps call identifiers inside technical details", () => {
    const actionIndex = historySource.indexOf("execution-history-action");
    const idIndex = historySource.indexOf("Tool Call ID");
    expect(actionIndex).toBeGreaterThan(-1);
    expect(idIndex).toBeGreaterThan(actionIndex);
  });
});
