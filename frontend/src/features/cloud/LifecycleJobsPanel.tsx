import { useState } from "react";
import { Download, RefreshCw } from "lucide-react";
import { downloadCloudExport, type LifecycleJob } from "../../api/cloud";
import { Badge, Button, ProgressBar } from "../../components/ui";
import { lifecyclePresentation } from "./cloudPresentation";

interface LifecycleJobsPanelProps {
  job: LifecycleJob;
  onRefresh: () => void;
  isRefreshing: boolean;
}

const DOWNLOAD_ERRORS: Record<number, string> = {
  401: "云会话已失效，请重新登录。",
  404: "找不到导出任务或文件。",
  410: "导出文件已过期，请重新申请导出。",
};

export function LifecycleJobsPanel({ job, onRefresh, isRefreshing }: LifecycleJobsPanelProps) {
  const [downloadError, setDownloadError] = useState("");
  const { title, label, tone, description, canDownload } = lifecyclePresentation(job);

  const downloadExport = async () => {
    setDownloadError("");
    const result = await downloadCloudExport(job.id);
    if (!result.ok) {
      setDownloadError(DOWNLOAD_ERRORS[result.status] ?? "暂时无法下载导出文件，请稍后重试。");
      return;
    }
    const url = URL.createObjectURL(result.data);
    const anchor = document.createElement("a");
    anchor.href = url;
    anchor.download = `yilian-export-${job.id}.json`;
    document.body.appendChild(anchor);
    anchor.click();
    anchor.remove();
    URL.revokeObjectURL(url);
  };

  return (
    <section className="cloud-card lifecycle-jobs-panel" data-testid="lifecycle-jobs-panel" aria-labelledby="lifecycle-job-title">
      <div className="cloud-card-title">
        <div>
          <h3 id="lifecycle-job-title">{title}</h3>
          {description ? <p>{description}</p> : null}
        </div>
        <div className="cloud-actions">
          <Badge tone={tone}>{label}</Badge>
          <Button size="sm" variant="secondary" disabled={isRefreshing} onClick={onRefresh} aria-label="刷新任务状态">
            <RefreshCw className={isRefreshing ? "animate-spin" : ""} />
          </Button>
        </div>
      </div>

      <div className="my-3 space-y-1.5">
        <div className="flex justify-between text-xs text-[var(--text-secondary)]">
          <span>执行进度</span><span>{job.progress}%</span>
        </div>
        <ProgressBar value={job.progress} tone={job.status === "failed" ? "danger" : "default"} size="sm" />
      </div>

      {job.kind === "deletion" ? (
        <dl className="cloud-storage-list">
          <div><dt>云端活跃数据</dt><dd>{job.live_data_removed ? "已清除" : "待清理"}</dd></div>
          <div>
            <dt>备份副本</dt>
            <dd>
              {job.backups_cleared ? "已擦除" : job.backup_purge_deadline
                ? `保留至 ${new Date(job.backup_purge_deadline).toLocaleString()}，到期擦除`
                : "等待确认保留期限"}
            </dd>
          </div>
        </dl>
      ) : null}

      {canDownload ? (
        <Button className="mt-3" onClick={() => void downloadExport()}>
          <Download />下载导出数据 (.json)
        </Button>
      ) : null}
      {downloadError ? <p className="cloud-inline-error" role="alert">{downloadError}</p> : null}

      {job.error_code ? (
        <details className="cloud-disclosure">
          <summary>任务诊断</summary>
          <dl className="cloud-data-list"><div><dt>错误代码</dt><dd>{job.error_code}</dd></div></dl>
        </details>
      ) : null}
    </section>
  );
}
