import {requestResult} from "../core/api/http";
export interface SoftwareRegistrationInput {
 name: string; adapter: string; executable: string; workspace_id: string | null;
 allow_observe: boolean; allow_content?: boolean; allow_window_ocr?: boolean; allow_actions: boolean;
}
export interface SoftwareRegistration extends SoftwareRegistrationInput {id: string; revision: number}
export interface SoftwareWindowIdentity {hwnd: string; pid: number; process_started_at: string; executable: string; class_name: string}
export interface SoftwareCandidate {identity: SoftwareWindowIdentity; visible: boolean; minimized: boolean}
export interface SoftwareViewport {x: number; y: number; width: number; height: number}
export interface SoftwareSession {
 id: string; registration_id: string; generation: number;
 window: {identity: SoftwareWindowIdentity; mode: string; visible: boolean; focused: boolean; minimized: boolean; embedding_supported: boolean; reason: string | null};
 state: string; launched_by_host: boolean; last_error: string | null;
}
export interface SoftwareOperation {
 operation_id: string; action: string; session_id: string | null; registration_id: string;
 state: string; reason: string | null; created_at: number;
}
export interface SoftwareMonitorAlert {
 id: string;
 kind: "process_exited" | "window_closed" | "window_rebuilt" | "notification_received" | "window_content_changed" | "wechat_messages_received";
 observed_at: number;
 summary: string;
 explanation: string | null;
 explaining: boolean;
}
export interface SoftwareMonitorStatus {
 registration_id: string;
 session_id?: string;
 state: "watching" | "waiting_window" | "waiting_process" | "unavailable";
 notification_state?: "checking" | "watching" | "access_required" | "unavailable";
 content_state?: "checking" | "watching" | "permission_required" | "unavailable";
 latest: SoftwareMonitorAlert | null;
}
export type SoftwareDimension = "discover_launch" | "display" | "perceive" | "operate" | "background_run" | "background_interaction";
export interface SoftwareCapabilityEntry {
 dimension: SoftwareDimension;
 status: "verified" | "unverified" | "unavailable" | "authorization_required" | "partial";
 source: string;
 evidence_id: string | null;
 observed_at: number | null;
}
export interface SoftwareCapabilityEvidence {
 registration_id: string; session_id: string | null; generation: number | null; entries: SoftwareCapabilityEntry[];
}
export interface SoftwareSnapshot {available: boolean; registrations: SoftwareRegistration[]; sessions: SoftwareSession[]; history: SoftwareOperation[]; monitors?: SoftwareMonitorStatus[]; capability_evidence?: SoftwareCapabilityEvidence[]}
export class SoftwareApiError extends Error {
 constructor(message: string, public status: number, public code: string) {super(message);}
 get uncertain() {return this.status === 0 || this.status >= 500;}
}
let nativeBinding: Promise<void> | null = null;
/** Bind only the trusted desktop's own main window; never accepts an HWND. */
export function initializeSoftwareHost(): Promise<void> {
 if (typeof window === "undefined" || window.__TAURI_INTERNALS__ === undefined) return Promise.resolve();
 if (nativeBinding) return nativeBinding;
 const pending = import("@tauri-apps/api/core")
  .then(native => native.invoke<void>("native_software_bind"))
  .catch(cause => {
   const message = cause && typeof cause === "object" && "message" in cause ? String(cause.message) : String(cause);
   throw new SoftwareApiError("原生软件连接初始化失败：" + message, 0, "native_software_bind_failed");
  }).finally(() => {if (nativeBinding === pending) nativeBinding = null;});
 nativeBinding = pending;
 return pending;
}
export function isSoftwareSession(value: unknown): value is SoftwareSession {
 if (!value || typeof value !== "object") return false;
 const session = value as Partial<SoftwareSession>;
 return typeof session.id === "string" && typeof session.registration_id === "string" && typeof session.generation === "number"
  && Number.isSafeInteger(session.generation) && session.generation >= 0 && typeof session.state === "string"
  && Boolean(session.window?.identity && typeof session.window.identity.hwnd === "string" && typeof session.window.identity.pid === "number"
   && typeof session.window.identity.process_started_at === "string" && /^[0-9]+$/.test(session.window.identity.process_started_at))
  && typeof session.window?.mode === "string";
}
async function sessionResult(pending: Promise<unknown>): Promise<SoftwareSession> {
 const value = await pending;
 if (!isSoftwareSession(value)) throw new SoftwareApiError("未收到有效软件会话，请刷新核对原操作，勿重复执行。", 0, "invalid_session_response");
 return value;
}
async function read<T>(method: string, path: string, body?: unknown): Promise<T> {
 const reply = await requestResult<T>(method, path, body, true);
 if (!reply.ok) {
  const details = reply.details;
  const code = typeof details?.error === "string" ? details.error : reply.error;
  const message = typeof details?.message === "string" ? details.message : reply.error;
  throw new SoftwareApiError(message, reply.status, code);
 }
 return reply.data;
}
const registrationPath = (id: string) => "/api/software/registrations/" + encodeURIComponent(id);
const sessionPath = (id: string) => "/api/software/sessions/" + encodeURIComponent(id);
export async function listSoftware() {
 const value = await read<SoftwareSnapshot>("GET", "/api/software");
 if (!value || typeof value.available !== "boolean" || !Array.isArray(value.registrations) || !Array.isArray(value.sessions) || !Array.isArray(value.history)) {
  throw new SoftwareApiError("未收到有效软件状态，请重新读取。", 0, "invalid_software_snapshot");
 }
 return value;
}
export const saveSoftware = (config: SoftwareRegistrationInput, current?: SoftwareRegistration) =>
 read<SoftwareRegistration>(current ? "PUT" : "POST", current ? registrationPath(current.id) : "/api/software/registrations", current ? {revision: current.revision, config} : config);
