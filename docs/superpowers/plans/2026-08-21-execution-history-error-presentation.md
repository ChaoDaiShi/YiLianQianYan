# Execution History and Error Presentation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. The user explicitly prohibited subagents, so execute inline. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make execution history explain what actually happened, expose real technical details on demand, and turn raw failures into understandable Chinese error descriptions.

**Architecture:** Keep the existing persisted execution record as the single source of truth. Add deterministic presentation helpers for action summaries and error descriptions, then consume them from ExecutionHistory, ToolCallCard, and MessageList without changing SSE, Store, API, database, or backend behavior.

**Tech Stack:** React 18, TypeScript, Vite, Vitest, existing CSS variables and execution state model.

## Global Constraints

- Do not use subagents.
- Do not modify SSE event semantics or Agent execution behavior.
- Do not change API Schema, Store state, database structure, or persisted execution fields.
- Derive display text only from real `name`, `args`, `result`, `reason`, status, and timestamps.
- Do not invent tool purpose, execution success, verification, or error causes.
- Keep raw errors, stack text, stderr, JSON, and call IDs out of the default user layer.
- Keep technical detail controls keyboard accessible and visibly focused.
- Add no dependency and no persistent animation.

---

### Task 1: Add Deterministic Tool Action Summaries

**Files:**
- Modify: `frontend/src/components/chat/toolDisplay.ts`
- Modify: `frontend/src/components/chat/toolDisplay.test.ts`

**Interfaces:**
- Produces: `formatToolActionSummary(name: string, args: Record<string, unknown>): string`.
- Preserves: `formatToolDisplayName`, `formatToolActivity`, `formatToolResultSummary`, `formatToolStatus`, and `formatElapsed`.

- [ ] **Step 1: Write failing action-summary tests**

Add these assertions to `toolDisplay.test.ts`:

```ts
import { formatToolActionSummary } from "./toolDisplay";

it("describes observable tool actions instead of call identifiers", () => {
  expect(formatToolActionSummary("powershell", { command: "Get-Process\nSelect-Object -First 5" }))
    .toBe("执行命令：Get-Process");
  expect(formatToolActionSummary("read_file", { path: "C:\\work\\README.md" }))
    .toBe("读取文件：README.md");
  expect(formatToolActionSummary("write_file", { file: "/tmp/report.md" }))
    .toBe("写入文件：report.md");
  expect(formatToolActionSummary("http_request", { url: "https://www.bilibili.com/video/1" }))
    .toBe("请求网络资源：www.bilibili.com/video/1");
  expect(formatToolActionSummary("screenshot", {})).toBe("截取当前屏幕");
});

it("falls back conservatively when an action cannot be determined", () => {
  expect(formatToolActionSummary("powershell", {})).toBe("调用 PowerShell");
  expect(formatToolActionSummary("mcp_server_tool", {})).toBe("调用 MCP · Tool");
});

it("keeps action summaries single-line and bounded", () => {
  const summary = formatToolActionSummary("bash", { command: `echo ${"x".repeat(120)}` });
  expect(summary).not.toContain("\n");
  expect(summary.length).toBeLessThanOrEqual(84);
});
```

- [ ] **Step 2: Run tests and verify RED**

Working directory: `frontend`

Run: `npm.cmd test -- --run src/components/chat/toolDisplay.test.ts`

Expected: FAIL because `formatToolActionSummary` is not exported.

- [ ] **Step 3: Implement the action formatter**

Add focused private helpers to `toolDisplay.ts`:

```ts
const SUMMARY_LIMIT = 84;

function stringArg(args: Record<string, unknown>, ...keys: string[]): string | null {
  for (const key of keys) {
    const value = args[key];
    if (typeof value === "string" && value.trim()) return value.trim();
  }
  return null;
}

function oneLine(value: string, limit = SUMMARY_LIMIT): string {
  const firstLine = value.split(/\r?\n/).find((line) => line.trim())?.trim() || "";
  return firstLine.length > limit ? `${firstLine.slice(0, limit - 1)}…` : firstLine;
}

function baseName(value: string): string {
  const parts = value.replace(/\\/g, "/").split("/").filter(Boolean);
  return parts.at(-1) || value;
}

function displayUrl(value: string): string {
  try {
    const url = new URL(value);
    return `${url.host}${url.pathname === "/" ? "" : url.pathname}`;
  } catch {
    return oneLine(value);
  }
}
```

