import { describe, expect, it } from "vitest";
import workspaceSurfaceSource from "../surfaces/workspace/WorkspaceSurface.tsx?raw";
import memoryPageSource from "./MemoryPage.tsx?raw";
import knowledgePageSource from "./KnowledgePage.tsx?raw";
import { NAV_GROUPS } from "../components/layout/navGroups";

describe("Memory and Knowledge page boundaries", () => {
  it("exposes Memory Center as a real memory management route", () => {
    expect(workspaceSurfaceSource).toContain('path="memory"');
    expect(memoryPageSource).toContain("useMemoryStore");
    expect(memoryPageSource).toContain("aria-pressed");
    expect(memoryPageSource).toContain("ErrorState");
    expect(memoryPageSource).toContain("Skeleton");
    expect(memoryPageSource).not.toContain("embedding");
  });

  it("keeps the Knowledge page honest when no knowledge API exists", () => {
    expect(knowledgePageSource).toContain("没有独立知识库数据源");
    expect(knowledgePageSource).toContain("尚未接入");
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
