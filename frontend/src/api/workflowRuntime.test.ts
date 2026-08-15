import { afterEach, describe, expect, it, vi } from "vitest";

import {
  API_BASE,
  listWorkflowGraphs,
  createWorkflowGraph,
  startWorkflowRun,
  getWorkflowRun,
  listWorkflowRuns,
  cancelWorkflowRun,
  streamWorkflowApprovalDecision,
  type WorkflowGraphDefinition,
  type WorkflowRunRecord,
} from "./client";
import { validateGraphClientSide } from "../components/workflow/WorkflowGraphEditor";

vi.mock("./controlSession", () => ({
  controlSessionHeaders: () => ({ "X-Control-Session": "test-session" }),
}));

afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "Content-Type": "application/json" },
  });
}

function outputDefinition(): WorkflowGraphDefinition {
  return {
    schema_version: 1,
    entry_node_id: "done",
    nodes: [
      {
        id: "done",
        kind: "output",
        config: { type: "output", template: null },
      },
    ],
    edges: [],
  };
}

describe("workflow runtime API client", () => {
  it("listWorkflowGraphs maps the { graphs } envelope", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () =>
        jsonResponse({
          graphs: [
            {
              id: "g1",
              name: "研究",
              description: "",
              definition: outputDefinition(),
              created_at: 1,
              updated_at: 2,
            },
          ],
        })
      )
    );
    const res = await listWorkflowGraphs();
    expect(res.ok).toBe(true);
    if (res.ok) {
      expect(res.data).toHaveLength(1);
      expect(res.data[0].name).toBe("研究");
    }
    expect(fetch).toHaveBeenCalledWith(`${API_BASE}/api/workflow-graphs`, expect.anything());
  });

  it("createWorkflowGraph surfaces the backend DAG validation error", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () =>
        jsonResponse({ error: "cycle detected involving node: a" }, 400)
      )
    );
    const res = await createWorkflowGraph({
      name: "bad",
      description: "",
      definition: outputDefinition(),
    });
    expect(res.ok).toBe(false);
    if (!res.ok) {
      expect(res.status).toBe(400);
      expect(res.error).toContain("cycle detected");
    }
  });

  it("createWorkflowGraph surfaces network failure as a non-ok result", async () => {
    vi.spyOn(console, "error").mockImplementation(() => undefined);
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => Promise.reject(new Error("offline")))
    );
    const res = await createWorkflowGraph({
      name: "x",
      description: "",
      definition: outputDefinition(),
    });
    expect(res.ok).toBe(false);
    if (!res.ok) expect(res.status).toBe(0);
  });

  it("startWorkflowRun returns the run id immediately", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () =>
        jsonResponse({ run_id: "run-1", execution_id: "exec-1", status: "created" })
      )
    );
    const res = await startWorkflowRun("g1");
    expect(res.ok).toBe(true);
    if (res.ok) {
      expect(res.data.run_id).toBe("run-1");
      expect(res.data.status).toBe("created");
    }
    expect(fetch).toHaveBeenCalledWith(
      `${API_BASE}/api/workflow-graphs/g1/run`,
      expect.objectContaining({ method: "POST" })
    );
  });

  it("getWorkflowRun returns node results", async () => {
    const run: WorkflowRunRecord = {
      run_id: "run-1",
      workflow_graph_id: "g1",
      execution_id: "exec-1",
      subject_id: "local-user",
      status: "completed",
      created_at: 1,
      updated_at: 2,
      nodes: [
        {
          node_id: "done",
          status: "completed",
          result: { summary: "工作流执行完成" },
        },
      ],
    };
    vi.stubGlobal("fetch", vi.fn(async () => jsonResponse(run)));
    const res = await getWorkflowRun("run-1");
    expect(res.ok).toBe(true);
    if (res.ok) {
      expect(res.data.status).toBe("completed");
      expect(res.data.nodes[0].result?.summary).toBe("工作流执行完成");
    }
  });

  it("listWorkflowRuns sends limit/status params and maps runs", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () =>
        jsonResponse({
          runs: [
            {
              run_id: "run-2",
              status: "failed",
              execution_id: "e",
              subject_id: "local-user",
              created_at: 3,
              updated_at: 4,
              nodes: [],
            },
          ],
        })
      )
    );
    const res = await listWorkflowRuns({ status: "failed", limit: 20 });
    expect(res.ok).toBe(true);
    if (res.ok) expect(res.data[0].status).toBe("failed");
    expect(fetch).toHaveBeenCalledWith(
      `${API_BASE}/api/workflow-runs?status=failed&limit=20`,
      expect.anything()
    );
  });

  it("cancelWorkflowRun returns the cancelled run", async () => {
    const cancelled: WorkflowRunRecord = {
      run_id: "run-1",
      workflow_graph_id: "g1",
      execution_id: "e",
      subject_id: "local-user",
      status: "cancelled",
      created_at: 1,
      updated_at: 5,
      nodes: [],
    };
    vi.stubGlobal("fetch", vi.fn(async () => jsonResponse(cancelled)));
    const res = await cancelWorkflowRun("run-1");
    expect(res.ok).toBe(true);
    if (res.ok) expect(res.data.status).toBe("cancelled");
    expect(fetch).toHaveBeenCalledWith(
      `${API_BASE}/api/workflow-runs/run-1/cancel`,
      expect.objectContaining({ method: "POST" })
    );
  });

  it("streams approval_resolved then workflow_run_updated", async () => {
    const sseBody = [
      "event: approval_resolved",
      `data: ${JSON.stringify({ type: "approval_resolved", approval_id: "a1", status: "approved" })}`,
      "",
      "event: workflow_run_updated",
      `data: ${JSON.stringify({ type: "workflow_run_updated", workflow_run_id: "run-1", status: "completed" })}`,
      "",
    ].join("\n");
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => new Response(sseBody, { status: 200 }))
    );

    const events: Array<{ type: string }> = [];
    await streamWorkflowApprovalDecision("/api/approvals/a1/approve", (e) =>
      events.push(e)
    );

    expect(events.map((e) => e.type)).toEqual([
      "approval_resolved",
      "workflow_run_updated",
    ]);
  });

  it("emits workflow_approval_error on a failed decision HTTP response", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () =>
        jsonResponse({ error: "审批已被处理，不能重复操作" }, 409)
      )
    );
    const events: Array<{ type: string; error?: string }> = [];
    await streamWorkflowApprovalDecision("/api/approvals/a1/approve", (e) =>
      events.push(e)
    );
    expect(events[0].type).toBe("workflow_approval_error");
    expect((events[0] as { error?: string }).error).toContain("不能重复操作");
  });
});

describe("validateGraphClientSide", () => {
  it("accepts a valid output graph", () => {
    expect(validateGraphClientSide(outputDefinition())).toBeNull();
  });

  it("rejects an empty node list", () => {
    const def = { ...outputDefinition(), nodes: [] };
    expect(validateGraphClientSide(def)).toContain("至少需要一个节点");
  });

  it("rejects duplicate node ids", () => {
    const def = {
      ...outputDefinition(),
      nodes: [
        { id: "a", kind: "output" as const, config: { type: "output" as const, template: null } },
        { id: "a", kind: "output" as const, config: { type: "output" as const, template: null } },
      ],
    };
    expect(validateGraphClientSide(def)).toContain("重复的节点 ID");
  });

  it("rejects edges referencing missing nodes", () => {
    const def: WorkflowGraphDefinition = {
      schema_version: 1,
      entry_node_id: "a",
      nodes: [
        { id: "a", kind: "output", config: { type: "output", template: null } },
        { id: "b", kind: "output", config: { type: "output", template: null } },
      ],
      edges: [{ from: "a", to: "ghost" }],
    };
    expect(validateGraphClientSide(def)).toContain("不存在的节点");
  });
});
