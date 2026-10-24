import { requestResult } from "../core/api/http";
export interface ChannelProvider { id: string; label: string; supported: boolean; protocol: string; docs_url: string; limitations: string[] }
export type ChannelReplyMode = "automatic" | "manual";
export type ChannelGroupTrigger = "mention" | "all";
export interface ExternalChannel {
  id: string; name: string; provider: string; enabled: boolean; app_id: string | null;
  api_base_url: string | null; self_id: string | null; allowed_senders: string[]; allowed_groups: string[]; group_trigger: ChannelGroupTrigger;
  connection_mode: "http" | "websocket" | "reverse_websocket" | "polling";
  preset_id: string | null;
  reply_mode: ChannelReplyMode;
  status: "disabled" | "configured" | "connecting" | "connected" | "awaiting_login" | "receiving" | "error";
  last_received_at: number | null;
  last_event: { at: number; kind: string; sender_id: string | null; detail: string } | null;
  last_error: string | null; created_at: number; updated_at: number;
  callback_path: string; credentials_configured: boolean;
}
export interface ChannelMessage {
  id: string; channel_id: string; conversation_id: string; event_id: string; external_message_id: string;
  sender_id: string; peer_type: "private" | "group"; peer_id: string; content: string; draft_reply: string | null;
  status: "queued" | "running" | "awaiting_send" | "failed" | "sending" | "sent" | "delivery_unknown" | "send_failed" | "dismissed";
  send_mode: ChannelReplyMode | null;
  error: string | null; reply_message_id: string | null; approved_at: number | null; created_at: number; updated_at: number;
}
export interface ChannelSession { id: string; channel_id: string; peer_id: string; peer_type: "private" | "group"; conversation_id: string; created_at: number }
export interface ChannelInput {
  name: string; provider: string; enabled: boolean; app_id: string | null; api_base_url: string | null;
  self_id: string | null; allowed_senders: string[]; allowed_groups: string[]; group_trigger: ChannelGroupTrigger;
  connection_mode: "http" | "websocket" | "reverse_websocket" | "polling";
  preset_id: string | null;
  reply_mode: ChannelReplyMode;
  secrets?: { app_secret?: string; verification_token?: string; access_token?: string; event_secret?: string };
}
export interface ChannelPreset {
  id: string; name: string; agent_id: string | null; model_profile_id: string | null;
  instructions: string; builtin: boolean; created_at: number; updated_at: number;
}
export interface ChannelLogin {
  status: string; qr_code_data_url: string | null; expires_at: number | null;
  bot_id: string | null; error: string | null;
}
export interface ChannelCheckItem { label: string; ok: boolean; detail: string }
export interface ChannelPreview { reply: string; conversation_id: string }
async function call<T>(method: string, path: string, body?: unknown): Promise<T> {
  const result = await requestResult<T>(method, path, body);
  if (!result.ok) throw new Error(result.error);
  return result.data;
}
const path = (id: string) => `/api/channels/${encodeURIComponent(id)}`;
export const listChannelProviders = () => call<{ providers: ChannelProvider[] }>("GET", "/api/channels/providers");
export const listChannels = () => call<{ channels: ExternalChannel[] }>("GET", "/api/channels");
export const createChannel = (input: ChannelInput) => call<{ channel: ExternalChannel }>("POST", "/api/channels", input);
export const updateChannel = (id: string, input: ChannelInput) => call<{ channel: ExternalChannel }>("PUT", path(id), input);
export const setChannelEnabled = (id: string, enabled: boolean) => call<{ channel: ExternalChannel }>("POST", `${path(id)}/enabled`, { enabled });
export const setChannelReplyMode = (id: string, reply_mode: ChannelReplyMode) => call<{ channel: ExternalChannel }>("POST", `${path(id)}/reply-mode`, { reply_mode });
export const listChannelMessages = (id: string) => call<{ messages: ChannelMessage[] }>("GET", `${path(id)}/messages`);
export const listChannelSessions = (id: string) => call<{ sessions: ChannelSession[] }>("GET", `${path(id)}/sessions`);
export const approveChannelSend = (id: string, messageId: string, text: string) => call<{ message: ChannelMessage }>("POST", `${path(id)}/messages/${encodeURIComponent(messageId)}/approve-send`, { approved: true, text });
export const deleteChannel = (id: string) => call<{ deleted: true }>("DELETE", path(id));
export const checkChannel = (id: string) => call<{ checks: ChannelCheckItem[]; channel: ExternalChannel }>("POST", `${path(id)}/check`);
export const listChannelPresets = () => call<{ presets: ChannelPreset[] }>("GET", "/api/channels/presets");
export const createChannelPreset = (input: Pick<ChannelPreset, "name" | "agent_id" | "model_profile_id" | "instructions">) => call<{ preset: ChannelPreset }>("POST", "/api/channels/presets", input);
export const updateChannelPreset = (id: string, input: Pick<ChannelPreset, "name" | "agent_id" | "model_profile_id" | "instructions">) => call<{ preset: ChannelPreset }>("PUT", `/api/channels/presets/${encodeURIComponent(id)}`, input);
export const deleteChannelPreset = (id: string) => call<{ deleted: true }>("DELETE", `/api/channels/presets/${encodeURIComponent(id)}`);
export const previewChannelPreset = (id: string, message: string) => call<ChannelPreview>("POST", `/api/channels/presets/${encodeURIComponent(id)}/preview`, { message });
export const startChannelLogin = (id: string) => call<{ login: ChannelLogin }>("POST", `${path(id)}/login`);
export const getChannelLogin = (id: string) => call<{ login: ChannelLogin }>("GET", `${path(id)}/login`);
export const verifyChannelLogin = (id: string, code: string) => call<{ login: ChannelLogin }>("POST", `${path(id)}/login/verify`, { code });
export const cancelChannelLogin = (id: string) => call<{ login: ChannelLogin }>("DELETE", `${path(id)}/login`);
export const regenerateChannelMessage = (id: string, messageId: string) => call<{ message: ChannelMessage }>("POST", `${path(id)}/messages/${encodeURIComponent(messageId)}/regenerate`);
export const dismissChannelMessage = (id: string, messageId: string) => call<{ message: ChannelMessage }>("POST", `${path(id)}/messages/${encodeURIComponent(messageId)}/dismiss`);
