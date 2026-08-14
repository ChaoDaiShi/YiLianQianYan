import { afterEach, describe, expect, it, vi } from "vitest";

import {
  API_BASE,
  MEMORY_RETRIEVAL_MODE_LABELS,
  retrieveMemories,
} from "./client";

vi.mock("./controlSession", () => ({
  controlSessionHeaders: () => ({ "X-Control-Session": "test-session" }),
}));

afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe("memory retrieval surface", () => {
  it("keeps user-facing labels for every backend mode", () => {
    expect(MEMORY_RETRIEVAL_MODE_LABELS.lexical).toBe("词法检索");
    expect(MEMORY_RETRIEVAL_MODE_LABELS.hybrid).toBe("混合检索");
    expect(MEMORY_RETRIEVAL_MODE_LABELS.lexical_fallback).toContain("回退词法检索");
  });

  it("parses retrieval results without exposing raw vectors", async () => {
    const fetchMock = vi.fn(async () =>
      new Response(
        JSON.stringify({
          query: "Rust",
          count: 1,
          mode: "hybrid",
          memories: [{
            id: "memory-1",
            content: "Rust 安全执行",
            category: "knowledge",
            source: "manual",
            score: 0.842,
            lexical_score: 0.9,
            vector_score: 0.8,
          }],
        }),
        { status: 200, headers: { "Content-Type": "application/json" } },
      ),
    );
    vi.stubGlobal("fetch", fetchMock);

    await expect(retrieveMemories("Rust", 5, "knowledge")).resolves.toEqual({
      query: "Rust",
      count: 1,
      mode: "hybrid",
      memories: [{
        id: "memory-1",
        content: "Rust 安全执行",
        category: "knowledge",
        source: "manual",
        score: 0.842,
        lexical_score: 0.9,
        vector_score: 0.8,
      }],
    });
    expect(fetchMock).toHaveBeenCalledWith(
      `${API_BASE}/api/memories/retrieve?q=Rust&top_k=5&category=knowledge`,
      expect.objectContaining({ method: "GET" }),
    );
  });
});
