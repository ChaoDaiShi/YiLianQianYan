import { renderToStaticMarkup } from "react-dom/server";
import { StaticRouter } from "react-router-dom/server";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { CanvasView, TaskGraphDetail } from "../../api/taskWorld";
import TaskWorldPage from "./TaskWorldPage";
import type { GoalTrackingData } from "./goal-view/goalTracking";

const graphId = "graph-task-summary";
const detail: TaskGraphDetail = {
  graph_id: graphId,
  graph: { id: graphId, schema_version: 1, revision: 7, node_count: 1, edge_count: 0 },
  revision: 7,
  nodes: [{
    id: "collect", kind: "work", title: "提取材料", status: "succeeded",
    state: { status: "succeeded", attempts: 1, result_summary: "已提取三份材料", error: null, started_at: 1, finished_at: 2, updated_at: 2 },
    executor_ref: "capability://collect", instruction_summary: "读取工作空间的材料", acceptance_criteria: [],
    resources: [], validation: { status: "accepted", issues: [] }, result_summary: "已提取三份材料",
  }],
  edges: [], revisions: [], checkpoints: [],
};
const view: CanvasView = {
  schema_version: 1, graph_id: graphId, graph_revision_seen: 7, view_revision: 18,
  viewport: { x: 0, y: 0, zoom: 1 }, node_layouts: [], selection: [], groups: [], updated_at: 0,
};
const tracking: GoalTrackingData = {
  detail, report: null, snapshot: null, currentEvidence: null, artifacts: [], artifactError: null,
  binding: {
    goal_id: "goal-summary", goal_version: 1, graph_id: graphId, conversation_ids: ["approval-chat", "input-chat"],
    conversations: [
      { conversation_id: "approval-chat", title: "核对来源", run_status: "waiting_approval", run_error: null },
      { conversation_id: "input-chat", title: "明确任务范围", run_status: "waiting_input", run_error: null },
    ],
    contract: {
      schema_version: 1, goal_id: "goal-summary", version: 1, workspace_id: "ws-summary", user_goal: "整理已核验的综述",
      deliverables: [{ id: "report", title: "综述.md", mime_type: "text/markdown" }], criteria: [], constraints: [],
      allowed_capability_ids: [], budget: { max_model_calls: 3, max_total_tokens: 5000, max_wall_time_ms: 60000, max_repair_rounds: 1, time_basis: "active_work" },
    },
  },
};

vi.mock("./hooks/useTaskGraph", () => ({ useTaskGraph: () => ({ detail }) }));
vi.mock("./hooks/useCanvasView", () => ({ useCanvasView: () => ({ view, canvasSaveError: { message: "视图写入失败" } }) }));
let goalReadError: string | null = null;
vi.mock("./hooks/useGoalTracking", () => ({ useGoalTracking: () => ({ data: tracking, error: goalReadError }) }));
vi.mock("./canvas/TaskWorldCanvas", () => ({ default: () => <div data-testid="task-world-canvas" /> }));
vi.mock("../voice/GlobalVoiceHost", () => ({ useGlobalVoiceContext: () => ({ updateContext: vi.fn() }) }));

function renderView(mode: "goal" | "expert") {
  vi.stubGlobal("window", { localStorage: { getItem: () => mode } });
  vi.stubGlobal("localStorage", { getItem: () => null });
  return renderToStaticMarkup(<StaticRouter location={`/task-world/${graphId}`}><TaskWorldPage graphId={graphId} /></StaticRouter>);
}

afterEach(() => {
  goalReadError = null;
  vi.unstubAllGlobals();
});

describe("task canvas summaries", () => {
  it("shows the goal overview and keeps approval, input and lifecycle actions available", () => {
    const html = renderView("goal");
    expect(html).toMatch(/data-testid="goal-overview"[^>]*data-phase="waiting_approval"/);
    expect(html).not.toContain('aria-label="共享任务状态"');
    expect(html.match(/class="goal-view-phase goal-view-phase-waiting_approval"/g)).toHaveLength(1);
    expect(html).toContain("综述.md");
    expect(html).toContain("处理审批");
    expect(html).toContain('href="/chat/input-chat"');
    expect(html).toContain("暂停任务");
    expect(html).toContain("取消任务");
  });

  it("shows aggregate progress and keeps each step and its result together in the plan", () => {
    const html = renderView("goal");
    expect(html).toContain("1 / 1 步完成");
    expect(html.match(/>提取材料</g)).toHaveLength(1);
    expect(html).toMatch(/<h3>提取材料<\/h3>[\s\S]*?已提取三份材料[\s\S]*?<span class="goal-view-step-stage">已完成<\/span>/);
  });

  it("shows a goal read error once and leaves cancellation visible", () => {
    goalReadError = "目标读取暂时失败";
    const html = renderView("goal");
    const actions = html.match(/<nav[^>]*aria-label="任务操作">([\s\S]*?)<\/nav>/)?.[1];
    expect(html).toContain('role="alert">目标状态同步失败：目标读取暂时失败</p>');
    expect(actions).not.toContain("目标读取暂时失败");
    expect(actions).toContain("取消任务");
  });

  it("shows only the shared summary above the expert canvas without a direct link to itself", () => {
    const html = renderView("expert");
    expect(html).toContain('aria-label="共享任务状态"');
    expect(html).not.toContain('data-testid="goal-overview"');
    expect(html).toContain('data-testid="task-world-canvas"');
    expect(html).not.toContain("打开任务画布");
    expect(html).toContain("暂停任务");
  });

  it.each(["goal", "expert"] as const)("folds both revision values into technical information in %s view", (mode) => {
    const html = renderView(mode);
    const disclosure = html.match(/<details[^>]*><summary>技术信息<\/summary>([\s\S]*?)<\/details>/)?.[1];
    expect(disclosure).toContain("任务图 ID");
    expect(disclosure).toContain(graphId);
    expect(disclosure).toContain("<dt>语义修订</dt><dd>r7</dd>");
    expect(disclosure).toContain("<dt>视图修订</dt><dd>r18</dd>");
    expect(html).toContain('data-save-state="failed"');
    expect(html).toContain("重试保存画布");
  });
});
