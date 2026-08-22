import { describe, expect, it } from "vitest";
import type { Task, Workspace } from "../../api/client";
import type { ConversationSummary } from "../../types";
import {
  filterConversations,
  filterTasks,
  getTaskFilterStatuses,
  getTaskStatusLabel,
  getTaskStatusTone,
  hasActiveBackgroundWork,
  isTaskStartable,
} from "./taskPresentation";

const workspace: Workspace = {
  id: "w1",
  name: "YiLianQianYan",
  description: "",
  root_path: "F:\\项目开发\\忆涟千言\\YiLianQianYan",
  status: "active",
  created_at: 1,
  updated_at: 2,
};

const tasks: Task[] = [
  {
    id: "running",
    workspace_id: "w1",
    title: "整理前端代码",
    description: "扫描 TODO 标记",
    status: "waiting_approval",
    priority: "normal",
    created_at: 1,
    updated_at: 3,
  },
  {
    id: "done",
    workspace_id: "w1",
    title: "发布检查",
    description: "检查构建结果",
    status: "completed",
    priority: "high",
    created_at: 2,
    updated_at: 4,
  },
];

const conversations: ConversationSummary[] = [
  {
    id: "conversation-running",
    title: "后台整理文件",
    run_status: "running",
    created_at: 1,
    updated_at: 5,
  },
  {
    id: "conversation-completed",
    title: "后台检查构建",
    run_status: "completed",
    created_at: 2,
    updated_at: 6,
  },
];

describe("task presentation", () => {
  it("groups real waiting states into the running filter", () => {
    expect(getTaskFilterStatuses("running")).toContain("waiting_approval");
    expect(filterTasks(tasks, "running", "", [workspace])).toEqual([tasks[0]]);
  });

  it("searches task title, description, and workspace name without adding data", () => {
    expect(filterTasks(tasks, "all", "TODO", [workspace])).toEqual([tasks[0]]);
    expect(filterTasks(tasks, "all", "YiLianQianYan", [workspace])).toHaveLength(2);
  });

  it("exposes semantic status labels and shared badge tones", () => {
    expect(getTaskStatusLabel("completed")).toBe("已完成");
    expect(getTaskStatusLabel("waiting_approval")).toBe("等待确认");
    expect(getTaskStatusTone("failed")).toBe("danger");
  });

  it("only exposes start for tasks that can actually be started", () => {
    expect(isTaskStartable("draft")).toBe(true);
    expect(isTaskStartable("ready")).toBe(true);
    expect(isTaskStartable("running")).toBe(false);
    expect(isTaskStartable("waiting_approval")).toBe(false);
  });

  it("filters background conversation runs with the selected task status", () => {
    expect(filterConversations(conversations, "running", "")).toEqual([
      conversations[0],
    ]);
    expect(filterConversations(conversations, "completed", "")).toEqual([
      conversations[1],
    ]);
    expect(filterConversations(conversations, "failed", "")).toEqual([]);
  });

  it("polls only while real background work is active", () => {
    expect(hasActiveBackgroundWork([], conversations)).toBe(true);
    expect(hasActiveBackgroundWork([], [conversations[1]])).toBe(false);
    expect(
      hasActiveBackgroundWork([{ ...tasks[0], status: "running" }], [])
    ).toBe(true);
  });
});
