import { describe, expect, it } from "vitest";
import workspaceSurfaceSource from "../surfaces/workspace/WorkspaceSurface.tsx?raw";
import memoryPageSource from "./MemoryPage.tsx?raw";
import knowledgePageSource from "./KnowledgePage.tsx?raw";
import knowledgeLibrarySource from "../features/knowledge/KnowledgeLibrary.tsx?raw";
import knowledgeImportSource from "../features/knowledge/KnowledgeImportDialog.tsx?raw";
import knowledgeReaderSource from "../features/knowledge/KnowledgeDocumentReader.tsx?raw";
import referenceSource from "../features/memory/MemoryKnowledgeReferences.tsx?raw";
import memoryIndexControlsSource from "../features/memory/MemoryIndexControls.tsx?raw";
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

  it("offers one category filter and moves maintenance to More", () => {
    expect(memoryPageSource).not.toContain("记忆分类概览");
    expect(memoryPageSource.match(/aria-label="记忆分类筛选"/g)).toHaveLength(1);
    expect(memoryPageSource).toMatch(/<summary[^>]*>更多[\s\S]*记忆来源筛选[\s\S]*<MemoryIndexControls \/>[\s\S]*<\/details>/);
    expect(memoryIndexControlsSource).toContain("重建索引");
    expect(memoryPageSource.match(/>添加记忆<\/Button>/g)).toHaveLength(1);
  });

  it("loads curated knowledge entries and connects their versioned citations to the library reader", () => {
    for (const label of ["近期记忆", "长期记忆", "知识引用", "底层记忆"]) expect(memoryPageSource).toContain(label);
    expect(referenceSource).toContain("listKnowledgeEntries");
    expect(referenceSource).toContain("knowledgeSourceUrl");
    expect(referenceSource).not.toContain("chunk_count");
    expect(knowledgeReaderSource).toContain("listKnowledgeChunks");
    expect(knowledgeReaderSource).toContain("知识文档全文");
    expect(knowledgeReaderSource).toContain("文档切片");
    expect(knowledgeReaderSource).toContain("resolveKnowledgeAnchor");
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

  it("offers a single document import entry and shows format guidance in its chooser", () => {
    expect(knowledgeLibrarySource).toContain("导入资料");
    expect(knowledgeLibrarySource).not.toContain("knowledge-library-ai-entry");
    expect(knowledgeLibrarySource).not.toContain("PDF 文本层");
    expect(knowledgeImportSource).toContain("选择文件");
    expect(knowledgeImportSource).toContain("选择文件夹");
    expect(knowledgeImportSource).toContain("PDF 文本层");
    expect(knowledgeImportSource).toContain("停止后续导入");
  });

  it("renders a knowledge request failure instead of a no-results or empty-library state", () => {
    expect(knowledgeLibrarySource).toMatch(/resultsError\s*\?\s*<ErrorState[\s\S]*没有匹配的知识内容[\s\S]*导入第一份知识文档/);
  });

  it("keeps both capability entries in the existing nav group", () => {
    const capabilityRoutes = NAV_GROUPS.find((group) => group.label === "能力")?.items.map((item) => item.to);
    expect(capabilityRoutes).toContain("/memory");
    expect(capabilityRoutes).toContain("/knowledge");
  });
});
