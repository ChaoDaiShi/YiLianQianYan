import { requestResult } from "../core/api/http";

export interface ServiceLaunch { executable: string; working_directory: string; args: string[] }
export interface ServiceChannel { id: string; name: string; self_id: string | null; enabled: boolean; status: string; connection_mode: string }
export interface PluginServiceStatus {
  id: string; title: string; settings_id: string;
  state: "stopped" | "running" | "exited"; pid: number | null; exit_code: number | null;
  launch: ServiceLaunch | null; active_launch: ServiceLaunch | null; configuration_error: string | null;
  web_url: string | null; web_available: boolean; web_error: string | null;
  channel_id: string | null; channel: ServiceChannel | null; channels: ServiceChannel[];
  channel_id_key: string | null;
  configuration_revision: string | null;
  instance_id: string | null;
  service_health: string;
  health_error: string | null;
  health_check: "declared" | "management_page";
  task_uses: number;
  page_uses: number;
  deployment: "local" | "cloud";
  cloud_auth: "none" | "bearer";
  cloud_auth_configured: boolean;
  model_mode: "none" | "platform_chat" | "independent";
  model_allowed: boolean;
  mcp_server_id: string | null;
  mcp_status: "not_applicable" | "unavailable" | "ready";
  browser_tab_id?: string;
  page_lease_id?: string;
}
export interface EnsureServiceOptions {
  launch?: ServiceLaunch;
  configuration_revision?: string;
  allow_model?: boolean;
  acquire_page?: boolean;
}
export class PluginServiceError extends Error {
  constructor(message: string, readonly status: number, readonly details?: Record<string, unknown>) { super(message); }
}
const path = (plugin: string, service: string) => `/api/plugins/${encodeURIComponent(plugin)}/services/${encodeURIComponent(service)}`;
async function call<T>(method: string, url: string, body?: unknown): Promise<T> {
  const result = await requestResult<T>(method, url, body, true);
  if (!result.ok) throw new PluginServiceError(result.error, result.status, result.details);
  return result.data;
}
export const getPluginService = (plugin: string, service: string) => call<PluginServiceStatus>("GET", path(plugin, service));
export const startPluginService = (plugin: string, service: string, launch: ServiceLaunch, options: Omit<EnsureServiceOptions, "launch" | "acquire_page"> = {}) => call<PluginServiceStatus>("POST", `${path(plugin, service)}/start`, { launch, ...options });
export const ensurePluginService = (plugin: string, service: string, options: EnsureServiceOptions = {}) => call<PluginServiceStatus>("POST", `${path(plugin, service)}/ensure`, options);
export const stopPluginService = (plugin: string, service: string) => call<PluginServiceStatus>("POST", `${path(plugin, service)}/stop`);
export const savePluginServiceCredential = (plugin: string, service: string, configuration_revision: string, token: string) =>
  call<void>("PUT", `${path(plugin, service)}/credential`, { configuration_revision, token });
export const revokePluginServiceCredential = (plugin: string, service: string, configuration_revision: string) =>
  call<void>("DELETE", `${path(plugin, service)}/credential`, { configuration_revision });
export async function bindPluginServiceChannel(plugin: string, settings: string, key: string, channelId: string) {
  const url = `/api/plugins/${encodeURIComponent(plugin)}/settings/${encodeURIComponent(settings)}`;
  const data = await call<{ values: Record<string, unknown> }>("GET", url);
  return call("PUT", url, { values: { ...data.values, [key]: channelId } });
}
