import { requestResult } from "../../api/transport";
export interface ModelPoolTier { id: string; label: string; profile_ids: string[] }
export interface ModelPool { id: string; label: string; service_kind: "chat"|"vision"|"image"; enabled: boolean; revision: number; attempt_timeout_ms: number; total_timeout_ms: number; tiers: ModelPoolTier[] }
export interface ProviderAccount { base_url: string; credential_id: string | null; id: string; provider_id: string; label: string; profile_ids: string[]; credential_status?: "configured" | "missing" | "unavailable" | "not_required"; credential_configured?: boolean }
export async function poolRequest<T>(method: string, path: string, body?: unknown): Promise<T> {
 const result = await requestResult<T>(method, path, body);
 if (!result.ok) throw new Error(result.error);
 return result.data;
}
export const listModelPools = () => poolRequest<ModelPool[]>("GET", "/api/model-pools");
export const listProviderAccounts = () => poolRequest<ProviderAccount[]>("GET", "/api/provider-accounts");
