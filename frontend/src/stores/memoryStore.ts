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
  editMemory: (id: string, data: { content?: string; category?: string; metadata?: string }) => Promise<boolean>;
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
      const memories = await listMemories(query);
      set({ memories, isLoading: false });
    } catch (error) {
      set({ isLoading: false, error: error instanceof Error ? error.message : String(error) });
    }
  },

  addMemory: async (data) => {
    try {
      const result = await createMemory(data);
      set((state) => ({ memories: [result, ...state.memories], error: null }));
      get().loadStats();
      return result;
    } catch (error) {
      set({ error: error instanceof Error ? error.message : String(error) });
      return null;
    }
  },

  editMemory: async (id, data) => {
    try {
      const result = await updateMemory(id, data);
      set((state) => ({
        memories: state.memories.map((m) => (m.id === id ? result : m)),
        error: null,
      }));
      get().loadStats();
      return true;
    } catch (error) {
      set({ error: error instanceof Error ? error.message : String(error) });
      return false;
    }
  },

  removeMemory: async (id) => {
    try {
      await deleteMemory(id);
      set((state) => ({
        memories: state.memories.filter((m) => m.id !== id),
        error: null,
      }));
      get().loadStats();
    } catch (error) {
      set({ error: error instanceof Error ? error.message : String(error) });
    }
  },

  loadStats: async () => {
    try {
      const result = await getMemoryStats();
      if (result) set({ stats: result });
    } catch {
      // Do not overwrite stats or crash on failure
    }
  },

  triggerExtraction: async (conversationId) => {
    try {
      await extractMemories(conversationId);
      // Reload after extraction
      get().loadMemories();
      get().loadStats();
    } catch (error) {
      set({ error: error instanceof Error ? error.message : String(error) });
    }
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
      set({ reindexResult: { ok: false, error: e instanceof Error ? e.message : String(e) } });
    } finally {
      set({ isReindexing: false });
    }
  },
}));