Export `formatToolActionSummary` with these exact priorities:

```ts
export function formatToolActionSummary(
  name: string,
  args: Record<string, unknown>,
): string {
  const command = stringArg(args, "command", "script");
  const path = stringArg(args, "path", "file", "target", "file_path");
  const url = stringArg(args, "url", "uri", "href");
  const query = stringArg(args, "query", "pattern", "text");

  if ((name === "bash" || name === "powershell") && command) {
    return oneLine(`执行命令：${oneLine(command, 78)}`);
  }
  if (name === "read_file" && path) return oneLine(`读取文件：${baseName(path)}`);
  if (name === "write_file" && path) return oneLine(`写入文件：${baseName(path)}`);
  if (name === "edit_file" && path) return oneLine(`编辑文件：${baseName(path)}`);
  if (name === "grep" && query) return oneLine(`搜索内容：${query}`);
  if (name === "glob" && query) return oneLine(`查找文件：${query}`);
  if ((name === "http_request" || name.toLowerCase() === "http") && url) {
    return oneLine(`请求网络资源：${displayUrl(url)}`);
  }
  if (name === "process") return "管理系统进程";
  if (name === "screenshot") return "截取当前屏幕";
  if (name === "mouse") return "执行鼠标操作";
  if (name === "keyboard") return "执行键盘操作";
  return `调用 ${formatToolDisplayName(name)}`;
}
```

- [ ] **Step 4: Run tests and verify GREEN**

Run: `npm.cmd test -- --run src/components/chat/toolDisplay.test.ts`

Expected: all tool display tests PASS.

- [ ] **Step 5: Commit Task 1**

```powershell
git add -- frontend/src/components/chat/toolDisplay.ts frontend/src/components/chat/toolDisplay.test.ts
git commit -m "feat(ui): describe observable tool actions"
```

---

### Task 2: Add Human-Readable Error Descriptions

**Files:**
- Create: `frontend/src/components/chat/errorDisplay.ts`
- Create: `frontend/src/components/chat/errorDisplay.test.ts`

**Interfaces:**
- Produces: `ErrorPresentation { message: string; technical: string; code?: string }`.
- Produces: `presentExecutionError(raw: unknown): ErrorPresentation`.
- Consumes only the raw error value already received from SSE or tool results.

- [ ] **Step 1: Write failing error-presentation tests**

Create `errorDisplay.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { presentExecutionError } from "./errorDisplay";

describe("human-readable execution errors", () => {
  it.each([
    ["connection refused", "无法连接到服务，请确认后端或目标应用正在运行后重试。"],
    ["request timed out after 30s", "操作等待超时，目标服务可能响应较慢，请稍后重试。"],
    ["HTTP 401 unauthorized", "身份验证失败，请检查对应服务的账号或密钥设置。"],
    ["403 access denied", "当前权限不足，无法完成这个操作。"],
    ["404 not found", "没有找到需要访问的资源，请检查路径或地址。"],
    ["AbortError: operation cancelled", "任务已停止，没有继续执行后续操作。"],
  ])("turns %s into understandable guidance", (raw, expected) => {
    expect(presentExecutionError(raw).message).toBe(expected);
  });

  it("extracts nested structured errors while keeping the raw log", () => {
    const raw = JSON.stringify({ error: { code: "ECONNREFUSED", message: "connection refused" } });
    expect(presentExecutionError(raw)).toEqual({
      message: "无法连接到服务，请确认后端或目标应用正在运行后重试。",
      technical: raw,
      code: "ECONNREFUSED",
    });
  });

  it("preserves an understandable Chinese reason", () => {
    expect(presentExecutionError('{"error":"模型返回内容为空"}').message)
      .toBe("模型返回内容为空");
  });

  it("does not expose unknown raw English or stack text in the user message", () => {
    const error = presentExecutionError("InternalThingError: opaque failure\n at module.ts:10");
    expect(error.message).toBe("任务执行未成功，请查看错误日志了解具体原因后重试。");
    expect(error.technical).toContain("module.ts:10");
  });
});
```

