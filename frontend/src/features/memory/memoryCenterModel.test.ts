import { describe, expect, it } from "vitest";
import type { MemoryRecord, MemoryStats } from "../../api/client";
import {
  MEMORY_CATEGORY_OPTIONS,
  getMemoryCategoryPresentation,
  getMemoryDetailFields,
  getMemorySourceLabel,
  memoryCategoryCounts,
  truncateMemoryContent,
} from "./memoryCenterModel";

function memory(overrides: Partial<MemoryRecord> = {}): MemoryRecord {
  return {
    id: "m-1",
    content: "用户偏好浅色主题",
    category: "preference",
    source: "manual",
    created_at: 1_700_000_000_000,
    updated_at: 1_700_000_100_000,
    ...overrides,
  };
}

describe("memory center model", () => {
  it("uses only the backend's real memory categories", () => {
    expect(MEMORY_CATEGORY_OPTIONS.map((option) => option.value)).toEqual([
      "",
      "fact",
      "preference",
      "knowledge",
      "note",
    ]);
    expect(getMemoryCategoryPresentation("fact").label).toBe("事实");
    expect(getMemoryCategoryPresentation("preference").label).toBe("偏好");
    expect(getMemoryCategoryPresentation("knowledge").label).toBe("知识");
    expect(getMemoryCategoryPresentation("note").label).toBe("笔记");
  });

  it("maps real source values without inventing usage or provenance", () => {
    expect(getMemorySourceLabel("auto")).toBe("AI 提取");
    expect(getMemorySourceLabel("manual")).toBe("手动");
    expect(getMemorySourceLabel("document")).toBe("文档");
    expect(getMemorySourceLabel("other")).toBe("other");
  });

  it("builds category overview counts from persisted stats only", () => {
    const stats: MemoryStats = {
      total: 5,
      by_category: [["fact", 2], ["preference", 1], ["knowledge", 2]],
      by_source: [],
    };
    expect(memoryCategoryCounts(stats)).toEqual({ fact: 2, preference: 1, knowledge: 2, note: 0 });
  });

  it("truncates only the list preview and keeps the full detail value", () => {
    const value = "这是一个较长的记忆内容，用来确认列表不会把整段正文撑满页面。";
    expect(truncateMemoryContent(value, 12)).toHaveLength(13);
    expect(truncateMemoryContent(value, 12).endsWith("…")).toBe(true);
    expect(getMemoryDetailFields(memory()).map((field) => field.key)).toEqual([
      "category",
      "content",
      "created_at",
      "updated_at",
      "source",
    ]);
    expect(getMemoryDetailFields(memory()).map((field) => field.key)).not.toContain("embedding");
    expect(getMemoryDetailFields(memory()).map((field) => field.key)).not.toContain("confidence");
  });
});
