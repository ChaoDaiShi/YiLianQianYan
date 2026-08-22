import type { McpServer } from "../../api/client";

export interface McpDraft {
  name: string;
  transport: "stdio" | "streamable_http";
  command: string;
  args: string;
  url: string;
  env: string;
  editing: boolean;
}

export type McpDraftValidation =
  | { ok: true; value: Partial<McpServer> }
  | { ok: false; error: string };

export function validateMcpDraft(draft: McpDraft): McpDraftValidation {
  const name = draft.name.trim();
  if (!name) return { ok: false, error: "请输入服务名称。" };

  let args: string[] = [];
  if (draft.args.trim()) {
    try {
      const parsed = JSON.parse(draft.args) as unknown;
      if (!Array.isArray(parsed) || parsed.some((value) => typeof value !== "string")) {
        return { ok: false, error: "参数必须是 JSON 字符串数组。" };
      }
      args = parsed;
    } catch {
      return { ok: false, error: "参数必须是 JSON 字符串数组。" };
    }
  }

  let env: Record<string, string> | undefined;
  if (draft.env.trim()) {
    try {
      const parsed = JSON.parse(draft.env) as unknown;
      if (!parsed || Array.isArray(parsed) || typeof parsed !== "object" || Object.entries(parsed).some(([key, value]) => !key.trim() || typeof value !== "string")) {
        return { ok: false, error: "环境变量必须是键和值均为字符串的 JSON 对象。" };
      }
      env = parsed as Record<string, string>;
    } catch {
      return { ok: false, error: "环境变量必须是键和值均为字符串的 JSON 对象。" };
    }
  } else if (!draft.editing) {
    env = {};
  }

  if (draft.transport === "stdio") {
    const command = draft.command.trim();
    if (!command) return { ok: false, error: "Stdio 服务必须填写启动命令。" };
    return { ok: true, value: { name, transport: "stdio", command, args, ...(env === undefined ? {} : { env }) } };
  }

  const url = draft.url.trim();
  try {
    const parsed = new URL(url);
    if (parsed.protocol !== "http:" && parsed.protocol !== "https:") throw new Error("unsupported protocol");
  } catch {
    return { ok: false, error: "请输入有效的 HTTP 或 HTTPS MCP 地址。" };
  }
  return { ok: true, value: { name, transport: "streamable_http", url, args, ...(env === undefined ? {} : { env }) } };
}
