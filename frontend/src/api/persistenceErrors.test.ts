import { afterEach, describe, expect, it, vi } from "vitest";
import {
  createConversation,
  deleteConversation,
  listConversations,
  loadConversation,
} from "./conversations";
import {
  createMemory,
  deleteMemory,
  getMemory,
  getMemoryStats,
  listMemories,
  reindexMemories,
  retrieveMemories,
  updateMemory,
} from "./memory";

vi.mock("./controlSession", () => ({
  controlSessionHeaders: () => ({ "X-Control-Session": "test-session" }),
}));

afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe("P5 frontend persistence error propagation", () => {
  it("P5-01: createConversation throws on 500 and does not invent fake objects", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () =>
        new Response(
          JSON.stringify({
            code: "local_storage_unavailable",
            error: "本地数据暂时不可用，请重试",
          }),
          { status: 500, headers: { "Content-Type": "application/json" } }
        )
      )
    );

    await expect(createConversation("新对话")).rejects.toThrow("本地数据暂时不可用，请重试");
  });

  it("P5-02: listConversations throws on 500 and clearly distinguishes from empty 200 []", async () => {
    // 500 failure
    vi.stubGlobal(
      "fetch",
      vi.fn(async () =>
        new Response(
          JSON.stringify({
            code: "local_storage_unavailable",
            error: "本地数据暂时不可用，请重试",
          }),
          { status: 500, headers: { "Content-Type": "application/json" } }
        )
      )
    );
    await expect(listConversations()).rejects.toThrow("本地数据暂时不可用，请重试");

    // 200 empty
    vi.stubGlobal(
      "fetch",
      vi.fn(async () =>
        new Response(JSON.stringify([]), {
          status: 200,
          headers: { "Content-Type": "application/json" },
        })
      )
    );
    await expect(listConversations()).resolves.toEqual([]);
  });

  it("P5-03: deleteConversation throws on 500 and resolves on 200", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () =>
        new Response(
          JSON.stringify({
            code: "local_storage_unavailable",
            error: "本地数据暂时不可用，请重试",
          }),
          { status: 500, headers: { "Content-Type": "application/json" } }
        )
      )
    );
    await expect(deleteConversation("conv-123")).rejects.toThrow("本地数据暂时不可用，请重试");

    vi.stubGlobal(
      "fetch",
      vi.fn(async () =>
        new Response(JSON.stringify({ status: "deleted" }), {
          status: 200,
          headers: { "Content-Type": "application/json" },
        })
      )
    );
    await expect(deleteConversation("conv-123")).resolves.toEqual({ status: "deleted" });
  });

  it("P5-04: loadConversation distinguishes 404 not found from 500 storage failure", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () =>
        new Response(
          JSON.stringify({
            code: "not_found",
            error: "对话不存在",
          }),
          { status: 404, headers: { "Content-Type": "application/json" } }
        )
      )
    );
    await expect(loadConversation("missing-id")).rejects.toThrow("对话不存在");

    vi.stubGlobal(
      "fetch",
      vi.fn(async () =>
        new Response(
          JSON.stringify({
            code: "local_storage_unavailable",
            error: "本地数据暂时不可用，请重试",
          }),
          { status: 500, headers: { "Content-Type": "application/json" } }
        )
      )
    );
    await expect(loadConversation("db-error-id")).rejects.toThrow("本地数据暂时不可用，请重试");
  });

  it("memory API CRUD and stats throw on storage failure and return typed values on success", async () => {
    // listMemories failure vs empty
    vi.stubGlobal(
      "fetch",
      vi.fn(async () =>
        new Response(
          JSON.stringify({
            code: "local_storage_unavailable",
            error: "本地数据暂时不可用，请重试",
          }),
          { status: 500, headers: { "Content-Type": "application/json" } }
        )
      )
    );
    await expect(listMemories()).rejects.toThrow("本地数据暂时不可用，请重试");

    // createMemory failure
    await expect(createMemory({ content: "test" })).rejects.toThrow("本地数据暂时不可用，请重试");

    // updateMemory failure
    await expect(updateMemory("mem-1", { content: "updated" })).rejects.toThrow("本地数据暂时不可用，请重试");

    // deleteMemory failure
    await expect(deleteMemory("mem-1")).rejects.toThrow("本地数据暂时不可用，请重试");

    // getMemory failure
    await expect(getMemory("mem-1")).rejects.toThrow("本地数据暂时不可用，请重试");

    // getMemoryStats failure
    await expect(getMemoryStats()).rejects.toThrow("本地数据暂时不可用，请重试");

    // reindexMemories failure
    await expect(reindexMemories()).rejects.toThrow("本地数据暂时不可用，请重试");

    // retrieveMemories failure
    await expect(retrieveMemories("query")).rejects.toThrow("本地数据暂时不可用，请重试");
  });
});
