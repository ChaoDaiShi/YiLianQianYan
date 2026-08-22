import { describe, expect, it } from "vitest";
import { deriveTaskDisplayTitle, looksLikeCommand } from "./taskTitle";

describe("looksLikeCommand", () => {
  it.each([
    'Write-Output "release-isolation..."',
    "Get-Process",
    "npm install",
    "git status",
    "python script.py",
    "echo hello > out.txt",
    "ls -la | grep foo",
    "$env:PATH",
  ])("recognizes %j as a command", (title) => {
    expect(looksLikeCommand(title)).toBe(true);
  });

  it.each([
    "帮我整理一下文件",
    "创建一个 Python 脚本",
    "查看系统进程",
    "搜索包含 TODO 的文件",
  ])("does not treat %j as a command", (title) => {
    expect(looksLikeCommand(title)).toBe(false);
  });
});

describe("deriveTaskDisplayTitle", () => {
  it("keeps natural-language goals", () => {
    expect(deriveTaskDisplayTitle("帮我整理一下文件")).toBe("帮我整理一下文件");
  });

  it("demotes raw commands to a neutral label", () => {
    expect(deriveTaskDisplayTitle('Write-Output "release-isolation..."')).toBe(
      "新任务"
    );
  });

  it("falls back for empty titles", () => {
    expect(deriveTaskDisplayTitle("")).toBe("新任务");
    expect(deriveTaskDisplayTitle(null)).toBe("新任务");
    expect(deriveTaskDisplayTitle(undefined)).toBe("新任务");
  });
});
