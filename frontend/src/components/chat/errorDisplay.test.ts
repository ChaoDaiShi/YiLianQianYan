import { describe, expect, it } from "vitest";
import { presentExecutionError } from "./errorDisplay";

describe("human-readable execution errors", () => {
  it.each([
    [
      "connection refused",
      "无法连接到服务，请确认后端或目标应用正在运行后重试。",
    ],
    [
      "request timed out after 30s",
      "操作等待超时，目标服务可能响应较慢，请稍后重试。",
    ],
    [
      "HTTP 401 unauthorized",
      "身份验证失败，请检查对应服务的账号或密钥设置。",
    ],
    ["403 access denied", "当前权限不足，无法完成这个操作。"],
    ["404 not found", "没有找到需要访问的资源，请检查路径或地址。"],
    [
      "AbortError: operation cancelled",
      "任务已停止，没有继续执行后续操作。",
    ],
  ])("turns %s into understandable guidance", (raw, expected) => {
    expect(presentExecutionError(raw).message).toBe(expected);
  });

  it("extracts nested structured errors while keeping the raw log", () => {
    const raw = JSON.stringify({
      error: { code: "ECONNREFUSED", message: "connection refused" },
    });
    expect(presentExecutionError(raw)).toEqual({
      message: "无法连接到服务，请确认后端或目标应用正在运行后重试。",
      technical: raw,
      code: "ECONNREFUSED",
    });
  });

  it("preserves an understandable Chinese reason", () => {
    expect(presentExecutionError('{"error":"模型返回内容为空"}').message).toBe(
      "模型返回内容为空",
    );
  });

  it("does not expose unknown raw English or stack text in the user message", () => {
    const error = presentExecutionError(
      "InternalThingError: opaque failure\n at module.ts:10",
    );
    expect(error.message).toBe(
      "任务执行未成功，请查看错误日志了解具体原因后重试。",
    );
    expect(error.technical).toContain("module.ts:10");
  });
});
