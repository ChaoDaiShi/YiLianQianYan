import { describe, expect, it } from "vitest";
import canvasShellSource from "./canvas/TaskWorldCanvas.tsx?raw";
import taskNodeSource from "./canvas/TaskNode.tsx?raw";
import pageShellSource from "./TaskWorldPage.tsx?raw";
import useTaskGraphSource from "./hooks/useTaskGraph.ts?raw";
import useCanvasViewSource from "./hooks/useCanvasView.ts?raw";
import useTaskEventsSource from "./hooks/useTaskEvents.ts?raw";
import useTaskReviewSource from "./hooks/useTaskReview.ts?raw";
import useTaskCommandsSource from "./hooks/useTaskCommands.ts?raw";
import trailSource from "./TaskExecutionTrail.tsx?raw";
import inspectorShellSource from "./inspector/TaskInspector.tsx?raw";
import basicSectionSource from "./inspector/BasicSection.tsx?raw";
import executorSectionSource from "./inspector/ExecutorSection.tsx?raw";
import acceptanceSectionSource from "./inspector/AcceptanceSection.tsx?raw";
import stateSectionSource from "./inspector/StateSection.tsx?raw";
import executionSectionSource from "./inspector/ExecutionSection.tsx?raw";
import resourceSectionSource from "./inspector/ResourceSection.tsx?raw";
import artifactSectionSource from "./inspector/ArtifactSection.tsx?raw";
import dependencySectionSource from "./inspector/DependencySection.tsx?raw";
import versionSectionSource from "./inspector/VersionSection.tsx?raw";

// The canvas surface is its React Flow host plus the node component it renders.
const canvasSource = [canvasShellSource, taskNodeSource].join("\n");

// The page is composition plus its behaviour hooks. These assertions are about
// the surface's behaviour wherever it now lives, so they read all of it.
const pageSource = [
  pageShellSource,
  useTaskGraphSource,
  useCanvasViewSource,
  useTaskEventsSource,
  useTaskReviewSource,
  useTaskCommandsSource,
].join("\n");

// The inspector is one surface split across its shell and its sections; these
// assertions are about that surface, so they read all of it.
const inspectorSource = [
  inspectorShellSource,
  basicSectionSource,
  executorSectionSource,
  acceptanceSectionSource,
  stateSectionSource,
  executionSectionSource,
  resourceSectionSource,
  artifactSectionSource,
  dependencySectionSource,
  versionSectionSource,
].join("\n");

describe("Task World surface contract", () => {
  it("uses React Flow for an infinite, selectable canvas", () => {
    expect(canvasSource).toContain("@xyflow/react");
    expect(canvasSource).toContain("ReactFlowProvider");
    expect(canvasSource).toContain("onNodeDragStop");
    expect(canvasSource).toContain("onConnect");
    expect(canvasSource).toContain("onNodesDelete");
    expect(canvasSource).toContain(
      'deleteKeyCode={semanticLocked ? null : ["Backspace", "Delete"]}',
    );
    expect(canvasSource).toContain("nodesConnectable={!semanticLocked}");
    expect(canvasSource).toContain('change.type !== "remove"');
    expect(canvasSource).toContain("fitView");
    expect(canvasSource).toContain("task-world-node");
  });

  it("focuses trail items through the same node ids as the canvas", () => {
    expect(trailSource).toContain("buildExecutionTrail");
    expect(trailSource).toContain("onFocus(item.nodeId)");
    expect(pageSource).toContain("focusedNodeId");
    expect(pageSource).toContain("onFocusNode");
  });

  it("refreshes task.node.running through the event stream without page reload", () => {
    expect(pageSource).toContain("subscribeToEvents");
    expect(pageSource).toContain('event.type === "task.node.running"');
    expect(pageSource).toContain("refreshDetail");
    expect(pageSource).not.toContain("window.location.reload");
  });

  it("keeps semantic edits and stale revision handling in the inspector boundary", () => {
    expect(inspectorSource).toContain("acceptance_criteria");
    expect(inspectorSource).toContain("executor_ref");
    expect(inspectorSource).toContain("stale_view_revision");
    expect(inspectorSource).toContain("expectedRevision");
    expect(pageSource).toContain("updateTaskNode");
    expect(pageSource).toContain("saveCanvasView");
  });

  it("renders authoritative execution state and attempt history", () => {
    expect(canvasSource).toContain("data-execution-status");
    expect(inspectorSource).toContain("latest_execution");
    expect(inspectorSource).toContain("execution_history");
    expect(trailSource).toContain("execution_status");
    expect(pageSource).toContain("startTaskExecution");
  });

  it("routes configured executors through the task harness without desktop runtime coupling", () => {
    expect(inspectorSource).toContain("getExecutorAvailability");
    expect(pageSource).toContain("startTaskExecution");
    expect(inspectorSource).not.toContain("desktop.app");
    expect(pageSource).not.toContain("desktop.app");
  });

  it("offers persisted visual grouping, collapse and auto layout without semantic edits", () => {
    expect(pageSource).toContain("创建分组");
    expect(canvasSource).toContain("自动布局");
    expect(pageSource).toContain("group.collapsed");
    expect(pageSource).toContain("groups:");
  });

  it("keeps AI graph review as user-accepted suggestions", () => {
    expect(pageSource).toContain("AI 审查");
    expect(pageSource).toContain("接受建议");
    expect(pageSource).toContain("拒绝建议");
    expect(pageSource).toContain("reviewTaskGraph");
    expect(pageSource).toContain("reviewed_revision");
  });
});
