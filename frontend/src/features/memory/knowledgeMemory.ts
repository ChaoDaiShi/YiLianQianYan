import type { MemoryRecord } from "../../api/memory";

/** Only document-index chunks carry this generated document ownership metadata. */
export function knowledgeDocumentForMemory(memory: MemoryRecord): string | null {
  if (memory.category !== "knowledge" || memory.source !== "document" || !memory.metadata) return null;
  let metadata: unknown;
  try { metadata = JSON.parse(memory.metadata); } catch { return null; }
  if (!metadata || typeof metadata !== "object") return null;
  const id = (metadata as Record<string, unknown>).knowledge_document_id;
  return typeof id === "string" && id.length > 0 ? id : null;
}
