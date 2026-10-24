export interface CreationDraft { title: string; content: string; baseRevision: number }
export type CreationDrafts = Record<string, CreationDraft>;
const key = (workspace: string) => `yilian.creation.drafts.${encodeURIComponent(workspace)}`;
export function readCreationDrafts(workspace: string, storage: Pick<Storage, "getItem">): CreationDrafts {
  const saved = storage.getItem(key(workspace));
  return saved === null ? {} : JSON.parse(saved) as CreationDrafts;
}
export function writeCreationDrafts(workspace: string, drafts: CreationDrafts, storage: Pick<Storage, "setItem" | "removeItem">): void {
  if (Object.keys(drafts).length === 0) storage.removeItem(key(workspace));
  else storage.setItem(key(workspace), JSON.stringify(drafts));
}
export function clearSavedCreationDraft(workspace: string, document: string, storage: Pick<Storage, "getItem" | "setItem" | "removeItem">, submitted: CreationDraft): void {
  const drafts = readCreationDrafts(workspace, storage);
  const saved = drafts[document];
  if (!saved || saved.baseRevision !== submitted.baseRevision || saved.title !== submitted.title || saved.content !== submitted.content) return;
  delete drafts[document];
  writeCreationDrafts(workspace, drafts, storage);
}
