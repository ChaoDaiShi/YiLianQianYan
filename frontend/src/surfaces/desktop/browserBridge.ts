import type { BrowserTab, BrowserReply } from "../../api/browser";
// Native operations live in the desktop adapter; importing API modules stays portable.
async function invoke<T>(command: string, args?: Record<string,unknown>): Promise<T> {
 const native = await import("@tauri-apps/api/core");
 try { return await native.invoke<T>(command,args); }
 catch (cause) { if (cause && typeof cause === "object" && "message" in cause && typeof cause.message === "string") throw new Error(cause.message); throw cause; }
}
export function nativeBrowserAvailable(): boolean { return "__TAURI_INTERNALS__" in window; }
export async function browserTabs(): Promise<BrowserTab[]> { return invoke("native_browser_tabs"); }
export function targetOf(tab: BrowserTab) { return { session_id: tab.session_id, tab_id: tab.tab_id, navigation_id: tab.navigation_id }; }
export async function browserExecute(action: string, payload: Record<string, unknown> = {}, tab?: BrowserTab): Promise<BrowserReply> {
 const reply = await invoke<BrowserReply>("native_browser_execute", { request: { operation_id: crypto.randomUUID(), action, target: tab ? targetOf(tab) : null, payload, owner: null } });
 if (reply.state !== "verified") throw new Error(reply.state === "result_unknown" ? "操作结果不确定；请先检查页面，系统不会自动重复操作。" : reply.message || reply.reason || `浏览器未完成操作（${reply.state}）`);
 return reply;
}

export async function listenForBrowserOpened(onOpened: (tabId: string) => void): Promise<() => void> {
 const { listen } = await import("@tauri-apps/api/event");
 return listen<{tab_id:string}>("browser:opened", event => { if (typeof event.payload?.tab_id === "string" && event.payload.tab_id) onOpened(event.payload.tab_id); });
}
export async function listenForBrowserNewTab(onNewTab:()=>void):Promise<()=>void> {
 const {listen}=await import("@tauri-apps/api/event");
 return listen("browser:new-tab",()=>onNewTab());
}

export interface BrowserUiRequest {
 id: string;
 kind: "popup" | "download" | "permission";
 tab_id: string;
 session_id: string;
 navigation_id: number;
 url?: string;
 message: string;
 suggested_name?: string;
}
export interface BrowserUiState { requests: BrowserUiRequest[]; zoom: Record<string, number> }
export type BrowserShortcut = {action:"focus-address"|"new-tab"|"close-tab"|"next-tab"|"previous-tab";tab_id:string};
export interface BrowserFindResult { query: string; matched: boolean; match_count: number; active_match_index: number }
export interface BrowserRequestResult { saved?: boolean; cancelled?: boolean }
export function browserUiState(): Promise<BrowserUiState> { return invoke("native_browser_ui_state"); }
export function resolveBrowserRequest(id:string,decision:"open"|"save"|"allow"|"deny"): Promise<BrowserRequestResult> {
 return invoke("native_browser_resolve_request",{id,decision});
}
export function browserCommand<T>(tab:BrowserTab,command:"zoom"|"find"|"clear_data"|"new_tab",payload:Record<string,unknown>={}):Promise<T> {
 return invoke("native_browser_command",{target:targetOf(tab),command,payload});
}
export async function listenForBrowserShortcut(onShortcut:(shortcut:BrowserShortcut)=>void):Promise<()=>void> {
 const {listen}=await import("@tauri-apps/api/event");
 return listen<BrowserShortcut>("browser:shortcut",event=>onShortcut(event.payload));
}

export interface UnknownBrowserOperation { operation_id: string; action: string; target?: {session_id:string;tab_id:string;navigation_id:number} | null }
export function listBrowserUnknown(): Promise<UnknownBrowserOperation[]> { return invoke("native_browser_unknown"); }
export async function acknowledgeBrowserUnknown(operationId:string, inspected:boolean): Promise<void> {
 if (!inspected) throw new Error("请先检查实际页面并明确确认；不会自动重复原操作。");
 if (!operationId.trim()) throw new Error("缺少待审阅操作编号");
 await invoke("native_browser_acknowledge_unknown", {operationId});
}