- [ ] **Step 2: Run tests and verify RED**

Run: `npm.cmd test -- --run src/components/chat/errorDisplay.test.ts`

Expected: FAIL because `errorDisplay.ts` does not exist.

- [ ] **Step 3: Implement structured extraction and known-category mapping**

Create `errorDisplay.ts` with:

```ts
export interface ErrorPresentation {
  message: string;
  technical: string;
  code?: string;
}

function record(value: unknown): Record<string, unknown> | null {
  return value && typeof value === "object" && !Array.isArray(value)
    ? value as Record<string, unknown>
    : null;
}

function parseRaw(raw: unknown): unknown {
  if (typeof raw !== "string") return raw;
  try { return JSON.parse(raw); } catch { return raw; }
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
  const technical = typeof raw === "string"
    ? raw
    : JSON.stringify(raw ?? "", null, 2);
  return {
    message: understandableMessage(reason, code),
    technical,
    ...(code ? { code } : {}),
  };
}
```

- [ ] **Step 4: Run tests and verify GREEN**

Run: `npm.cmd test -- --run src/components/chat/errorDisplay.test.ts`

Expected: all error presentation tests PASS.

- [ ] **Step 5: Commit Task 2**

```powershell
git add -- frontend/src/components/chat/errorDisplay.ts frontend/src/components/chat/errorDisplay.test.ts
git commit -m "feat(ui): translate execution errors for users"
```

---

### Task 3: Rebuild Execution History Cards Around Actions

**Files:**
- Modify: `frontend/src/features/execution/ExecutionHistory.tsx`
- Create: `frontend/src/features/execution/executionHistoryPresentation.test.ts`
- Modify: `frontend/src/index.css`

**Interfaces:**
- Consumes: `formatToolActionSummary`, `formatToolDisplayName`, `formatElapsed`, and `presentExecutionError`.
- Preserves: `ExecutionRecord` and existing selectors.
- Produces accessible expandable history cards without changing record storage.

- [ ] **Step 1: Write failing history presentation contracts**

Create `executionHistoryPresentation.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import historySource from "./ExecutionHistory.tsx?raw";

describe("execution history presentation", () => {
  it("uses semantic action summaries in the primary layer", () => {
    expect(historySource).toContain("formatToolActionSummary(record.name, record.args)");
    expect(historySource).not.toContain("record.startedAt !== undefined\n                  ? formatElapsed");
  });

  it("supports accessible expandable technical details", () => {
    expect(historySource).toContain("aria-expanded={expanded}");
    expect(historySource).toContain("Tool Call ID");
    expect(historySource).toContain("Arguments");
    expect(historySource).toContain("错误日志");
  });

  it("keeps call identifiers inside technical details", () => {
    const actionIndex = historySource.indexOf("execution-history-action");
    const idIndex = historySource.indexOf("Tool Call ID");
    expect(actionIndex).toBeGreaterThan(-1);
    expect(idIndex).toBeGreaterThan(actionIndex);
  });
});
```

- [ ] **Step 2: Run tests and verify RED**

Run: `npm.cmd test -- --run src/features/execution/executionHistoryPresentation.test.ts`

Expected: FAIL because the current history items are not expandable and do not call the action formatter.

- [ ] **Step 3: Extract one expandable history item**

In `ExecutionHistory.tsx`:

