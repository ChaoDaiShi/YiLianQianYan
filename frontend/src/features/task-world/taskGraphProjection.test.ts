import { describe, expect, it } from "vitest";
import {
  buildExecutionTrail,
  deriveTaskRole,
  isTaskWorldEvent,
  projectTaskGraph,
  toReactFlowModel,
  getExecutorAvailability,
} from "./taskGraphProjection";
import * as taskGraphProjection from "./taskGraphProjection";
import type { CanvasView, TaskGraphDetail } from "../../api/taskWorld";

const detail: TaskGraphDetail = {
  graph: {
    id: "graph-1",
    schema_version: 1,
    revision: 7,
    node_count: 3,
    edge_count: 2,
  },
  graph_id: "graph-1",
  revision: 7,
  nodes: [
    {
      id: "prepare",
      kind: "work",
      title: "准备资料",
      status: "succeeded",
      state: {
        status: "succeeded",
        attempts: 1,
        result_summary: "资料已整理",
        error: null,
        started_at: 10,
        finished_at: 12,
        updated_at: 12,
      },
      executor_ref: "command://desktop.app.open",
      instruction_summary: "打开资料目录",
      acceptance_criteria: ["目录可见"],
      resources: [{ id: "folder", name: "资料", uri: null }],
      validation: { status: "not_checked", issues: [] },
      result_summary: "资料已整理",
      latest_execution: {
        execution_id: "execution-1",
        attempt: 2,
        status: "dispatching",
        executor_ref: "command://desktop.app.open",
        validation: null,
        approval_ref: null,
        failure_code: null,
        error: null,
        result_summary: null,
        created_at: 20,
        updated_at: 21,
        started_at: 21,
        finished_at: null,
      },
      execution_history: [],
    },
    {
      id: "review",
      kind: "approval",
      title: "确认资料",
      status: "runnable",
      state: {
        status: "runnable",
        attempts: 0,
        result_summary: null,
        error: null,
        started_at: null,
        finished_at: null,
        updated_at: 13,
      },
      executor_ref: null,
      instruction_summary: "请确认资料",
      acceptance_criteria: [],
      resources: [],
      validation: { status: "not_checked", issues: [] },
      result_summary: null,
    },
    {
      id: "finish",
      kind: "user_checkpoint",
      title: "完成确认",
      status: "pending",
      state: {
        status: "pending",
        attempts: 0,
        result_summary: null,
        error: null,
        started_at: null,
        finished_at: null,
        updated_at: 11,
      },
      executor_ref: null,
      instruction_summary: "等待你的确认",
      acceptance_criteria: [],
      resources: [],
      validation: { status: "not_checked", issues: [] },
      result_summary: null,
    },
  ],
  edges: [
    { from: "prepare", to: "review" },
    { from: "review", to: "finish" },
  ],
  revisions: [],
  checkpoints: [],
};

const view: CanvasView = {
  schema_version: 1,
  graph_id: "graph-1",
  view_revision: 2,
  graph_revision_seen: 7,
  viewport: { x: 0, y: 0, zoom: 1 },
  node_layouts: [
    { node_id: "prepare", x: 100, y: 120, width: 240, height: 128 },
    { node_id: "review", x: 420, y: 120, width: 240, height: 128 },
    { node_id: "finish", x: 740, y: 120, width: 240, height: 128 },
  ],
  selection: ["review"],
  groups: [],
  updated_at: 14,
};

describe("TaskGraph projection boundary", () => {
  it("projects public graph state into bounded visual roles and React Flow nodes", () => {
    const projection = projectTaskGraph(detail);
    const model = toReactFlowModel(projection, view, "review");

    expect(projection.nodes.map((node) => node.id)).toEqual(["prepare", "review", "finish"]);
    expect(projection.nodes.map((node) => node.role)).toEqual(["Task", "Decision", "Human"]);
    expect(projection.nodes[0].latest_execution?.attempt).toBe(2);
    expect(projection.nodes[0].isRunning).toBe(true);
    expect(new Set(projection.nodes.map((node) => node.role)).size).toBeLessThanOrEqual(5);
    expect(model.nodes[0]).toMatchObject({ id: "prepare", position: { x: 100, y: 120 } });
    expect(model.nodes.find((node) => node.id === "review")?.data.isFocused).toBe(true);
    expect(model.edges).toEqual([
      { id: "prepare->review", source: "prepare", target: "review", type: "smoothstep" },
      { id: "review->finish", source: "review", target: "finish", type: "smoothstep" },
    ]);
  });

  it("derives the execution trail from the same projected node objects", () => {
    const projection = projectTaskGraph(detail);
    const trail = buildExecutionTrail(projection);

    expect(trail.map((item) => item.nodeId)).toEqual(["review", "prepare", "finish"]);
    expect(trail[0].node).toBe(projection.nodes.find((node) => node.id === "review"));
  });

  it("hides collapsed visual-group members without mutating the semantic graph", () => {
    const projection = projectTaskGraph(detail);
    const groupedView = {
      ...view,
      groups: [{ id: "group-1", title: "准备阶段", node_ids: ["prepare", "review"], collapsed: true }],
    } satisfies CanvasView;

    const model = toReactFlowModel(projection, groupedView, null);

    expect(model.nodes.map((node) => node.id)).toEqual(["finish"]);
    expect(model.edges).toEqual([]);
    expect(projection.nodes.map((node) => node.id)).toEqual(["prepare", "review", "finish"]);
  });

  it("builds a deterministic dependency-aware auto layout", () => {
    const candidate = (taskGraphProjection as unknown as Record<string, unknown>).buildAutoLayout;
    expect(typeof candidate).toBe("function");
    if (typeof candidate !== "function") return;

    expect(candidate(projectTaskGraph(detail))).toEqual([
      { node_id: "prepare", x: 40, y: 40, width: 240, height: 128 },
      { node_id: "review", x: 360, y: 40, width: 240, height: 128 },
      { node_id: "finish", x: 680, y: 40, width: 240, height: 128 },
    ]);
  });

  it("matches task events by graph id without applying supervisor transitions", () => {
    expect(isTaskWorldEvent({ type: "task.node.running", payload: { graph_id: "graph-1" } }, "graph-1")).toBe(true);
    expect(isTaskWorldEvent({ type: "task.node.running", payload: { graph_id: "other" } }, "graph-1")).toBe(false);
    expect(isTaskWorldEvent({ type: "approval_required", payload: { graph_id: "graph-1" } }, "graph-1")).toBe(false);
  });

  it("marks an unavailable desktop provider as not installed", () => {
    expect(deriveTaskRole(detail.nodes[0])).toBe("Task");
    expect(getExecutorAvailability("command://desktop.app.open")).toEqual({
      kind: "unavailable",
      label: "未安装",
      reason: "v1 未安装桌面控制 Provider；该执行引用不会被调度。",
    });
  });
});
