import type { Task } from "../../api/client";
import { Badge, Panel, Skeleton } from "../../components/ui";
import { deriveTaskDisplayTitle } from "../../components/chat/taskTitle";
import {
  formatTaskDate,
  getTaskStatusLabel,
  getTaskStatusTone,
} from "./taskPresentation";

export function TaskRow({
  task,
  workspaceName,
  selected,
  onSelect,
}: {
  task: Task;
  workspaceName?: string;
  selected: boolean;
  onSelect: () => void;
}) {
  return (
    <button
      type="button"
      role="option"
      aria-selected={selected}
      onClick={onSelect}
      className={`group relative flex min-h-[72px] w-full items-center gap-3 rounded-[var(--radius-md)] border px-3 py-2.5 text-left transition-colors focus-ring-token focus-visible:outline-none ${
        selected
          ? "border-[var(--accent-border)] bg-[var(--accent-soft)]"
          : "border-transparent hover:border-[var(--border-soft)] hover:bg-[var(--surface-hover)]"
      }`}
    >
      {selected && (
        <span className="absolute inset-y-3 left-0 w-0.5 rounded-r-full bg-[var(--accent-primary)]" />
      )}
      <div className="min-w-0 flex-1">
        <p className="truncate text-sm font-medium text-[var(--text)]">
          {deriveTaskDisplayTitle(task.title)}
        </p>
        <div className="mt-1 flex flex-wrap items-center gap-x-2 gap-y-1 text-xs text-[var(--text-faint)]">
          <span>{formatTaskDate(task.updated_at) || "时间未知"}</span>
          {workspaceName && <span className="truncate">{workspaceName}</span>}
        </div>
      </div>
      <Badge tone={getTaskStatusTone(task.status)}>{getTaskStatusLabel(task.status)}</Badge>
    </button>
  );
}

export function TaskCenterSkeleton() {
  return (
    <div className="task-center-grid min-h-0 flex-1">
      <Panel padding={false} className="space-y-2 p-3">
        {Array.from({ length: 5 }).map((_, index) => (
          <div
            key={index}
            className="flex min-h-[72px] items-center gap-3 rounded-[var(--radius-md)] border border-[var(--border-soft)] px-3"
          >
            <div className="min-w-0 flex-1 space-y-2">
              <Skeleton className="h-4 w-2/3" />
              <Skeleton className="h-3 w-1/2" />
            </div>
            <Skeleton className="h-5 w-14" />
          </div>
        ))}
      </Panel>
      <Panel className="space-y-3">
        <Skeleton className="h-5 w-1/2" />
        <Skeleton className="h-4 w-3/4" />
        <Skeleton className="h-4 w-2/3" />
      </Panel>
    </div>
  );
}
