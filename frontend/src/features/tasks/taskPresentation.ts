import type {
  Task,
  TaskStatus,
  Workspace,
} from "../../api/client";
import { deriveTaskDisplayTitle } from "../../components/chat/taskTitle";

export type TaskFilter = "all" | "running" | "completed" | "failed" | "cancelled";
export type TaskBadgeTone = "default" | "success" | "warning" | "danger" | "info";

const RUNNING_STATUSES: TaskStatus[] = [
  "ready",
  "running",
  "waiting_approval",
  "waiting_user",
  "blocked",
];

const TASK_STATUS_LABELS: Record<TaskStatus, string> = {
  draft: "草稿",
  ready: "待执行",
  running: "运行中",
  waiting_approval: "等待确认",
  waiting_user: "等待输入",
  blocked: "已阻塞",
  completed: "已完成",
  failed: "失败",
  cancelled: "已取消",
};

export function getTaskFilterStatuses(filter: TaskFilter): TaskStatus[] | null {
  if (filter === "all") return null;
  if (filter === "running") return RUNNING_STATUSES;
  return [filter];
}

export function getTaskStatusLabel(status: TaskStatus): string {
  return TASK_STATUS_LABELS[status];
}

export function getTaskStatusTone(status: TaskStatus): TaskBadgeTone {
  if (status === "completed") return "success";
  if (status === "failed") return "danger";
  if (status === "waiting_approval" || status === "waiting_user" || status === "blocked") {
    return "warning";
  }
  if (status === "ready" || status === "running") return "info";
  return "default";
}

export function isTaskStartable(status: TaskStatus): boolean {
  return status === "draft" || status === "ready";
}

export function filterTasks(
  tasks: Task[],
  filter: TaskFilter,
  query: string,
  workspaces: Workspace[]
): Task[] {
  const statuses = getTaskFilterStatuses(filter);
  const keyword = query.trim().toLocaleLowerCase();
  const workspaceNames = new Map(workspaces.map((workspace) => [workspace.id, workspace.name]));

  return tasks.filter((task) => {
    if (statuses && !statuses.includes(task.status)) return false;
    if (!keyword) return true;

    const searchable = [
      task.title,
      deriveTaskDisplayTitle(task.title),
      task.description,
      getTaskStatusLabel(task.status),
      workspaceNames.get(task.workspace_id) || "",
    ]
      .join(" ")
      .toLocaleLowerCase();
    return searchable.includes(keyword);
  });
}

export function formatTaskDate(value: number | null | undefined): string | null {
  if (!value) return null;
  return new Date(value).toLocaleString();
}
