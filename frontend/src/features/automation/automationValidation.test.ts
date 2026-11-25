import { describe, expect, it } from "vitest";
import type { Automation } from "../../api/automations";
import type { Workspace } from "../../api/workspaces";
import { isLocalAbsoluteFilePath, prospectiveWorkspaceRoot, validateAutomationDraft, type AutomationDraft } from "./automationValidation";

const workspaces: Workspace[] = [{ id: "old", name: "old", description: "", root_path: "C:/old", status: "active", created_at: 1, updated_at: 1 }, { id: "new", name: "new", description: "", root_path: "D:/new", status: "active", created_at: 1, updated_at: 1 }];
const draft = (): AutomationDraft => ({ name: "报告", targetKind: "personal_assistant", graphId: "", workspaceId: "old", timezone: "Asia/Shanghai", schedule: { kind: "daily", time: "09:00" }, goal: "整理资料", maxRuns: 5 });
describe("automation editor validation", () => {
  it("accepts the normal bounded draft and rejects invalid budgets and workspace states", () => {
    expect(validateAutomationDraft(draft(), workspaces)).toEqual([]);
    for (const maxRuns of [0, 21, 1.5, NaN]) expect(validateAutomationDraft({ ...draft(), maxRuns }, workspaces).some(error => error.includes("1–20"))).toBe(true);
    expect(validateAutomationDraft(draft(), workspaces.map(item => ({ ...item, status: "archived" as const })))).toContain("请选择一个有效且未归档的工作空间");
  });
  it("validates names, targets, time zones and real calendar dates", () => {
    expect(validateAutomationDraft({ ...draft(), name: "a".repeat(121) }, workspaces)).toContain("任务名称最多 120 个字符");
    expect(validateAutomationDraft({ ...draft(), goal: "x".repeat(8001) }, workspaces)).toContain("个人目标最多 8000 个字符");
    expect(validateAutomationDraft({ ...draft(), timezone: "Mars/Olympus" }, workspaces).some(error => error.includes("IANA"))).toBe(true);
    expect(validateAutomationDraft({ ...draft(), schedule: { kind: "once", local_at: "2026-02-30T09:00" } }, workspaces)).toContain("请输入有效的执行日期与时间");
    expect(validateAutomationDraft({ ...draft(), schedule: { kind: "weekly", time: "25:00", weekdays: [] } }, workspaces)).toHaveLength(2);
    expect(validateAutomationDraft({ ...draft(), targetKind: "workflow", graphId: "g", schedule: { kind: "file_changed", path: "C:/notes.txt" } }, workspaces)).toContain("本地文件变化仅可关联个人主动助手");
  });
  it("rejects UNC, traversal, streams and directory paths without touching the filesystem", () => {
    for (const path of ["notes.txt", "\\\\server\\share\\notes.txt", "C:/data/../notes.txt", "C:/data/notes.txt:private", "C:/data/", "C:/data/file. "]) expect(isLocalAbsoluteFilePath(path), path).toBe(false);
    expect(isLocalAbsoluteFilePath("C:/data/notes.txt")).toBe(true);
    expect(isLocalAbsoluteFilePath("\\\\?\\C:\\data\\notes.txt")).toBe(true);
  });
  it("accepts a saved personal watch and checks its source and review budget", () => {
    const schedule = {
      kind: "watch" as const,
      source: { kind: "http" as const, url: "https://example.invalid/status", send_preview: false },
      expires_at: Date.now() + 60_000, min_interval_secs: 10, max_checks: 5, max_retries: 2,
    };
    expect(validateAutomationDraft({ ...draft(), schedule }, workspaces)).toEqual([]);
    expect(validateAutomationDraft({ ...draft(), schedule: { ...schedule, max_retries: 6 } }, workspaces)).toContain("重试上限须在 0 与检查上限之间");
    expect(validateAutomationDraft({ ...draft(), schedule: { ...schedule, source: { ...schedule.source, url: "https://user:token@example.invalid" } } }, workspaces)).toContain("请提供无账号凭据的 HTTP 或 HTTPS 网址");
    expect(validateAutomationDraft({ ...draft(), targetKind: "workflow", graphId: "g", schedule }, workspaces)).toContain("主动关注仅可关联个人主动助手");
  });
  it("displays the newly selected scope rather than the old saved root", () => {
    const existing = { workspace_id: "old", target: { kind: "personal_assistant", root_path: "C:/saved-old" } } as Automation;
    expect(prospectiveWorkspaceRoot(existing, "new", workspaces)).toEqual({ root: "D:/new", changed: true, workspaceRootChanged: false });
    expect(prospectiveWorkspaceRoot(existing, "old", workspaces)).toEqual({ root: "C:/saved-old", changed: false, workspaceRootChanged: true });
  });
});
