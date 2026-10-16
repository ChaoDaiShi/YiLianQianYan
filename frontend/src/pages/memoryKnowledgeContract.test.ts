import { describe, expect, it } from "vitest";
import workspaceSurfaceSource from "../surfaces/workspace/WorkspaceSurface.tsx?raw";
import memoryPageSource from "./MemoryPage.tsx?raw";
import knowledgePageSource from "./KnowledgePage.tsx?raw";
import { NAV_GROUPS } from "../components/layout/navGroups";

describe("Memory and Knowledge page boundaries", () => {
  it("exposes Memory Center as a real memory management route with scoped optimization class", () => {
    expect(workspaceSurfaceSource).toContain('path="memory"');
    expect(memoryPageSource).toContain("useMemoryStore");
    expect(memoryPageSource).toContain("aria-pressed");
    expect(memoryPageSource).toContain("ErrorState");
    expect(memoryPageSource).toContain("Skeleton");
    expect(memoryPageSource).toContain("ui-opt-memory");
    expect(memoryPageSource).not.toContain("embedding");
    expect(memoryPageSource).not.toContain("confidence");
    expect(memoryPageSource).not.toContain("置信度");
  });

  it("separates empty states between no records and no filter match", () => {
    expect(memoryPageSource).toContain("没有匹配的记忆记录");
    expect(memoryPageSource).toContain("清除筛选");
    expect(memoryPageSource).toContain("这里暂时还没有记忆记录");
    expect(memoryPageSource).toContain("handleClearFilters");
  });

  it("enforces mutation loading states and preserves drafts on failure", () => {
    expect(memoryPageSource).toContain("isAdding");
    expect(memoryPageSource).toContain("isEditing");
    expect(memoryPageSource).toContain("addError");
    expect(memoryPageSource).toContain("editError");
    expect(memoryPageSource).toContain("loading={isAdding}");
    expect(memoryPageSource).toContain("loading={isEditing}");
    expect(memoryPageSource).toContain("loadingLabel=");
    expect(memoryPageSource).toContain("useMemoryStore.getState()");
    expect(memoryPageSource).toContain("stopPropagation");
  });

  it("keeps the Knowledge page honest when no knowledge API exists", () => {
    expect(knowledgePageSource).toContain("ui-opt-knowledge");
    expect(knowledgePageSource).toContain("没有独立知识库数据源");
    expect(knowledgePageSource).toContain("尚未接入");
    expect(knowledgePageSource).toContain("当前版本提供长期记忆记录；外部文档、文件预览和独立检索索引尚未接入");
    expect(knowledgePageSource).toContain("当前可用：长期记忆记录");
    expect(knowledgePageSource).toContain("尚未接入：独立文档知识库");
    expect(knowledgePageSource).toContain('navigate("/memory")');
    expect(knowledgePageSource).not.toContain("上传文档");
    expect(knowledgePageSource).not.toContain("相似度");
    expect(knowledgePageSource).not.toContain("embedding");
  });

  it("keeps both capability entries in the existing nav group", () => {
    const capabilityRoutes = NAV_GROUPS.find((group) => group.label === "能力")?.items.map((item) => item.to);
    expect(capabilityRoutes).toContain("/memory");
    expect(capabilityRoutes).toContain("/knowledge");
  });
});
