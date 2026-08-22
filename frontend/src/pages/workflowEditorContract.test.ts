import { describe, expect, it } from "vitest";
import workflowsPageSource from "./WorkflowsPage.tsx?raw";
import workflowEditorSource from "../components/workflow/WorkflowGraphEditor.tsx?raw";

describe("workflow editor page contract", () => {
  it("keeps the existing graph APIs and run inspector wiring", () => {
    expect(workflowsPageSource).toContain('useState<Tab>("runtime")');
    expect(workflowsPageSource).toContain("RuntimeWorkflowList");
    expect(workflowsPageSource).toContain("WorkflowGraphEditor");
    expect(workflowsPageSource).toContain("startWorkflowRun");
    expect(workflowsPageSource).toContain("WorkflowRunInspector");
    expect(workflowEditorSource).toContain("createWorkflowGraph");
    expect(workflowEditorSource).toContain("updateWorkflowGraph");
    expect(workflowEditorSource).toContain("validateGraphClientSide");
    expect(workflowEditorSource).toContain("onRun");
  });

  it("keeps canvas space and the three editor regions responsive", () => {
    expect(workflowEditorSource).toContain("workflow-editor-library");
    expect(workflowEditorSource).toContain("workflow-editor-main");
    expect(workflowEditorSource).toContain("workflow-editor-inspector");
    expect(workflowEditorSource).toContain("workflow-canvas-viewport scrollbar-thin");
    expect(workflowEditorSource).toContain("minHeight: layout.height");
  });

  it("does not add a second workflow runtime or persisted canvas schema", () => {
    expect(workflowEditorSource).not.toContain("progress");
    expect(workflowEditorSource).not.toContain("estimated");
    expect(workflowEditorSource).not.toContain("position:");
    expect(workflowEditorSource).toContain("节点位置仅用于当前页面展示");
  });
});
