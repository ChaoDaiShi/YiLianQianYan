import { describe, expect, it } from "vitest";
import canvasSource from "./TaskWorldCanvas.tsx?raw";
import pageSource from "./TaskWorldPage.tsx?raw";
import trailSource from "./TaskExecutionTrail.tsx?raw";
import inspectorSource from "./TaskWorldInspector.tsx?raw";

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
});
