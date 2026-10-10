import { request, requestBlob, requestResult, type ApiResult } from "../core/api/http";

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

export interface CloudSyncPolicy {
  sync_conversations: boolean;
  sync_messages: boolean;
  sync_memories: boolean;
  sync_agent_profiles: boolean;
  sync_workflow_metadata: boolean;
  sync_artifact_metadata: boolean;
  sync_task_graphs: boolean;
  sync_canvas_views: boolean;
  sync_settings: boolean;
  sync_capability_usage: boolean;
}

export type LocalCleanupStatus = "cleared" | "failed";
export type RemoteRevocationStatus = "confirmed" | "pending" | "unknown" | "not_required";

export interface LogoutReport {
  local_signed_out: boolean;
  local_cleanup: LocalCleanupStatus;
  remote_revocation: RemoteRevocationStatus;
  error_codes: string[];
}

export interface CloudIdentityResponse {
  identity: CloudIdentity;
  sync: CloudSyncStatus;
  sync_existing_choice: "undecided" | "sync" | "local_only";
  report?: LogoutReport;
}

export interface CloudLogoutResponse {
  report: LogoutReport;
  activation: ActivationPlan;
  status: "awaiting_restart";
}

export interface CloudConnectionPayload {
  cloud_base_url: string;
  email: string;
  password: string;
  display_name?: string;
}

export interface CloudAuthenticationResponse {
  activation: ActivationPlan;
  status: "awaiting_restart";
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

export type LifecycleJobStatus =
  | "queued"
  | "running"
  | "waiting_for_producers"
  | "waiting_backup_expiry"
  | "completed"
  | "failed"
  | "retry_wait";

export interface LifecycleJob {
  id: string;
  kind: "export" | "deletion";
  status: LifecycleJobStatus;
  progress: number;
  result_ref: string | null;
  error_code: string | null;
  live_data_removed?: boolean;
  backups_cleared?: boolean;
  backup_purge_deadline?: number | null;
  status_credential?: string | null;
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

export function loginCloud(payload: CloudConnectionPayload): Promise<ApiResult<CloudAuthenticationResponse>> {
  return requestResult("POST", "/api/cloud/login", payload);
}

export function registerCloud(payload: CloudConnectionPayload): Promise<ApiResult<CloudAuthenticationResponse>> {
  return requestResult("POST", "/api/cloud/register", payload);
}

export interface ScopeLease {
  scope_id: string;
  generation: string;
}

export interface ProfileRecord {
  id: string;
  kind: "guest" | "account";
  service_origin: string | null;
  account_id: string | null;
  created_at: number;
  last_activated_at?: number;
  reauthentication_required?: boolean;
}

export interface ProfileView {
  active_profile_id: string;
  active_generation: string;
  active_lease: ScopeLease;
  profiles: ProfileRecord[];
}

export interface ActivationPlan {
  profile_id: string;
  lease: ScopeLease;
  requires_restart: boolean;
  import_count: number;
}

export interface ProfileSwitchResponse {
  plan: ActivationPlan;
  status: "awaiting_restart";
}

export type ActivationChoice =
  | { type: "open_account_only" }
  | { type: "import_selected_guest"; entity_ids: string[] };

export interface ProfileSwitchPayload {
  service_origin: string;
  account_id: string;
  choice?: ActivationChoice;
}

export function logoutCloud(): Promise<ApiResult<CloudLogoutResponse>> {
  return requestResult("POST", "/api/cloud/logout");
}

export function getCloudProfile(): Promise<ApiResult<ProfileView>> {
  return requestResult("GET", "/api/cloud/profile");
}

export function switchCloudProfile(payload: ProfileSwitchPayload): Promise<ApiResult<ProfileSwitchResponse>> {
  return requestResult("POST", "/api/cloud/profile/switch", payload);
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

export function getCloudSyncPolicy(): Promise<ApiResult<{ policy: CloudSyncPolicy }>> {
  return requestResult("GET", "/api/cloud/sync/policy");
}

export function updateCloudSyncPolicy(policy: CloudSyncPolicy): Promise<ApiResult<{ policy: CloudSyncPolicy }>> {
  return requestResult("PUT", "/api/cloud/sync/policy", policy);
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

export function downloadCloudExport(id: string): Promise<ApiResult<Blob>> {
  return requestBlob(`/api/cloud/export/${encodeURIComponent(id)}/download`);
}

export function requestAccountDeletion(): Promise<ApiResult<{ job: LifecycleJob; identity: CloudIdentity | null }>> {
  return requestResult("POST", "/api/cloud/account/delete");
}

export function getLifecycleJob(kind: LifecycleJob["kind"], id: string): Promise<ApiResult<{ job: LifecycleJob }>> {
  return requestResult("GET", `/api/cloud/jobs/${kind}/${id}`);
}