- import `useState`, `ChevronDown`, `ChevronUp`, `formatToolActionSummary`, and `presentExecutionError`;
- keep existing status/tone maps;
- add `formatValue(value: unknown)` using `JSON.stringify(value, null, 2)` with a `String(value)` fallback;
- add a private `ExecutionHistoryItem({ record })` component;
- make its top-level action a button with `aria-expanded`, `aria-controls`, and visible action summary;
- render Tool display name and elapsed time as secondary metadata;
- never render `toolCallId` before the expandable detail region;
- render existing status badges in the collapsed section;
- when a failed record has `result`, render `presentExecutionError(record.result).message` below the badges;
- for rejected/cancelled records, render the existing explicit non-execution status and do not call them failures.

Use this detail structure only when expanded:

```tsx
<div id={`execution-details-${record.toolCallId}`} className="execution-history-details">
  <dl className="space-y-2 text-xs">
    <div><dt>Tool Name</dt><dd>{record.name}</dd></div>
    <div><dt>Tool Call ID</dt><dd>{record.toolCallId}</dd></div>
    {typeof record.args.command === "string" ? (
      <div><dt>Command</dt><dd>{record.args.command}</dd></div>
    ) : null}
    <div><dt>Arguments</dt><dd>{formatValue(record.args)}</dd></div>
    {record.reason ? <div><dt>操作原因</dt><dd>{record.reason}</dd></div> : null}
    {record.riskLevel !== "unknown" ? <div><dt>风险等级</dt><dd>{record.riskLevel}</dd></div> : null}
    {record.result ? (
      <div>
        <dt>{record.executionStatus === "failed" ? "错误日志" : "执行结果"}</dt>
        <dd>{record.result}</dd>
      </div>
    ) : null}
    {record.verificationReason ? <div><dt>验证信息</dt><dd>{record.verificationReason}</dd></div> : null}
  </dl>
</div>
```

Map `records` to `<ExecutionHistoryItem key={record.toolCallId} record={record} />`.

- [ ] **Step 4: Add bounded history detail styles**

In `index.css`, add semantic styles under the execution section:

```css
.execution-history-action {
  transition: background-color var(--motion-fast) ease;
}

.execution-history-action:hover {
  background: var(--surface-hover);
}

.execution-history-details {
  border-top: 1px solid var(--divider);
  background: var(--surface-muted);
  padding: 0.75rem;
}

.execution-history-details dd {
  margin-top: 0.2rem;
  overflow-wrap: anywhere;
  color: var(--text-secondary);
}

.execution-history-technical-value {
  max-height: 12rem;
  overflow: auto;
  white-space: pre-wrap;
  border: 1px solid var(--border-soft);
  border-radius: var(--radius-sm);
  background: var(--code-bg);
  padding: 0.55rem;
  color: var(--code-text);
  font-family: "IBM Plex Mono", "Cascadia Code", monospace;
}
```

- [ ] **Step 5: Run history and visual tests GREEN**

Run: `npm.cmd test -- --run src/features/execution/executionHistoryPresentation.test.ts src/components/chat/conversationThemeVisual.test.ts src/features/execution/reducer.test.ts`

Expected: all selected tests PASS.

- [ ] **Step 6: Commit Task 3**

```powershell
git add -- frontend/src/features/execution/ExecutionHistory.tsx frontend/src/features/execution/executionHistoryPresentation.test.ts frontend/src/index.css
git commit -m "feat(ui): add expandable semantic execution history"
```

---

### Task 4: Apply Error Presentation to Conversation and Tool Failures

**Files:**
- Modify: `frontend/src/components/chat/MessageList.tsx`
- Modify: `frontend/src/components/chat/ToolCallCard.tsx`
- Modify: `frontend/src/components/chat/messageRendering.test.ts`
- Modify: `frontend/src/components/chat/toolDisplay.test.ts`

**Interfaces:**
- Consumes: `presentExecutionError(raw)` from Task 2.
- Preserves: retry behavior, raw SSE error storage, ToolCallCard expansion, and Tool result rendering.

