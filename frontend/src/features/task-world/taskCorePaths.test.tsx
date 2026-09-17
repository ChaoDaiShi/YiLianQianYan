import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import TaskWorldInspector from "./TaskWorldInspector";
import type { TaskNodeProjection } from "./taskGraphProjection";
import pageSource from "./TaskWorldPage.tsx?raw";

function inspector(status: "runnable" | "invalidated", executor_ref: string | null) {
  const node = {
    id: "one", kind: "work", title: "Editable", status, executor_ref,
    state: { status, attempts: 0, result_summary: null, error: null, started_at: null, finished_at: null, updated_at: 1 },
    instruction_summary: "", acceptance_criteria: [], resources: [], validation: { status: "pending", issues: [] },
    result_summary: null, latest_execution: null, execution_history: [], role: "Task", isRunning: false,
  } as TaskNodeProjection;
  const noop = async () => {};
  return renderToStaticMarkup(createElement(TaskWorldInspector, {
    graphId: "graph-test", node, expectedRevision: 1, revisions: [], checkpoints: [], dependencyEdges: [], dependencyCandidates: [],
    onSave: noop, onStart: noop, onStartExecution: noop, onRerun: noop, onCheckpoint: noop,
    onRestore: noop, onAddDependency: noop, onRemoveDependency: noop, graphLocked: false,
  }));
}

describe("editable TaskGraph core paths", () => {
  it("disables execution for an unconfigured node while keeping semantic editing", () => {
    const html = inspector("runnable", null);
    expect(html).toMatch(/<button[^>]*disabled=""[^>]*>开始执行<\/button>/);
    expect(html).toContain("保存语义");
    expect(html).not.toContain("标记为运行中");
  });

  it("offers validation and rerun recovery for an invalidated node with no attempts", () => {
    const html = inspector("invalidated", "workflow://real-workflow");
    expect(html).toContain("校验并准备重跑");
    expect(html).toContain("不会撤销已发生的外部操作");
  });

  it("exposes a node creation entry on an empty canvas", () => {
    expect(pageSource).toContain("addTaskNode(");
    expect(pageSource).toContain("添加任务节点");
  });
});
