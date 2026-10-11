import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useMemoryStore } from "./memoryStore";
import * as memoryApi from "../api/client";

vi.mock("../api/client", () => ({
  listMemories: vi.fn(),
  createMemory: vi.fn(),
  updateMemory: vi.fn(),
  deleteMemory: vi.fn(),
  getMemoryStats: vi.fn(),
  extractMemories: vi.fn(),
  reindexMemories: vi.fn(),
}));

const mockMemory = {
  id: "mem-1",
  content: "原始记忆",
  category: "fact",
  source: "manual",
  created_at: 1000,
  updated_at: 1000,
};

describe("memoryStore persistence error resilience", () => {
  beforeEach(() => {
    useMemoryStore.setState({
      memories: [],
      stats: null,
      isLoading: false,
      error: null,
      isReindexing: false,
      reindexResult: null,
    });
    vi.clearAllMocks();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("loadMemories preserves existing list when API fails", async () => {
    useMemoryStore.setState({ memories: [mockMemory] });
    vi.mocked(memoryApi.listMemories).mockRejectedValueOnce(
      new Error("本地数据暂时不可用，请重试")
    );

    await useMemoryStore.getState().loadMemories();

    const state = useMemoryStore.getState();
    expect(state.isLoading).toBe(false);
    expect(state.error).toBe("本地数据暂时不可用，请重试");
    // Crucial: previous list is preserved, not overwritten with []
    expect(state.memories).toEqual([mockMemory]);
  });

  it("loadMemories sets empty list when API succeeds with empty array", async () => {
    useMemoryStore.setState({ memories: [mockMemory] });
    vi.mocked(memoryApi.listMemories).mockResolvedValueOnce([]);

    await useMemoryStore.getState().loadMemories();

    const state = useMemoryStore.getState();
    expect(state.isLoading).toBe(false);
    expect(state.error).toBeNull();
    expect(state.memories).toEqual([]);
  });

  it("removeMemory does not remove item from store if deletion fails", async () => {
    useMemoryStore.setState({ memories: [mockMemory] });
    vi.mocked(memoryApi.deleteMemory).mockRejectedValueOnce(
      new Error("删除失败")
    );

    await useMemoryStore.getState().removeMemory("mem-1");

    const state = useMemoryStore.getState();
    expect(state.error).toBe("删除失败");
    expect(state.memories).toEqual([mockMemory]);
  });

  it("removeMemory removes item from store when deletion succeeds", async () => {
    useMemoryStore.setState({ memories: [mockMemory] });
    vi.mocked(memoryApi.deleteMemory).mockResolvedValueOnce({ status: "deleted" });
    vi.mocked(memoryApi.getMemoryStats).mockResolvedValueOnce({
      total: 0,
      by_category: [],
      by_source: [],
    });

    await useMemoryStore.getState().removeMemory("mem-1");

    const state = useMemoryStore.getState();
    expect(state.error).toBeNull();
    expect(state.memories).toEqual([]);
  });

  it("editMemory preserves original item if update fails", async () => {
    useMemoryStore.setState({ memories: [mockMemory] });
    vi.mocked(memoryApi.updateMemory).mockRejectedValueOnce(
      new Error("更新失败")
    );

    await useMemoryStore.getState().editMemory("mem-1", { content: "修改后的记忆" });

    const state = useMemoryStore.getState();
    expect(state.error).toBe("更新失败");
    expect(state.memories[0].content).toBe("原始记忆");
  });

  it("addMemory does not modify list if creation fails", async () => {
    useMemoryStore.setState({ memories: [mockMemory] });
    vi.mocked(memoryApi.createMemory).mockRejectedValueOnce(
      new Error("创建失败")
    );

    const result = await useMemoryStore.getState().addMemory({ content: "新记忆" });

    expect(result).toBeNull();
    const state = useMemoryStore.getState();
    expect(state.error).toBe("创建失败");
    expect(state.memories).toEqual([mockMemory]);
  });
});
