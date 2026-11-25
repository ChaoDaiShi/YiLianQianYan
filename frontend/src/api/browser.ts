import { requestResult } from "../core/api/http";
export interface BrowserTab { session_id: string; tab_id: string; navigation_id: number; url: string; title: string; mode: "embedded" | "detached" | "hidden"; workspace_id?: string | null; site_id?: string | null; loading?: boolean; can_go_back?: boolean; can_go_forward?: boolean; error?: string | null; ephemeral?: boolean }
export interface BrowserReply { state: string; data: unknown; message?: string; reason?: string }
export interface ProjectSite { id: string; workspace_id: string; title: string; purpose: string; url: string; origin: string; port: number; startup_hint: string; bookmarked: boolean; status: "active" | "archived"; revision: number; last_visited_at: number | null }
export interface BrowserVisit { id: string; workspace_id?: string | null; site_id?: string | null; redacted_url: string; title: string; visited_at: number; service_identity?: string }
export type SiteInput = Pick<ProjectSite, "workspace_id" | "title" | "purpose" | "url" | "startup_hint" | "bookmarked">;
export { nativeBrowserAvailable, browserTabs, browserExecute, targetOf, listBrowserUnknown, acknowledgeBrowserUnknown, type UnknownBrowserOperation } from "../surfaces/desktop/browserBridge";
export { browserUiState, resolveBrowserRequest, browserCommand, listenForBrowserShortcut, listenForBrowserOpened, type BrowserUiRequest, type BrowserUiState, type BrowserShortcut, type BrowserFindResult } from "../surfaces/desktop/browserBridge";
/** Open results contain the authoritative native tab inside the result envelope. */
export function openedBrowserTab(reply: BrowserReply): BrowserTab {
 const data = reply.data as {tab?: Partial<BrowserTab>} | null;
 const tab = data?.tab;
 if (!tab || typeof tab.tab_id !== "string" || typeof tab.session_id !== "string" || typeof tab.navigation_id !== "number" || typeof tab.url !== "string") throw new Error("浏览器未返回有效的原生会话；请重新检查会话列表。");
 return tab as BrowserTab;
}
async function read<T>(method: string, path: string, body?: unknown): Promise<T> { const result = await requestResult<T>(method, path, body); if (!result.ok) throw new Error(result.error); return result.data; }
export function listProjectSites(workspaceId?: string, archived = false) { const q = new URLSearchParams({ include_archived: String(archived) }); if (workspaceId) q.set("workspace_id", workspaceId); return read<{sites: ProjectSite[]; truncated: boolean}>("GET", `/api/browser/sites?${q}`); }
export function saveProjectSite(body: SiteInput, site?: ProjectSite) { return read<ProjectSite>(site ? "PUT" : "POST", site ? `/api/browser/sites/${encodeURIComponent(site.id)}` : "/api/browser/sites", site ? { title: body.title, purpose: body.purpose, url: body.url, startup_hint: body.startup_hint, bookmarked: body.bookmarked, expected_revision: site.revision } : body); }
export function archiveProjectSite(site: ProjectSite) { return read<unknown>("DELETE", `/api/browser/sites/${encodeURIComponent(site.id)}?expected_revision=${site.revision}`); }
export function restoreProjectSite(site: ProjectSite) { return read<unknown>("POST", `/api/browser/sites/${encodeURIComponent(site.id)}/restore`, { expected_revision: site.revision }); }
export function listBrowserVisits(workspaceId?: string) { return read<{visits: BrowserVisit[]; truncated: boolean; projection_issues?: unknown[]; projection_pending?: number}>("GET", `/api/browser/visits${workspaceId ? `?workspace_id=${encodeURIComponent(workspaceId)}` : ""}`); }
