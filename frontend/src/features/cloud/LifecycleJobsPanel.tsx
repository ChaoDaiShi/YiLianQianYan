import { Download, AlertCircle, CheckCircle2, Clock, RefreshCw } from "lucide-react";
import {
  downloadCloudExport,
  type LifecycleJob,
  type LifecycleJobStatus,
} from "../../api/cloud";
import { Badge, Button, ProgressBar } from "../../components/ui";

interface LifecycleJobsPanelProps {
  job: LifecycleJob | null;
  onRefresh?: () => void;
  isRefreshing?: boolean;
}

type BadgeTone = "default" | "success" | "warning" | "danger" | "info" | "accent";

function getStatusDetails(status: LifecycleJobStatus): {
  label: string;
  tone: BadgeTone;
  description: string;
} {
  switch (status) {
    case "queued":
      return {
        label: "排队中",
        tone: "default",
        description: "任务已进入队列，等待空闲工作进程领取。",
      };
    case "running":
      return {
        label: "处理中",
        tone: "accent",
        description: "工作进程已加锁并执行流式处理。",
      };
    case "waiting_for_producers":
      return {
        label: "等待写操作排空",
        tone: "warning",
        description: "正在等待进行中的导出或同步写入退出，确保数据清理边界完整。",
      };
    case "waiting_backup_expiry":
      return {
        label: "等待备份到期擦除",
        tone: "warning",
        description: "云端活跃数据已彻底清理，正在等待既定安全备份周期到期并执行不可逆擦除。",
      };
    case "retry_wait":
      return {
        label: "等待重试",
        tone: "danger",
        description: "单步执行遇到临时异常，系统已进入退避等待，稍后自动重试。",
      };
    case "completed":
      return {
        label: "已完成",
        tone: "success",
        description: "任务目标已全部达成并通过一致性复核。",
      };
    case "failed":
      return {
        label: "执行失败",
        tone: "danger",
        description: "重试次数达到上限或遇到不可恢复错误，需人工排查。",
      };
    default:
      return {
        label: status,
        tone: "default",
        description: "状态更新中…",
      };
  }
}

export function LifecycleJobsPanel({
  job,
  onRefresh,
  isRefreshing = false,
}: LifecycleJobsPanelProps) {
  const downloadExport = async () => {
    if (!job || job.kind !== "export") return;
    const result = await downloadCloudExport(job.id);
    if (!result.ok) return;
    const url = URL.createObjectURL(result.data);
    const anchor = document.createElement("a");
    anchor.href = url;
    anchor.download = `yilian-export-${job.id}.json`;
    document.body.appendChild(anchor);
    anchor.click();
    anchor.remove();
    URL.revokeObjectURL(url);
  };
  if (!job) {
    return null;
  }

  const statusInfo = getStatusDetails(job.status);
  const isExport = job.kind === "export";
  const isDeletion = job.kind === "deletion";

  return (
    <div className="cloud-card lifecycle-jobs-panel" data-testid="lifecycle-jobs-panel">
      <div className="cloud-card-title">
        <div>
          <h3>{isExport ? "云端数据导出任务" : "云端账号注销任务"}</h3>
          <p>{statusInfo.description}</p>
        </div>
        <div style={{ display: "flex", alignItems: "center", gap: "8px" }}>
          <Badge tone={statusInfo.tone}>{statusInfo.label}</Badge>
          {onRefresh ? (
            <Button
              size="sm"
              variant="secondary"
              disabled={isRefreshing}
              onClick={onRefresh}
              aria-label="刷新任务状态"
            >
              <RefreshCw
                className={isRefreshing ? "animate-spin" : ""}
                style={{ width: 14, height: 14 }}
              />
            </Button>
          ) : null}
        </div>
      </div>

      <div className="my-3 space-y-1.5">
        <div className="flex justify-between text-xs text-[var(--text-secondary)]">
          <span>执行进度</span>
          <span>{job.progress}%</span>
        </div>
        <ProgressBar
          value={job.progress}
          tone={job.status === "failed" ? "danger" : "default"}
          size="sm"
        />
      </div>

      {isDeletion ? (
        <dl className="cloud-data-list mt-3">
          <div>
            <dt>活跃数据清理</dt>
            <dd>
              {job.live_data_removed ? (
                <span className="inline-flex items-center gap-1 text-xs text-[var(--success-fg)]">
                  <CheckCircle2 className="h-3.5 w-3.5 text-[var(--success)]" /> 已彻底清除
                </span>
              ) : (
                <span className="inline-flex items-center gap-1 text-xs text-[var(--warning-fg)]">
                  <Clock className="h-3.5 w-3.5 text-[var(--warning)]" /> 清理处理中
                </span>
              )}
            </dd>
          </div>
          <div>
            <dt>备份擦除状态</dt>
            <dd>
              {job.backups_cleared ? (
                <span className="inline-flex items-center gap-1 text-xs text-[var(--success-fg)]">
                  <CheckCircle2 className="h-3.5 w-3.5 text-[var(--success)]" /> 备份副本已全部擦除
                </span>
              ) : job.backup_purge_deadline ? (
                <span>
                  保留至 {new Date(job.backup_purge_deadline).toLocaleString()} (到期自动擦除)
                </span>
              ) : (
                <span>等待周期确认</span>
              )}
            </dd>
          </div>
        </dl>
      ) : null}

      {isExport && job.status === "completed" ? (
        <div className="mt-4">
          <Button
            variant="primary"
            onClick={() => void downloadExport()}
            className="inline-flex items-center gap-1.5"
          >
            <Download className="h-4 w-4" /> 下载导出数据 (.json)
          </Button>
        </div>
      ) : null}

      {job.error_code ? (
        <div
          className="mt-3 flex items-center gap-2 rounded-[var(--radius-sm)] border border-[var(--danger-border)] bg-[var(--danger-soft)] p-2 text-xs text-[var(--danger-fg)]"
          role="status"
        >
          <AlertCircle className="h-4 w-4 shrink-0 text-[var(--danger)]" />
          <span>错误代号：{job.error_code}</span>
        </div>
      ) : null}
    </div>
  );
}
