import type { KnowledgeDetail, KnowledgeSearchHit, KnowledgeSourceAnchor } from "../../api/knowledge";

export function anchorForSearchHit(hit: KnowledgeSearchHit): KnowledgeSourceAnchor {
  return {
    document_id: hit.document_id,
    version: hit.version,
    start_char: hit.start_char,
    end_char: hit.start_char + [...hit.content].length,
  };
}

export function knowledgeSourceParams(anchor: KnowledgeSourceAnchor) {
  return new URLSearchParams({
    document: anchor.document_id,
    version: anchor.version,
    start: String(anchor.start_char),
    end: String(anchor.end_char),
  });
}

export function knowledgeSourceUrl(anchor: KnowledgeSourceAnchor) {
  return `/knowledge?${knowledgeSourceParams(anchor)}`;
}

export function readKnowledgeAnchor(params: URLSearchParams): KnowledgeSourceAnchor | null {
  const document_id = params.get("document");
  const version = params.get("version");
  const start = params.get("start");
  const end = params.get("end");
  return document_id && version && start && end && /^\d+$/.test(start) && /^\d+$/.test(end)
    ? { document_id, version, start_char: Number(start), end_char: Number(end) }
    : null;
}

export function resolveKnowledgeAnchor(detail: KnowledgeDetail, anchor: KnowledgeSourceAnchor) {
  if (detail.document.id !== anchor.document_id || detail.version !== anchor.version
    || !Number.isSafeInteger(anchor.start_char) || !Number.isSafeInteger(anchor.end_char)
    || anchor.start_char < 0 || anchor.start_char >= anchor.end_char
    || anchor.end_char > detail.document.char_count) return null;

  let start = 0;
  let end = 0;
  let index = 0;
  for (const character of detail.text) {
    if (index === anchor.start_char) start = end;
    if (index === anchor.end_char) break;
    index++;
    end += character.length;
  }
  if (index !== anchor.end_char) return null;
  return {
    before: detail.text.slice(0, start),
    excerpt: detail.text.slice(start, end),
    after: detail.text.slice(end),
  };
}
