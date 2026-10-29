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

  it("identifies missing vision credentials and keeps the original log", () => {
    const error=presentExecutionError("No API key configured","vision_analysis");
    expect(error.message).toContain("图像理解服务所选厂商账户尚未配置 API 密钥");
    expect(error.technical).toBe("No API key configured");
    expect(error.action).toEqual({label:"检查图像理解配置",href:"/settings?section=vision"});
  });

  it("keeps a precise structured service message and directs it to the matching settings", () => {
    const message="图像生成服务所选厂商账户没有可用 API Key，请确认所选账户。";
    const error=presentExecutionError({code:"BASIC_SERVICE_NO_API_KEY",message},"image_generation");
    expect(error.message).toBe(message);
    expect(error.action?.href).toBe("/settings?section=image");
  });

  it("distinguishes unavailable secret storage from an absent image", () => {
    expect(presentExecutionError({code:"BASIC_SERVICE_SECRET_UNAVAILABLE",message:"SecretUnavailable"},"vision_analysis").message).toContain("系统凭据库暂不可用");
    expect(presentExecutionError("secret store unavailable","vision_analysis").message).toContain("系统凭据库暂不可用");
    const error=presentExecutionError("识图需要显式提供 resource_id 或 image_url；仅有文字不能识图。","vision_analysis");
    expect(error.message).toContain("仅有文字不能识图");
    expect(error.action).toBeUndefined();
  });
});