- [ ] **Step 1: Add failing user-layer/error-log contracts**

Append to `messageRendering.test.ts`:

```ts
it("shows a readable error message and keeps raw fields in an error log", () => {
  expect(messageListSource).toContain("presentExecutionError(error)");
  expect(messageListSource).toContain("presentation.message");
  expect(messageListSource).toContain("查看错误日志");
  expect(messageListSource).toContain("presentation.technical");
});
```

Extend the technical-details test in `toolDisplay.test.ts`:

```ts
expect(toolCallCardSource).toContain("presentExecutionError(result)");
expect(toolCallCardSource).toContain('effectiveStatus === "error" ? "错误日志" : "Raw Result"');
```

- [ ] **Step 2: Run tests and verify RED**

Run: `npm.cmd test -- --run src/components/chat/messageRendering.test.ts src/components/chat/toolDisplay.test.ts`

Expected: FAIL because neither component uses the error presenter.

- [ ] **Step 3: Replace generic conversation error copy with the readable message**

In `MessageList.tsx`, import `presentExecutionError`, compute:

```ts
const presentation = presentExecutionError(error);
```

Keep the heading `任务没有成功完成`. Replace the generic description with `{presentation.message}`. Rename the details summary to `查看错误日志`, and render `{presentation.technical}` in the existing technical `<pre>`. If `presentation.code` exists, render `错误代码：{presentation.code}` above the pre inside the details region.

- [ ] **Step 4: Give failed tool cards a readable summary and raw error log**

In `ToolCallCard.tsx`, import `presentExecutionError`. Compute only when `effectiveStatus === "error" && result`:

```ts
const errorPresentation =
  effectiveStatus === "error" && result ? presentExecutionError(result) : null;
```

Use `errorPresentation?.message || "这个操作没有成功，请展开错误日志了解原因。"` as the failed summary. In technical details, change the result label to `错误日志` for failed cards and keep `ToolResultContent` fed by the unmodified raw `result`.

- [ ] **Step 5: Run component contracts GREEN**

Run: `npm.cmd test -- --run src/components/chat/errorDisplay.test.ts src/components/chat/messageRendering.test.ts src/components/chat/toolDisplay.test.ts`

Expected: all selected tests PASS.

- [ ] **Step 6: Commit Task 4**

```powershell
git add -- frontend/src/components/chat/MessageList.tsx frontend/src/components/chat/ToolCallCard.tsx frontend/src/components/chat/messageRendering.test.ts frontend/src/components/chat/toolDisplay.test.ts
git commit -m "feat(ui): separate readable errors from technical logs"
```

---

### Task 5: Regression and Production Verification

**Files:**
- Verify only unless a regression requires a scoped correction in files listed above.

**Interfaces:**
- Confirms the new presentation layer without changing execution persistence or behavior.

- [ ] **Step 1: Run focused execution-history tests**

Working directory: `frontend`

Run:

```powershell
npm.cmd test -- --run src/components/chat/toolDisplay.test.ts src/components/chat/errorDisplay.test.ts src/components/chat/messageRendering.test.ts src/features/execution/executionHistoryPresentation.test.ts src/features/execution/reducer.test.ts
```

Expected: all selected tests PASS.

- [ ] **Step 2: Run the complete frontend suite**

Run: `npm.cmd test -- --run`

Expected: all frontend tests PASS, including existing approval, execution, history hydration, and conversation tests.

- [ ] **Step 3: Run the production build**

Run: `npm.cmd run build`

Expected: TypeScript and Vite production build PASS with no new warning.

- [ ] **Step 4: Verify repository hygiene**

Working directory: repository root

Run: `git diff --check`

Run: `git status --short --branch`

Expected: no whitespace errors and no uncommitted implementation changes after task commits.

- [ ] **Step 5: Report the final branch state**

Report branch, HEAD, commits, modified files, history interaction, error descriptions, test totals, build status, push status, and any remaining limitation. Do not merge or push without explicit authorization.
