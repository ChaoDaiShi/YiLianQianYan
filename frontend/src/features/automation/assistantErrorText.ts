/** Preserve exact failures in storage; explain bounded execution in the UI. */
export function assistantErrorText(error: string): string {
  if (error.includes("proofloop_usage_unavailable")) return "模型服务未返回可核实的用量，本轮已停止。请检查模型设置后显式继续。";
  if (error.includes("proofloop_budget_exhausted")) return "本轮模型调用或用量预算已耗尽，请检查执行记录后显式继续。";
  return error;
}