export const deleteSoftware = (current: SoftwareRegistration) => read<unknown>("DELETE", registrationPath(current.id), {revision: current.revision});
export const discoverSoftware = (id: string) => read<{candidates: SoftwareCandidate[]}>("POST", registrationPath(id) + "/discover", {});
export const setSoftwareMonitor = (id: string, enabled: boolean, sessionId?: string) =>
 read<SoftwareMonitorStatus[]>("PUT", registrationPath(id) + "/monitor", {enabled, session_id: sessionId});
export const attachSoftware = (id: string, identity: SoftwareWindowIdentity, operation_id: string) => sessionResult(read("POST", registrationPath(id) + "/attach", {operation_id, identity}));
export const launchSoftware = (current: Pick<SoftwareRegistration, "id" | "revision" | "executable">, operation_id: string) =>
 sessionResult(read("POST", registrationPath(current.id) + "/launch", {operation_id, confirmed: true, revision: current.revision, expected_executable: current.executable}));
export const acknowledgeSoftwareLaunch = (id: string, operation_id: string) => read<unknown>("POST", registrationPath(id) + "/acknowledge", {operation_id, confirmed: true});
export const presentSoftware = (session: SoftwareSession, mode: "embedded" | "independent" | "focus", operation_id: string, viewport?: SoftwareViewport) =>
 sessionResult(read("POST", sessionPath(session.id) + "/present", {generation: session.generation, operation_id, mode, viewport}));
export const releaseSoftware = (session: SoftwareSession, operation_id: string) =>
 sessionResult(read("POST", sessionPath(session.id) + "/release", {generation: session.generation, operation_id}));
export async function recognizeWechatCrop(png: string): Promise<string> {
 const native = await import("@tauri-apps/api/core");
 return native.invoke<string>("native_software_recognize_crop", {png});
}
export const readWechatFrame = (session: SoftwareSession) =>
 read<{png: string}>("POST", sessionPath(session.id) + "/wechat-frame", {generation: session.generation});
export const readWechatConversation = (session: SoftwareSession) =>
 read<{title: string; messages: string[]; draft: string; minimized: boolean}>(
  "POST", sessionPath(session.id) + "/wechat-conversation", {generation: session.generation});
export const stopSoftware = (session: SoftwareSession) => sessionResult(read("POST", sessionPath(session.id) + "/stop", {generation: session.generation}));
export const resumeSoftware = (session: SoftwareSession) => sessionResult(read("POST", sessionPath(session.id) + "/resume", {generation: session.generation}));
export const acknowledgeSoftware = (session: SoftwareSession) => sessionResult(read("POST", sessionPath(session.id) + "/acknowledge", {generation: session.generation, confirmed: true}));
