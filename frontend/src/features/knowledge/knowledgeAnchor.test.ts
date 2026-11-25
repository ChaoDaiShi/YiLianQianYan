import { describe, expect, it } from "vitest";
import type { KnowledgeDetail } from "../../api/knowledge";
import { anchorForSearchHit, knowledgeSourceParams, readKnowledgeAnchor, resolveKnowledgeAnchor } from "./knowledgeAnchor";

const detail: KnowledgeDetail = {
  document: {
    id: "文档", resource_id: "resource", name: "原文", mime_type: "text/plain", hash: "file",
    size: 12, char_count: 5, chunk_count: 1, created_at: 1,
  },
  version: "body-v1", text: "前𠮷证据后", memberships: [], build_id: null, chunks: [],
};

describe("canonical knowledge source anchors", () => {
  it("round-trips document, body revision and Unicode scalar offsets through the library URL", () => {
    const anchor = anchorForSearchHit({
      document_id: detail.document.id, document_name: detail.document.name,
      version: detail.version, start_char: 1, content: "𠮷证据",
    });
    expect(readKnowledgeAnchor(knowledgeSourceParams(anchor))).toEqual(anchor);
    expect(resolveKnowledgeAnchor(detail, anchor)).toEqual({ before: "前", excerpt: "𠮷证据", after: "后" });
    expect(resolveKnowledgeAnchor({ ...detail, version: "body-v2" }, anchor)).toBeNull();
  });
});
