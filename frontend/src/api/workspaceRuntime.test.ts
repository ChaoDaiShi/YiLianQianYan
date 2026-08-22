import { afterEach, describe, expect, it, vi } from "vitest";

import {
  API_BASE,
  listWorkspaces,
  createWorkspace,
  listTasks,
  createTask,
  startTask,
  listAgents,
  listTeams,
} from "./client";

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

describe("workspace/task runtime API client", () => {
  it("listWorkspaces maps the { workspaces } envelope", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue(
        jsonResponse({
          workspaces: [{ id: "w1", name: "项目", status: "active" }],
        })
      )
    );
    const res = await listWorkspaces();
    expect(res.ok).toBe(true);
    if (res.ok) {
      expect(res.data).toHaveLength(1);
      expect(res.data[0].name).toBe("项目");
    }
  });

  it("createWorkspace posts the payload", async () => {
    const fetchMock = vi.fn().mockResolvedValue(
      jsonResponse({ id: "w1", name: "项目", status: "active" })
    );
    vi.stubGlobal("fetch", fetchMock);
    await createWorkspace({ name: "项目" });
    const [url, opts] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect(url).toBe(`${API_BASE}/api/workspaces`);
    expect(opts.method).toBe("POST");
    expect(JSON.parse(opts.body as string).name).toBe("项目");
  });

  it("listTasks maps the { tasks } envelope and filters by workspace", async () => {
    const fetchMock = vi.fn().mockResolvedValue(
      jsonResponse({ tasks: [{ id: "t1", title: "任务", status: "draft" }] })
    );
    vi.stubGlobal("fetch", fetchMock);
    const res = await listTasks({ workspace_id: "w1" });
    expect(res.ok).toBe(true);
    const [url] = fetchMock.mock.calls[0] as [string];
    expect(url).toContain("workspace_id=w1");
  });

  it("startTask returns a typed run descriptor", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue(
        jsonResponse({
          task_id: "t1",
          task_execution_id: "e1",
          execution_id: "x1",
          status: "running",
        })
      )
    );
    const res = await startTask("t1");
    expect(res.ok).toBe(true);
    if (res.ok) expect(res.data.task_execution_id).toBe("e1");
  });

  it("listAgents and listTeams map their envelopes", async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(jsonResponse({ agents: [{ id: "a1", name: "研究员" }] }))
      .mockResolvedValueOnce(jsonResponse({ teams: [{ id: "team1", name: "团队" }] }));
    vi.stubGlobal("fetch", fetchMock);
    const agents = await listAgents();
    const teams = await listTeams();
    expect(agents.ok).toBe(true);
    expect(teams.ok).toBe(true);
    if (agents.ok) expect(agents.data[0].name).toBe("研究员");
    if (teams.ok) expect(teams.data[0].name).toBe("团队");
  });

  it("surfaces backend validation errors through ApiResult", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue(
        jsonResponse({ error: "任务不存在" }, 404)
      )
    );
    const res = await createTask({ workspace_id: "w1", title: "x" });
    expect(res.ok).toBe(false);
    if (!res.ok) {
      expect(res.status).toBe(404);
      expect(res.error).toBe("任务不存在");
    }
  });
});
