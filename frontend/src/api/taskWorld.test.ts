import { afterEach, describe, expect, it, vi } from "vitest";

vi.mock("./controlSession", () => ({
  controlSessionHeaders: () => ({ "X-Yilian-Control-Session": "a".repeat(64) }),
}));

import {
  addTaskEdge,
  cancelTaskExecution,
  getCanvasView,
  getTaskGraphDetail,
  listTaskNodeExecutions,
  listTaskGraphs,
  rerunTaskFromNode,
  saveCanvasView,
  startTaskExecution,
  updateTaskNode,
  type CanvasView,
} from "./taskWorld";

afterEach(() => vi.unstubAllGlobals());

const graph = {
  schema_version: 1,
  id: "graph-1",
  revision: 3,
  nodes: [],
  edges: [],
};

const graphSummary = {
  id: graph.id,
  schema_version: graph.schema_version,
  revision: graph.revision,
  node_count: graph.nodes.length,
  edge_count: graph.edges.length,
};

const view: CanvasView = {
  schema_version: 1,
  graph_id: "graph-1",
  view_revision: 2,
  graph_revision_seen: 3,
  viewport: { x: 12, y: -4, zoom: 1.1 },
  node_layouts: [],
  selection: [],
  groups: [],
  updated_at: 1_700_000_000,
};

function jsonResponse(body: unknown, status = 200) {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "Content-Type": "application/json" },
  });
}

