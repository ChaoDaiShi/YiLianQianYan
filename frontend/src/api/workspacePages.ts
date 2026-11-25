import { requestBlob, requestResult } from "../core/api/http";
import type { RichMessage } from "./richContent";

export interface WorkspacePage {
  id: string;
  workspace_id: string;
  parent_page_id: string | null;
  title: string;
  icon: string | null;
  cover_resource_id: string | null;
  favorite: boolean;
  last_visited_at: number | null;
  revision: number;
  created_at: number;
  updated_at: number;
  deleted_at: number | null;
  has_children: boolean;
}

export type PageBlock =
  | { id: string; type: "markdown"; text: string }
  | { id: string; type: "page_link"; page_id: string }
  | { id: string; type: "knowledge_link"; document_id: string; name: string; version: string; end_char: number }
  | { id: string; type: "resource"; resource_id: string; name: string; mime_type: string }
  | { id: string; type: "rich"; conversation_id: string; message_id: string; snapshot: RichMessage };

export interface PageDocument extends WorkspacePage {
  blocks: PageBlock[];
}

export type BlockOperation =
  | { op: "insert" | "replace"; block: PageBlock; before?: string }
  | { op: "move" | "remove"; id: string; before?: string };

export interface PageRevision {
  revision: number;
  source: string;
  created_at: number;
}

const pagePath = (id: string) => "/api/v1/pages/" + encodeURIComponent(id);
export function workspacePagePath(workspaceId: string, pageId: string) {
  return "/workspaces/" + encodeURIComponent(workspaceId) + "?tab=pages&page=" + encodeURIComponent(pageId);
}

export function listPages(workspace: string, options: { parent?: string; offset?: number; q?: string; view?: "trash" | "favorites" | "recent" | "all" } = {}) {
  const params = new URLSearchParams({ workspace_id: workspace });
  if (options.parent) params.set("parent_page_id", options.parent);
  if (options.offset) params.set("offset", String(options.offset));
  if (options.q) params.set("q", options.q);
  if (options.view) params.set("view", options.view);
  return requestResult<{ pages: WorkspacePage[]; next_offset: number | null }>("GET", "/api/v1/pages?" + params);
}

export function loadPage(id: string, visit = false) {
  return requestResult<PageDocument>("GET", pagePath(id) + (visit ? "?visit=true" : ""));
}

export function createPage(data: { workspace_id: string; title: string; parent_page_id?: string; markdown?: string }) {
  return requestResult<PageDocument>("POST", "/api/v1/pages", data);
}

export function updatePageMetadata(id: string, data: {
  revision: number; title?: string; icon?: string; cover_resource_id?: string; favorite?: boolean;
  parent_page_id?: string | null; before_page_id?: string;
}) {
  return requestResult<PageDocument>("PATCH", pagePath(id), data);
}

export function updatePageBlocks(id: string, revision: number, operations: BlockOperation[]) {
  return requestResult<PageDocument>("POST", pagePath(id) + "/blocks/patch", { revision, operations });
}

export function listPageHistory(id: string, offset = 0) {
  return requestResult<PageRevision[]>("GET", pagePath(id) + "/revisions?offset=" + offset);
}

export function restorePageRevision(id: string, revision: number, target: number) {
  return requestResult<PageDocument>("POST", pagePath(id) + "/revisions/" + target + "/restore", { revision });
}

export function movePageToTrash(id: string, revision: number) {
  return requestResult<Record<string, never>>("DELETE", pagePath(id), { revision });
}

export function restorePage(id: string, revision: number) {
  return requestResult<Record<string, never>>("POST", pagePath(id) + "/restore", { revision });
}

export function duplicatePage(id: string, revision: number) {
  return requestResult<PageDocument>("POST", pagePath(id) + "/duplicate", { revision });
}

export function exportPageMarkdown(id: string) {
  return requestBlob(pagePath(id) + "/markdown");
}
