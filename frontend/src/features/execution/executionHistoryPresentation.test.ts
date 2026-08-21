import { describe, expect, it } from "vitest";
// @ts-expect-error -- Node file access is test-only and not bundled.
import { readFileSync } from "node:fs";
import historySource from "./ExecutionHistory.tsx?raw";

const css = readFileSync(new URL("../../index.css", import.meta.url), "utf8");

function rule(selector: string): string {
  const escaped = selector.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  return css.match(new RegExp(`${escaped}\\s*\\{([\\s\\S]*?)\\}`))?.[1] ?? "";
}

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
    expect(historySource).toContain("createPortal");
    expect(historySource).toContain('role="dialog"');
    expect(historySource).toContain('aria-modal="false"');
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

  it("contains long unbroken technical values inside the floating panel", () => {
    expect(rule(".execution-history-details")).toContain("max-width: calc(100vw - 24px)");
    expect(rule(".execution-history-details-scroll")).toContain("min-width: 0");
    expect(rule(".execution-history-technical-value")).toContain("max-width: 100%");
    expect(rule(".execution-history-technical-value")).toContain("overflow-wrap: anywhere");
    expect(rule(".execution-history-technical-value")).toContain("word-break: break-word");
  });
});
