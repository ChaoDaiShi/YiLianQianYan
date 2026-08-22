import { describe, expect, it } from "vitest";
import { validateMcpDraft } from "./mcpForm";

describe("MCP form validation", () => {
  it("requires a usable name and transport target", () => {
    expect(validateMcpDraft({ name: "", transport: "stdio", command: "", args: "", url: "", env: "", editing: false })).toEqual({ ok: false, error: "请输入服务名称。" });
    expect(validateMcpDraft({ name: "Local", transport: "stdio", command: "", args: "", url: "", env: "", editing: false })).toEqual({ ok: false, error: "Stdio 服务必须填写启动命令。" });
    expect(validateMcpDraft({ name: "Remote", transport: "streamable_http", command: "", args: "", url: "ftp://example.com", env: "", editing: false })).toEqual({ ok: false, error: "请输入有效的 HTTP 或 HTTPS MCP 地址。" });
  });

  it("rejects malformed arguments and environment values", () => {
    expect(validateMcpDraft({ name: "Local", transport: "stdio", command: "node", args: "{}", url: "", env: "", editing: false })).toEqual({ ok: false, error: "参数必须是 JSON 字符串数组。" });
    expect(validateMcpDraft({ name: "Local", transport: "stdio", command: "node", args: "[]", url: "", env: '{"PORT": 3000}', editing: false })).toEqual({ ok: false, error: "环境变量必须是键和值均为字符串的 JSON 对象。" });
  });

  it("returns a normalized payload and preserves secrets when editing with an empty env", () => {
    expect(validateMcpDraft({ name: " Local MCP ", transport: "stdio", command: " node ", args: '["server.js"]', url: "", env: "", editing: true })).toEqual({
      ok: true,
      value: { name: "Local MCP", transport: "stdio", command: "node", args: ["server.js"] },
    });
  });
});
