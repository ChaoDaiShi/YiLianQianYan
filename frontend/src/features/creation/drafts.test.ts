import { describe, expect, it } from "vitest";
import { clearSavedCreationDraft, readCreationDrafts, writeCreationDrafts } from "./drafts";

describe("workspace creation drafts", () => {
  it("restores unsaved text and its base revision only in its workspace, then removes saved drafts", () => {
    const values = new Map<string, string>();
    const storage = { getItem: (key: string) => values.get(key) ?? null, setItem: (key: string, value: string) => { values.set(key, value); }, removeItem: (key: string) => { values.delete(key); } };
    const drafts = { document: { title: "未保存的标题", content: "刷新以后仍在\n**正文**", baseRevision: 3 } };
    writeCreationDrafts("workspace-a", drafts, storage);
    expect(readCreationDrafts("workspace-a", storage)).toEqual(drafts);
    expect(readCreationDrafts("workspace-b", storage)).toEqual({});
    writeCreationDrafts("workspace-a", {}, storage);
    expect(values.size).toBe(0);
    expect(readCreationDrafts("workspace-a", storage)).toEqual({});
  });
  it("clears a successful save even after its editor unmounts, keeping other unsaved documents", () => {
    const values = new Map<string, string>();
    const storage = { getItem: (key: string) => values.get(key) ?? null, setItem: (key: string, value: string) => { values.set(key, value); }, removeItem: (key: string) => { values.delete(key); } };
    const first = { title: "已提交", content: "保存中的内容", baseRevision: 1 };
    const other = { title: "另一份草稿", content: "尚未保存", baseRevision: 3 };
    writeCreationDrafts("workspace", { first, other }, storage);
    clearSavedCreationDraft("workspace", "first", storage, first);
    expect(readCreationDrafts("workspace", storage)).toEqual({ other });
  });
  it("keeps a newer draft written after the save started", () => {
    const values = new Map<string, string>();
    const storage = { getItem: (key: string) => values.get(key) ?? null, setItem: (key: string, value: string) => { values.set(key, value); }, removeItem: (key: string) => { values.delete(key); } };
    const submitted = { title: "文稿", content: "提交时的正文", baseRevision: 1 };
    const newer = { ...submitted, content: "返回页面后继续编辑的正文" };
    writeCreationDrafts("workspace", { document: newer }, storage);
    clearSavedCreationDraft("workspace", "document", storage, submitted);
    expect(readCreationDrafts("workspace", storage)).toEqual({ document: newer });
  });
});
