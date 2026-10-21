import { describe, expect, it } from "vitest";
import { knowledgeDocumentForMemory } from "./knowledgeMemory";
import type { MemoryRecord } from "../../api/memory";
const record: MemoryRecord = { id: "m", category: "knowledge", source: "document", content: "original chunk", metadata: JSON.stringify({ knowledge_document_id: "document-1", chunk_index: 0 }), created_at: 1, updated_at: 1 };
describe("knowledge document ownership in memory", () => {
  it("marks only generated knowledge document chunks as document-owned", () => {
    expect(knowledgeDocumentForMemory(record)).toBe("document-1");
    expect(knowledgeDocumentForMemory({ ...record, source: "manual" })).toBeNull();
    expect(knowledgeDocumentForMemory({ ...record, category: "fact" })).toBeNull();
  });
  it("keeps existing manually managed memories editable without inventing document ownership", () => {
    expect(knowledgeDocumentForMemory({ ...record, metadata: undefined })).toBeNull();
    expect(knowledgeDocumentForMemory({ ...record, metadata: JSON.stringify({ source_file: "old.txt" }) })).toBeNull();
  });
});
