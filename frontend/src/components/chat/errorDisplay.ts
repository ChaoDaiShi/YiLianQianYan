export interface ErrorPresentation {
  message: string;
  technical: string;
  code?: string;
}

function record(value: unknown): Record<string, unknown> | null {
  return value && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
}

function parseRaw(raw: unknown): unknown {
  if (typeof raw !== "string") return raw;
  try {
    return JSON.parse(raw);
  } catch {
    return raw;
  }
}

function findText(value: unknown): string | null {
  if (typeof value === "string" && value.trim()) return value.trim();
  const object = record(value);
  if (!object) return null;
  for (const key of ["message", "error", "detail", "reason"]) {
    const found = findText(object[key]);
    if (found) return found;
  }
  return null;
}

function findCode(value: unknown): string | undefined {
  const object = record(value);
  if (!object) return undefined;
  for (const key of ["code", "status", "statusCode"]) {
    const candidate = object[key];
    if (typeof candidate === "string" || typeof candidate === "number") {
      return String(candidate);
    }
  }
  for (const key of ["error", "detail"]) {
    const nested = findCode(object[key]);
    if (nested) return nested;
  }
  return undefined;
}

function understandableMessage(reason: string, code?: string): string {
  const value = `${code || ""} ${reason}`.toLowerCase();
  if (/econnrefused|connection refused|failed to fetch|networkerror/.test(value)) {
    return "无法连接到服务，请确认后端或目标应用正在运行后重试。";
  }
  if (/timeout|timed out|etimedout/.test(value)) {
    return "操作等待超时，目标服务可能响应较慢，请稍后重试。";
  }
  if (/\b401\b|unauthorized|invalid.*(?:key|token)|authentication/.test(value)) {
    return "身份验证失败，请检查对应服务的账号或密钥设置。";
  }
  if (/\b403\b|forbidden|access denied|permission denied/.test(value)) {
    return "当前权限不足，无法完成这个操作。";
  }
  if (/\b404\b|not found|enoent/.test(value)) {
    return "没有找到需要访问的资源，请检查路径或地址。";
  }
  if (/abort|cancelled|canceled/.test(value)) {
    return "任务已停止，没有继续执行后续操作。";
  }
  if (/\b429\b|rate limit|too many requests/.test(value)) {
    return "请求过于频繁，请稍等片刻后重试。";
  }
  if (/insufficient[_ ]quota|quota exceeded/.test(value)) {
    return "服务额度不足，请检查对应服务的用量或计费设置。";
  }
  if (/\b5\d\d\b|internal server error|bad gateway|service unavailable/.test(value)) {
    return "服务暂时无法完成请求，请稍后重试。";
  }
  if (/\p{Script=Han}/u.test(reason) && !/[{}\[\]<>]|\n\s*at\s/.test(reason)) {
    return reason;
  }
  return "任务执行未成功，请查看错误日志了解具体原因后重试。";
}

export function presentExecutionError(raw: unknown): ErrorPresentation {
  const parsed = parseRaw(raw);
  const reason = findText(parsed) || "";
  const code = findCode(parsed);
  const technical =
    typeof raw === "string" ? raw : JSON.stringify(raw ?? "", null, 2);
  return {
    message: understandableMessage(reason, code),
    technical,
    ...(code ? { code } : {}),
  };
}
