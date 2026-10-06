import { request, requestResult, type ApiResult } from "../core/api/http";

export type IdentityStatus = "guest" | "authenticated" | "offline_authenticated" | "closing";

export interface CloudIdentity {
  local_identity_id: string;
  device_id: string;
  account_id: string | null;
  email: string | null;
  status: IdentityStatus;
  local_core_available: boolean;
  cloud_enabled: boolean;
}

export interface CloudSyncStatus {
  pending_count: number;
  conflict_count: number;
  error_count: number;
  cursor: string | null;
  last_success_at: number | null;
  last_error_code: string | null;
}

export interface CloudIdentityResponse {
  identity: CloudIdentity;
  sync: CloudSyncStatus;
  sync_existing_choice: "undecided" | "sync" | "local_only";
}

export interface CloudConnectionPayload {
  cloud_base_url: string;
  email: string;
  password: string;
  display_name?: string;
}

export interface CloudSyncResponse {
  identity_status: IdentityStatus;
  cloud_configured: boolean;
  sync_existing_choice: "undecided" | "sync" | "local_only";
  sync: CloudSyncStatus;
}

export interface SearchConfig {
  provider: "anysearch";
  base_url: string;
  configured: boolean;
  credential_state: "anonymous" | "local_only";
}

export interface SearchResult {
  title: string;
  url: string;
  snippet?: string | null;
  content?: string | null;
}

export interface SearchResponse {
  request_id: string;
  results: SearchResult[];
  total_results: number;
  search_time_ms: number;
}

export interface CloudDataSummary {
  counts: Record<string, number>;
  object_bytes: number;
}

export interface LifecycleJob {
  id: string;
  kind: "export" | "deletion";
  status: "queued" | "running" | "completed" | "failed";
  progress: number;
  result_ref: string | null;
  error_code: string | null;
  created_at: number;
  updated_at: number;
}

export interface SyncConflict {
  id: string;
  entity_type: string;
  entity_id: string;
  local_revision: number;
  remote_revision: number;
  local_payload: unknown;
  remote_payload: unknown;
  remote_operation: "upsert" | "delete";
  created_at: number;
}

export interface CloudSession {
  id:string;
  device_id:string;
  device_label:string;
  current:boolean;
  created_at:number;
  last_seen_at:number;
  revoked_at:number|null;
}

export function getCloudIdentity(): Promise<CloudIdentityResponse | null> {
  return request("GET", "/api/cloud/identity");
}

export function loginCloud(payload: CloudConnectionPayload): Promise<ApiResult<CloudIdentityResponse>> {
  return requestResult("POST", "/api/cloud/login", payload);
}

export function registerCloud(payload: CloudConnectionPayload): Promise<ApiResult<CloudIdentityResponse>> {
  return requestResult("POST", "/api/cloud/register", payload);
}

export function logoutCloud(): Promise<ApiResult<CloudIdentityResponse>> {
  return requestResult("POST", "/api/cloud/logout");
}

export function getCloudSyncStatus(): Promise<CloudSyncResponse | null> {
  return request("GET", "/api/cloud/sync/status");
}

export function syncCloudNow(): Promise<ApiResult<{ report: { pushed: number; pulled: number; conflicts: number; cursor: string }; sync: CloudSyncStatus }>> {
  return requestResult("POST", "/api/cloud/sync/now");
}

export function updateSyncChoice(choice: CloudIdentityResponse["sync_existing_choice"]): Promise<ApiResult<CloudSyncResponse>> {
  return requestResult("PUT", "/api/cloud/sync/choice", { choice });
}

export function listSyncConflicts(): Promise<ApiResult<{ conflicts: SyncConflict[] }>> {
  return requestResult("GET", "/api/cloud/sync/conflicts");
}

export function resolveSyncConflict(id: string, resolution: "local" | "remote"): Promise<ApiResult<{ conflicts: SyncConflict[] }>> {
  return requestResult("POST", `/api/cloud/sync/conflicts/${id}/resolve`, { resolution });
}

export function listCloudSessions():Promise<ApiResult<{sessions:CloudSession[]}>>{return requestResult("GET","/api/cloud/sessions")}
export function revokeCloudSession(id:string):Promise<ApiResult<{sessions:CloudSession[]}>>{return requestResult("POST",`/api/cloud/sessions/${id}/revoke`)}

export function getSearchConfig(): Promise<SearchConfig | null> {
  return request("GET", "/api/search/config");
}

export function updateSearchConfig(payload: { base_url?: string; api_key?: string; clear_api_key?: boolean }): Promise<ApiResult<SearchConfig>> {
  return requestResult("PUT", "/api/search/config", payload);
}

export function runSearch(payload: { query: string; max_results?: number; tag?: string; zone?: "cn" | "intl"; language?: string }): Promise<ApiResult<SearchResponse>> {
  return requestResult("POST", "/api/search", payload);
}

export function getPrivacySummary(): Promise<ApiResult<{ summary: CloudDataSummary; identity: CloudIdentity }>> {
  return requestResult("GET", "/api/cloud/privacy/summary");
}

export function requestCloudExport(): Promise<ApiResult<{ job: LifecycleJob }>> {
  return requestResult("POST", "/api/cloud/export");
}

export function requestAccountDeletion(): Promise<ApiResult<{ job: LifecycleJob; identity: CloudIdentity | null }>> {
  return requestResult("POST", "/api/cloud/account/delete");
}

export function getLifecycleJob(kind: LifecycleJob["kind"], id: string): Promise<ApiResult<{ job: LifecycleJob }>> {
  return requestResult("GET", `/api/cloud/jobs/${kind}/${id}`);
}
