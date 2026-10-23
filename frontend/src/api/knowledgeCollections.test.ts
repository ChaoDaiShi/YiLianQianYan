import { afterEach, describe, expect, it, vi } from "vitest";
import { importKnowledge, listKnowledge, searchKnowledge } from "./knowledge";
import { ingestResource } from "./resources";
import { requestResult } from "../core/api/http";

vi.mock("./resources", () => ({ ingestResource: vi.fn() }));
vi.mock("../core/api/http", () => ({ requestResult: vi.fn() }));
afterEach(() => vi.resetAllMocks());

describe("knowledge folder API contract", () => {
  it("retains folder membership even when the server reports a deduplicated document", async () => {
    vi.mocked(ingestResource).mockResolvedValue({ id: "resource", source: "upload", name: "notes.txt", mime_type: "text/plain", size: 12, hash: "same-content", storage_path: "managed", metadata: {}, created_at: 1, updated_at: 1 });
    vi.mocked(requestResult).mockResolvedValue({ ok: true, data: { document: { id: "canonical" }, created: false } });
    const file = { name: "notes.txt", size: 12, type: "text/plain", webkitRelativePath: "课程/章节/notes.txt" } as File;
    const location = { collection_id: "collection", relative_path: file.webkitRelativePath };
    const result = await importKnowledge(file, location);
    expect(result.created).toBe(false);
    expect(requestResult).toHaveBeenCalledWith("POST", "/api/knowledge/documents", { resource_id: "resource", location });
  });

  it("applies the same collection to listing and search", async () => {
    vi.mocked(requestResult).mockResolvedValue({ ok: true, data: {} });
    await listKnowledge("collection");
    await searchKnowledge("tail", "collection");
    expect(requestResult).toHaveBeenNthCalledWith(1, "GET", "/api/knowledge/documents?collection_id=collection", undefined);
    expect(requestResult).toHaveBeenNthCalledWith(2, "GET", "/api/knowledge/search?q=tail&collection_id=collection", undefined);
  });

  it("keeps the 25 MiB upload boundary before reading the file", async () => {
    await expect(importKnowledge({ size: 25 * 1024 * 1024 + 1 } as File)).rejects.toThrow("25 MiB");
    expect(ingestResource).not.toHaveBeenCalled();
  });
});
