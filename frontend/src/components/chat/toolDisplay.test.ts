import { describe, expect, it } from "vitest";
import toolCallCardSource from "./ToolCallCard.tsx?raw";
import {
  formatElapsed,
  formatToolActionSummary,
  formatToolActivity,
  formatToolDisplayName,
  formatToolResultSummary,
  formatToolStatus,
} from "./toolDisplay";

describe("tool display helpers", () => {
  it("formats known and fallback tool names for users", () => {
    expect(formatToolDisplayName("powershell")).toBe("PowerShell");
    expect(formatToolDisplayName("filesystem")).toBe("文件操作");
    expect(formatToolDisplayName("mcp_server_tool")).toBe("MCP · Tool");
    expect(formatToolDisplayName("read_file")).toBe("读取文件");
    expect(formatToolDisplayName("unknown_tool_name")).toBe("Unknown Tool Name");
  });

  it("does not invent a tool purpose when only the tool name is observable", () => {
    expect(formatToolActivity("powershell", {})).toBe("正在调用 PowerShell");
    expect(formatToolActivity("mcp_server_tool", {})).toBe("正在调用 MCP · Tool");
  });

  it("describes observable tool actions instead of call identifiers", () => {
    expect(
      formatToolActionSummary("powershell", {
        command: "Get-Process\nSelect-Object -First 5",
      }),
    ).toBe("执行命令：Get-Process");
    expect(
      formatToolActionSummary("read_file", { path: "C:\\work\\README.md" }),
    ).toBe("读取文件：README.md");
    expect(
      formatToolActionSummary("write_file", { file: "/tmp/report.md" }),
    ).toBe("写入文件：report.md");
    expect(
      formatToolActionSummary("http_request", {
        url: "https://www.bilibili.com/video/1",
      }),
    ).toBe("请求网络资源：www.bilibili.com/video/1");
    expect(formatToolActionSummary("screenshot", {})).toBe("截取当前屏幕");
  });

  it("falls back conservatively when an action cannot be determined", () => {
    expect(formatToolActionSummary("powershell", {})).toBe("调用 PowerShell");
    expect(formatToolActionSummary("mcp_server_tool", {})).toBe("调用 MCP · Tool");
  });

  it("keeps action summaries single-line and bounded", () => {
    const summary = formatToolActionSummary("bash", {
      command: `echo ${"x".repeat(120)}`,
    });
    expect(summary).not.toContain("\n");
    expect(summary.length).toBeLessThanOrEqual(84);
  });

  it("keeps result summaries factual and compact", () => {
    expect(formatToolResultSummary("126 processes")).toBe("工具已返回结果");
    expect(formatToolResultSummary("\n  ")).toBe("工具未返回内容");
  });

  it("formats observable tool states without exposing implementation language", () => {
    expect(formatToolStatus("running")).toBe("正在执行");
    expect(formatToolStatus("success")).toBe("已完成");
    expect(formatToolStatus("error")).toBe("执行失败");
    expect(formatToolStatus("blocked")).toBe("等待你的确认");
  });

  it("formats elapsed time without predicting remaining duration", () => {
    expect(formatElapsed(0, 0)).toBe("已运行 0 秒");
    expect(formatElapsed(0, 2100)).toBe("已运行 2.1 秒");
    expect(formatElapsed(0, 65000)).toBe("已运行 1 分 5 秒");
  });

  it("keeps technical details collapsed behind an accessible control", () => {
    expect(toolCallCardSource).toContain("aria-expanded={expanded}");
    expect(toolCallCardSource).toContain("Raw Result");
    expect(toolCallCardSource).toContain("Tool Name");
    expect(toolCallCardSource).toContain("presentExecutionError(result)");
    expect(toolCallCardSource).toContain(
      'effectiveStatus === "error" ? "错误日志" : "Raw Result"',
    );
  });
});
