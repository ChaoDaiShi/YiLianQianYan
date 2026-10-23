import { describe, expect, it } from "vitest";
import workflowsPageSource from "./WorkflowsPage.tsx?raw";
import workflowEditorSource from "../components/workflow/WorkflowGraphEditor.tsx?raw";
import workflowRunDialogSource from "../features/workflows/WorkflowRunDialog.tsx?raw";

describe("workflow editor page contract", () => {
  it("keeps the existing graph APIs and run inspector wiring", () => {
    expect(workflowsPageSource).toContain('useState<Tab>("runtime")');
    expect(workflowsPageSource).toContain("RuntimeWorkflowList");
    expect(workflowsPageSource).toContain("WorkflowGraphEditor");
    expect(workflowsPageSource).toMatch(/<RuntimeWorkflowList\b[\s\S]*?onRun=\{handleRun\}/);
    expect(workflowsPageSource).toMatch(/<WorkflowGraphEditor\b[\s\S]*?onRun=\{handleRun\}/);
    expect(workflowsPageSource).toMatch(/const handleRun[\s\S]*?setRunGraph\(graph\)/);
    expect(workflowsPageSource).toMatch(/<WorkflowRunDialog\b[^>]*graph=\{runGraph\}/);
    expect(workflowRunDialogSource).toContain('from "../../api/workflows"');
    expect(workflowRunDialogSource).toContain("await startWorkflowRun(graph.id, inputs)");
    expect(workflowRunDialogSource).toContain("if (response.ok) onStarted(response.data.run_id)");
    expect(workflowRunDialogSource).toContain("else setError(response.error)");
    expect(workflowsPageSource).toMatch(/onStarted=\{id\s*=>\s*\{[\s\S]*?setActiveRunId\(id\)/);
    expect(workflowsPageSource).toMatch(/<WorkflowRunInspector\b[^>]*runId=\{activeRunId\}/);
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
