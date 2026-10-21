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

  it("connects Knowledge to its actual import and retrieval library",()=>{
    expect(knowledgePageSource).toContain("ui-opt-knowledge");
    expect(knowledgePageSource).toContain("KnowledgeLibrary");
    expect(knowledgePageSource).not.toContain("尚未接入");
  });

  it("keeps both capability entries in the existing nav group", () => {
    const capabilityRoutes = NAV_GROUPS.find((group) => group.label === "能力")?.items.map((item) => item.to);
    expect(capabilityRoutes).toContain("/memory");
    expect(capabilityRoutes).toContain("/knowledge");
  });
});
