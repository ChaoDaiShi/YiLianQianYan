import { requestResult } from "./transport";
export type AccessMode = "full" | "approve" | "deny";
export type PermissionId = "filesystem.read" | "filesystem.write" | "shell.execute" | "process.inspect" | "process.control" | "network.request" | "desktop.observe" | "desktop.interact" | "skill.load" | "agent.plan" | "mcp.invoke" | "agent.delegate" | "messaging.read" | "messaging.send";
export interface AccessRule { id: string; scope: "default" | "agent" | "capability" | "resource"; subject_id: string | null; permission_id: PermissionId; resource: Record<string, unknown> | null; mode: AccessMode; }
export const accessPermissions: Array<{id: PermissionId; label: string}> = [
 {id:"messaging.read",label:"读取渠道消息与联系人"},{id:"messaging.send",label:"发送渠道消息"},
 {id:"filesystem.read",label:"读取文件"},{id:"filesystem.write",label:"修改文件"},{id:"shell.execute",label:"运行命令"},{id:"process.inspect",label:"查看进程"},{id:"process.control",label:"控制进程"},{id:"network.request",label:"网络请求"},{id:"desktop.observe",label:"观察屏幕"},{id:"desktop.interact",label:"操作桌面"},{id:"skill.load",label:"读取 Skill"},{id:"agent.plan",label:"任务规划"},{id:"mcp.invoke",label:"调用 MCP"},{id:"agent.delegate",label:"委派智能体"},
];
export const getAccessRules = () => requestResult<{rules: AccessRule[]}>("GET", "/api/security/access-rules");
export const putAccessRules = (rules: AccessRule[]) => requestResult<{rules: AccessRule[]}>("PUT", "/api/security/access-rules", {rules});
