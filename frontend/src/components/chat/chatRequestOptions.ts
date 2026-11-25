export type ApprovalMode = "full_approval" | "partial_approval" | "full_access";
export type ThinkingMode = "standard" | "light" | "balanced" | "thinking" | "maximum";
import type { WorkspaceMode } from "../../types";
export type { WorkspaceMode } from "../../types";
export interface ChatRequestOptions { model_profile_id?: string; agent_id?: string; thinking_mode?: ThinkingMode; workspace_mode?: WorkspaceMode; resource_ids?: string[] }
export const workspaceModes = [
  { value: "auto", label: "自动", description: "根据本轮需求选择交流、查阅或执行" },
  { value: "chat", label: "聊天", description: "交流、问答和方案讨论" },
  { value: "work", label: "工作", description: "在工作区执行与交付任务" },
  { value: "create", label: "创造", description: "创作内容与探索想法" },
] as const;
export const approvalModes = [
  { value: "full_approval", label: "完全审批", description: "每次操作都先征得你的批准。" },
  { value: "partial_approval", label: "部分审批", description: "常规操作直接执行，敏感或危险操作请求批准。" },
  { value: "full_access", label: "完全权限", description: "允许直接执行操作，已设置的禁止范围仍然生效。" },
] as const;
export const thinkingLevels: Record<ThinkingMode, { label: string; description: string }> = {
  standard: { label: "标准", description: "使用当前模型的常规响应设置" },
  light: { label: "轻度", description: "适合提取信息与简短判断" },
  balanced: { label: "适中", description: "适合比较方案与多步分析" },
  thinking: { label: "深度", description: "适合复杂推理与长任务" },
  maximum: { label: "最高", description: "为复杂任务分配更多推理资源" },
};
export function availableThinkingModes(provider: string, model: string): ThinkingMode[] {
  if (provider === "deepseek") {
    return /^(deepseek-flash|deepseek-v4-flash|deepseek-v4-pro)$/.test(model)
      ? ["standard", "light", "thinking", "maximum"]
      : ["standard", "thinking"];
  }
  if (provider === "openai" && /^(gpt-5|o[134](?:-|$))/.test(model)
      && !/-(pro|search)(-|$)|^o1-(mini|preview)/.test(model)) {
    return ["standard", "light", "balanced", "thinking"];
  }
  return ["standard"];
}
export function supportsThinking(provider: string, model: string): boolean {
  return availableThinkingModes(provider, model).length > 1;
}
export function buildChatRequestOptions(profile: string, thinking: ThinkingMode, mode?: WorkspaceMode, agentId?: string): ChatRequestOptions {
  return { ...(profile ? { model_profile_id: profile } : {}), ...(agentId && !profile.startsWith("cli:") ? { agent_id: agentId } : {}), thinking_mode: thinking, ...(mode ? { workspace_mode: mode } : {}) };
}
