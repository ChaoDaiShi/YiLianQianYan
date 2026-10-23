export type ApprovalMode = "full_approval" | "partial_approval" | "full_access";
export type ThinkingMode = "standard" | "thinking";
import type { WorkspaceMode } from "../../types";
export type { WorkspaceMode } from "../../types";
export interface ChatRequestOptions { model_profile_id?: string; thinking_mode?: ThinkingMode; workspace_mode?: WorkspaceMode; resource_ids?: string[] }
export const workspaceModes = [
  { value: "chat", label: "聊天", description: "轻松交流与问答" },
  { value: "work", label: "工作", description: "在工作区执行与交付任务" },
  { value: "create", label: "创造", description: "创作内容与探索想法" },
] as const;
export const approvalModes = [
  { value: "full_approval", label: "完全审批", description: "每次操作都先征得你的批准。" },
  { value: "partial_approval", label: "部分审批", description: "常规操作直接执行，敏感或危险操作请求批准。" },
  { value: "full_access", label: "完全权限", description: "允许直接执行操作，已设置的禁止范围仍然生效。" },
] as const;
export function supportsThinking(provider: string, model: string): boolean {
  return provider === "deepseek" || (provider === "openai" && /^(gpt-5|o[134](?:-|$))/.test(model));
}
export function buildChatRequestOptions(profile: string, thinking: ThinkingMode, mode?: WorkspaceMode): ChatRequestOptions {
  return { ...(profile ? { model_profile_id: profile } : {}), thinking_mode: thinking, ...(mode ? { workspace_mode: mode } : {}) };
}
