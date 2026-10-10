import { Download, AlertCircle, CheckCircle2, Clock, RefreshCw } from "lucide-react";
import { type LifecycleJob, type LifecycleJobStatus } from "../../api/cloud";
import { Badge, Button } from "../../components/ui";

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

      <div style={{ marginTop: "12px", marginBottom: "12px" }}>
        <div
          style={{
            display: "flex",
            justifyContent: "space-between",
            fontSize: "13px",
            color: "var(--text-secondary, #666)",
            marginBottom: "6px",
          }}
        >
          <span>执行进度</span>
          <span>{job.progress}%</span>
        </div>
        <div
          style={{
            height: "8px",
            background: "var(--bg-muted, #f0f0f0)",
            borderRadius: "4px",
            overflow: "hidden",
          }}
        >
          <div
            style={{
              height: "100%",
              width: `${Math.min(100, Math.max(0, job.progress))}%`,
              background: job.status === "failed" ? "var(--color-danger, #ef4444)" : "var(--color-primary, #3b82f6)",
              transition: "width 0.3s ease",
            }}
          />
        </div>
      </div>

      {isDeletion ? (
        <dl className="cloud-data-list" style={{ marginTop: "12px" }}>
          <div>
            <dt>活跃数据清理</dt>
            <dd>
              {job.live_data_removed ? (
                <span style={{ color: "#16a34a", display: "inline-flex", alignItems: "center", gap: 4 }}>
                  <CheckCircle2 style={{ width: 14, height: 14 }} /> 已彻底清除
                </span>
              ) : (
                <span style={{ color: "#ca8a04", display: "inline-flex", alignItems: "center", gap: 4 }}>
                  <Clock style={{ width: 14, height: 14 }} /> 清理处理中
                </span>
              )}
            </dd>
          </div>
          <div>
            <dt>备份擦除状态</dt>
            <dd>
              {job.backups_cleared ? (
                <span style={{ color: "#16a34a", display: "inline-flex", alignItems: "center", gap: 4 }}>
                  <CheckCircle2 style={{ width: 14, height: 14 }} /> 备份副本已全部擦除
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
        <div style={{ marginTop: "16px" }}>
          <a
            href={`/api/cloud/export/${encodeURIComponent(job.id)}/download`}
            download={`yilian-export-${job.id}.json`}
            style={{ textDecoration: "none" }}
          >
            <Button variant="primary" style={{ display: "inline-flex", alignItems: "center", gap: 6 }}>
              <Download style={{ width: 16, height: 16 }} /> 下载导出数据 (.json)
            </Button>
          </a>
        </div>
      ) : null}

      {job.error_code ? (
        <div
          style={{
            marginTop: "12px",
            padding: "8px 12px",
            background: "rgba(239, 68, 68, 0.1)",
            border: "1px solid rgba(239, 68, 68, 0.2)",
            borderRadius: "6px",
            display: "flex",
            alignItems: "center",
            gap: "8px",
            color: "#dc2626",
            fontSize: "13px",
          }}
          role="status"
        >
          <AlertCircle style={{ width: 16, height: 16, flexShrink: 0 }} />
          <span>错误代号：{job.error_code}</span>
        </div>
      ) : null}
    </div>
  );
}
