import { describe, expect, it } from "vitest";
import { nodePresentation } from "./nodePresentation";
import type { TaskNodeProjection } from "../taskGraphProjection";

const node = (values: Partial<TaskNodeProjection>) => ({ kind: "work", role: "Task", executor_ref: null, ...values }) as TaskNodeProjection;

describe("Studio node presentation boundaries", () => {
  it("keeps approval and human gates distinct even when they are graph entries", () => {
    expect(nodePresentation(node({ kind: "approval" }), true).kind).toBe("approval");
    expect(nodePresentation(node({ kind: "user_checkpoint" }), true).kind).toBe("human");
  });
  it("labels entry and output roles without introducing backend kinds", () => {
    expect(nodePresentation(node({}), true).label).toBe("入口任务");
    expect(nodePresentation(node({ role: "Output" })).kind).toBe("output");
    expect(nodePresentation(node({})).executor).toBe("未配置执行器");
  });
  it("marks MCP as a reservation, never an available execution capability", () => {
    expect(nodePresentation(node({ executor_ref: "mcp://future" }))).toEqual({ kind: "mcp", label: "MCP · 预留", executor: "能力未接入" });
    expect(nodePresentation(node({ executor_ref: "workflow://local" })).executor).toBe("工作流");
  });
});
