import { create } from "zustand";
import {
  listMemories,
  createMemory,
  updateMemory,
  deleteMemory,
  getMemoryStats,
  extractMemories,
  reindexMemories,
  type MemoryRecord,
  type MemoryReindexResult,
  type MemoryStats,
} from "../api/client";

interface MemoryStore {
  memories: MemoryRecord[];
  stats: MemoryStats | null;
  isLoading: boolean;
  error: string | null;
  isReindexing: boolean;
  reindexResult: MemoryReindexResult | null;

  // Actions
  loadMemories: (query?: { category?: string; source?: string; q?: string }) => Promise<void>;
  addMemory: (data: { content: string; category?: string; source?: string; metadata?: string }) => Promise<MemoryRecord | null>;
  editMemory: (id: string, data: { content?: string; category?: string; metadata?: string }) => Promise<void>;
  removeMemory: (id: string) => Promise<void>;
  loadStats: () => Promise<void>;
  triggerExtraction: (conversationId?: string) => Promise<void>;
  reindexMemoriesAction: () => Promise<void>;
}

export const useMemoryStore = create<MemoryStore>((set, get) => ({
  memories: [],
  stats: null,
  isLoading: false,
  error: null,
  isReindexing: false,
  reindexResult: null,

  loadMemories: async (query) => {
    set({ isLoading: true, error: null });
    try {
      const result = await listMemories(query);
      set({ memories: result || [], isLoading: false });
    } catch (e) {
      set({ error: String(e), isLoading: false });
    }
  },

  addMemory: async (data) => {
    const result = await createMemory(data);
    if (result) {
      set((state) => ({ memories: [result, ...state.memories] }));
      get().loadStats();
    }
    return result;
  },

  editMemory: async (id, data) => {
    const result = await updateMemory(id, data);
    if (result) {
      set((state) => ({
        memories: state.memories.map((m) => (m.id === id ? result : m)),
      }));
      get().loadStats();
    }
  },

  removeMemory: async (id) => {
    await deleteMemory(id);
    set((state) => ({
      memories: state.memories.filter((m) => m.id !== id),
    }));
    get().loadStats();
  },

  loadStats: async () => {
    const result = await getMemoryStats();
    if (result) set({ stats: result });
  },

  triggerExtraction: async (conversationId) => {
    await extractMemories(conversationId);
    // Reload after extraction
    get().loadMemories();
    get().loadStats();
  },

  reindexMemoriesAction: async () => {
    set({ isReindexing: true, reindexResult: null });
    try {
      const result = await reindexMemories();
      set({
        reindexResult: result ?? { ok: false, error: "reindex request failed" },
      });
      await Promise.all([get().loadMemories(), get().loadStats()]);
    } catch (e) {
      set({ reindexResult: { ok: false, error: String(e) } });
    } finally {
      set({ isReindexing: false });
    }
  },
}));
