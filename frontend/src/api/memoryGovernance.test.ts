import { afterEach, describe, expect, it, vi } from "vitest";
import { correctMemory, getMemoryHistory, withdrawMemory } from "./memory";
import { requestResult } from "../core/api/http";

vi.mock("../core/api/http", () => ({ requestResult: vi.fn() }));
afterEach(() => vi.resetAllMocks());

describe("memory governance requests", () => {
  it("sends the reviewed revision and explicit reason with a correction", async () => {
    vi.mocked(requestResult).mockResolvedValueOnce({ ok: true, data: { id: "memory", content: "不再喜欢咖啡" } });
    const payload = { content: "不再喜欢咖啡", expected_revision: 7, reason: "用户纠正", scope: "profile" };
    await correctMemory("memory", payload);
    expect(requestResult).toHaveBeenCalledWith("POST", "/api/memories/memory/correct", payload);
  });

  it("does not treat a conflicting withdrawal as persisted", async () => {
    vi.mocked(requestResult).mockResolvedValueOnce({ ok: false, error: "memory_revision_conflict", status: 409 });
    await expect(withdrawMemory("memory", { expected_revision: 2, reason: "过时" })).rejects.toThrow("memory_revision_conflict");
  });

  it("retrieves history separately from the active memory list", async () => {
    const history = [{ revision: 1, content: "旧偏好", status: "active" }];
    vi.mocked(requestResult).mockResolvedValueOnce({ ok: true, data: history });
    await expect(getMemoryHistory("memory")).resolves.toEqual(history);
    expect(requestResult).toHaveBeenCalledWith("GET", "/api/memories/memory/history");
  });
});
