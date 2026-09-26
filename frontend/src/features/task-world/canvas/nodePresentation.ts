import { createContext } from "react";
import type { TaskNodeProjection } from "../taskGraphProjection";

/** A graph role, not a new backend node kind or executable capability. */
export const EntryNodesContext = createContext<ReadonlySet<string>>(new Set());

export function nodePresentation(node: TaskNodeProjection, entry = false) {
  if (node.kind === "approval") return { kind: "approval", label: "审批", executor: "现有审批流程" };
  if (node.kind === "user_checkpoint") return { kind: "human", label: "人工确认", executor: "等待用户" };
  if (node.role === "Output") return { kind: "output", label: "结果", executor: "输出节点" };
  if (node.executor_ref?.startsWith("mcp://")) return { kind: "mcp", label: "MCP · 预留", executor: "能力未接入" };
  return {
    kind: entry ? "entry" : "task", label: entry ? "入口任务" : "任务",
    executor: node.executor_ref?.startsWith("workflow://") ? "工作流" : node.executor_ref ? "已配置执行器" : "未配置执行器",
  };
}