describe("Task World API", () => {
  it("loads graph summaries and unwraps the graphs envelope", async () => {
    const fetchMock = vi.fn(async () => jsonResponse({ graphs: [graph] }));
    vi.stubGlobal("fetch", fetchMock);

    const result = await listTaskGraphs();

    expect(result).toEqual({ ok: true, data: [graphSummary] });
    expect(fetchMock).toHaveBeenCalledWith(
      expect.stringContaining("/api/task-world/graphs"),
      expect.objectContaining({
        method: "GET",
        headers: expect.objectContaining({
          "X-Yilian-Control-Session": "a".repeat(64),
        }),
      }),
    );
  });

  it("keeps detail and canvas view as separate typed envelopes", async () => {
    const detail = { graph: { ...graph, node_count: 0, edge_count: 0 }, graph_id: graph.id, revision: graph.revision, nodes: [], edges: [], revisions: [], checkpoints: [] };
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(jsonResponse({ detail }))
      .mockResolvedValueOnce(jsonResponse({ view }));
    vi.stubGlobal("fetch", fetchMock);

    await expect(getTaskGraphDetail(graph.id)).resolves.toEqual({ ok: true, data: detail });
    await expect(getCanvasView(graph.id)).resolves.toEqual({ ok: true, data: view });
    expect(fetchMock.mock.calls[0][0]).toContain("/api/task-world/graphs/graph-1/detail");
    expect(fetchMock.mock.calls[1][0]).toContain("/api/task-world/graphs/graph-1/canvas-view");
  });

  it("sends canvas writes with the independent view revision and graph revision seen", async () => {
    const fetchMock = vi.fn(async () => jsonResponse({ view: { ...view, view_revision: 3 } }));
    vi.stubGlobal("fetch", fetchMock);

    const result = await saveCanvasView(graph.id, view);

    expect(result).toEqual({ ok: true, data: { ...view, view_revision: 3 } });
    expect(fetchMock).toHaveBeenCalledWith(
      expect.stringContaining("/api/task-world/graphs/graph-1/canvas-view"),
      expect.objectContaining({
        method: "PUT",
        body: JSON.stringify({
          expected_view_revision: view.view_revision,
          schema_version: view.schema_version,
          view_revision: view.view_revision,
          graph_revision_seen: view.graph_revision_seen,
          viewport: view.viewport,
          node_layouts: view.node_layouts,
          selection: view.selection,
          groups: view.groups,
        }),
      }),
    );
  });

  it("surfaces a stale view revision as a typed conflict instead of swallowing it", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => jsonResponse({ error: "stale_view_revision" }, 409)));

    await expect(saveCanvasView(graph.id, view)).resolves.toEqual({
      ok: false,
      error: {
        status: 409,
        code: "stale_view_revision",
        message: "stale_view_revision",
      },
    });
  });

  it("shows the backend diagnostic while preserving the stable error code", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => jsonResponse({
      error: "task_execution_error",
      message: "task harness node is not ready: node-4",
    }, 400)));

    await expect(startTaskExecution(graph.id, "node-4", 3)).resolves.toEqual({
      ok: false,
      error: {
        status: 400,
        code: "task_execution_error",
        message: "task harness node is not ready: node-4",
      },
    });
  });

  it("uses authoritative graph revision for semantic edits", async () => {
    const fetchMock = vi.fn(async (_input: RequestInfo | URL, _init?: RequestInit) => jsonResponse({ graph }));
    vi.stubGlobal("fetch", fetchMock);

    await addTaskEdge(graph.id, 3, "source", "target");
    await updateTaskNode(graph.id, "source", 3, {
      kind: "work",
      title: "Updated",
      input: { instruction: "Do the work" },
      retry_policy: { max_attempts: 1 },
    });

    expect(fetchMock.mock.calls[0][1]).toEqual(expect.objectContaining({
      method: "POST",
      body: JSON.stringify({ expected_revision: 3, from: "source", to: "target" }),
    }));
    expect(fetchMock.mock.calls[1][1]).toEqual(expect.objectContaining({
      method: "PUT",
      body: JSON.stringify({
        expected_revision: 3,
        kind: "work",
        title: "Updated",
        input: { instruction: "Do the work" },
        retry_policy: { max_attempts: 1 },
      }),
    }));
  });

  it("starts, reads and cancels independent harness attempts", async () => {
    const execution = {
      execution_id: "execution-1",
      attempt: 1,
      status: "dispatching",
      executor_ref: "command://desktop.app.focus",
      validation: null,
      approval_ref: null,
      failure_code: null,
      error: null,
      result_summary: null,
      created_at: 1,
      updated_at: 1,
      started_at: 1,
      finished_at: null,
    } as const;
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(jsonResponse({ execution }))
      .mockResolvedValueOnce(jsonResponse({ executions: [execution] }))
      .mockResolvedValueOnce(jsonResponse({ execution: { ...execution, status: "cancelled" } }));
    vi.stubGlobal("fetch", fetchMock);

    await expect(startTaskExecution(graph.id, "node-1", 3)).resolves.toEqual({
      ok: true,
      data: execution,
    });
    await expect(listTaskNodeExecutions(graph.id, "node-1")).resolves.toEqual({
      ok: true,
      data: [execution],
    });
    await expect(cancelTaskExecution(graph.id, execution.execution_id, 3)).resolves.toEqual({
      ok: true,
      data: { ...execution, status: "cancelled" },
    });
    expect(fetchMock.mock.calls[0][0]).toContain("/nodes/node-1/executions");
    expect(fetchMock.mock.calls[0][1]).toEqual(expect.objectContaining({
      method: "POST",
      body: JSON.stringify({ expected_revision: 3 }),
    }));
    expect(fetchMock.mock.calls[2][1]).toEqual(expect.objectContaining({
      method: "POST",
      body: JSON.stringify({ expected_revision: 3 }),
    }));
  });

  it("requests dependent-only partial reruns", async () => {
    const fetchMock = vi.fn(async (_input: RequestInfo | URL, _init?: RequestInit) => jsonResponse({
      graph_id: graph.id,
      node_id: "source",
      affected_nodes: ["source", "dependent"],
    }));
    vi.stubGlobal("fetch", fetchMock);

    await expect(rerunTaskFromNode(graph.id, "source", 3)).resolves.toEqual({
      ok: true,
      data: { graph_id: graph.id, node_id: "source", affected_nodes: ["source", "dependent"] },
    });
    expect(fetchMock.mock.calls[0][0]).toContain("/api/task-world/graphs/graph-1/rerun");
    expect(fetchMock.mock.calls[0][1]).toEqual(expect.objectContaining({
      method: "POST",
      body: JSON.stringify({ expected_revision: 3, node_id: "source" }),
    }));
  });
});
