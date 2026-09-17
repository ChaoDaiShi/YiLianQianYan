import { describe, expect, it, vi } from "vitest";
import taskCenterSource from "./TaskCenterPage.tsx?raw";

type GraphCreationResult =
  | {
      ok: true;
      data: {
        id: string;
        schema_version: number;
        revision: number;
        nodes: [];
        edges: [];
      };
    }
  | {
      ok: false;
      error: { status: number; code: string; message: string };
    };

interface CreationDependencies {
  createGraph: (graph: { id: string; nodes: []; edges: [] }) => Promise<GraphCreationResult>;
  navigate: (path: string) => void;
  setCreating: (creating: boolean) => void;
  setError: (error: { status: number; code: string; message: string } | null) => void;
  inFlight: { current: boolean };
  makeGraphId: () => string;
}

type CreateEmptyTaskGraph = (dependencies: CreationDependencies) => Promise<void>;

async function loadCreateEmptyTaskGraph(): Promise<CreateEmptyTaskGraph> {
  const taskCenterModule = await import("./TaskCenterPage");
  const createEmptyTaskGraph = (taskCenterModule as Record<string, unknown>)
    .createEmptyTaskGraph;
  expect(createEmptyTaskGraph).toBeTypeOf("function");
  return createEmptyTaskGraph as CreateEmptyTaskGraph;
}

function successfulGraph(id: string): GraphCreationResult {
  return {
    ok: true,
    data: { id, schema_version: 1, revision: 1, nodes: [], edges: [] },
  };
}

function creationDependencies(
  createGraph: CreationDependencies["createGraph"],
): CreationDependencies {
  return {
    createGraph,
    navigate: vi.fn(),
    setCreating: vi.fn(),
    setError: vi.fn(),
    inFlight: { current: false },
    makeGraphId: () => "client-request-id",
  };
}

describe("task center page contract", () => {
  it("uses the existing task APIs and keeps task-to-conversation links honest", () => {
    expect(taskCenterSource).toContain("listTasks");
    expect(taskCenterSource).toContain("listTaskGraphs");
    expect(taskCenterSource).toContain("/task-world/");
    expect(taskCenterSource).toContain("TaskDetailPanel");
    expect(taskCenterSource).toContain("navigate(\"/chat\")");
    expect(taskCenterSource).not.toContain("conversation_id");
  });

  it("shows a real empty state with an explicit task canvas creation entry", () => {
    expect(taskCenterSource).toContain("还没有任务画布");
    expect(taskCenterSource).toContain("新建任务画布");
  });

  it("creates at most one empty graph while a request is in flight", async () => {
    const createEmptyTaskGraph = await loadCreateEmptyTaskGraph();
    let finishCreation: ((result: GraphCreationResult) => void) | undefined;
    const createGraph = vi.fn(
      () =>
        new Promise<GraphCreationResult>((resolve) => {
          finishCreation = resolve;
        }),
    );
    const dependencies = creationDependencies(createGraph);

    const firstCreation = createEmptyTaskGraph(dependencies);
    const duplicateCreation = createEmptyTaskGraph(dependencies);

    expect(createGraph).toHaveBeenCalledTimes(1);
    expect(createGraph).toHaveBeenCalledWith({
      id: "client-request-id",
      nodes: [],
      edges: [],
    });
    expect(dependencies.setCreating).toHaveBeenCalledWith(true);
    finishCreation?.(successfulGraph("created-graph"));
    await Promise.all([firstCreation, duplicateCreation]);
    expect(dependencies.setCreating).toHaveBeenLastCalledWith(false);
  });

  it("navigates with the graph id returned by the real API", async () => {
    const createEmptyTaskGraph = await loadCreateEmptyTaskGraph();
    const dependencies = creationDependencies(
      vi.fn().mockResolvedValue(successfulGraph("server-returned-graph")),
    );

    await createEmptyTaskGraph(dependencies);

    expect(dependencies.navigate).toHaveBeenCalledWith(
      "/task-world/server-returned-graph",
    );
  });

  it("shows the real API error and does not navigate when creation fails", async () => {
    const createEmptyTaskGraph = await loadCreateEmptyTaskGraph();
    const apiError = {
      status: 409,
      code: "already_exists",
      message: "任务画布创建失败",
    };
    const dependencies = creationDependencies(
      vi.fn().mockResolvedValue({ ok: false, error: apiError }),
    );

    await createEmptyTaskGraph(dependencies);

    expect(dependencies.setError).toHaveBeenCalledWith(apiError);
    expect(dependencies.navigate).not.toHaveBeenCalled();
  });

  it("keeps existing graph cards linked to their task world route", () => {
    expect(taskCenterSource).toContain("taskGraphs.map((graph)");
    expect(taskCenterSource).toContain(
      "navigate(`/task-world/${encodeURIComponent(graph.id)}`)",
    );
  });
});
