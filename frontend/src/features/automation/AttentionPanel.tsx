import { useEffect, useState } from "react";
import { getAutomationObservations, type Automation, type AttentionState, type SourceObservation } from "../../api/automations";
import { Badge, Button, EmptyState } from "../../components/ui";
import { occurrenceTime } from "./automationDisplay";

const attentionLabels: Record<AttentionState["status"], string> = {
  waiting: "等待复查", checking: "正在采集与判断", deferred: "暂缓推进",
  unchanged: "暂无新证据", action_queued: "已安排委托", proposed: "有目标提议",
  conflict: "观察有冲突", blocked: "关注来源受阻", unknown: "当前状态未知", stopped: "关注已停止",
};

export function AttentionSummary({ automation, onOpenLog, onSelectGoal }: {
  automation: Automation;
  onOpenLog: () => void;
  onSelectGoal: (proposal: NonNullable<AttentionState["proposal"]>) => void;
}) {
  const { attention, schedule } = automation;
  if (!attention || schedule.kind !== "watch") return null;
  const conflict = ["conflict", "blocked", "unknown"].includes(attention.status);
  return <section className="space-y-2 rounded-lg border border-[var(--border-soft)] p-3" aria-label="主动关注">
    <div className="flex flex-wrap items-center gap-2">
      <h4 className="font-medium">主动关注</h4>
      <Badge tone={conflict ? "warning" : "default"}>{attentionLabels[attention.status]}</Badge>
    </div>
    <p>{attention.reason}</p>
    <p className="text-xs text-[var(--text-secondary)]">
      {automation.next_run_at ? `下次复查 ${occurrenceTime(automation.next_run_at, automation.timezone)} · ` : ""}
      已检查 {attention.checks_used}/{schedule.max_checks} 次，已用重试 {attention.retries_used}/{schedule.max_retries} 次
    </p>
    {attention.last_observed_at !== null && <p className="text-xs text-[var(--text-secondary)]">
      最近确认来源状态 {occurrenceTime(attention.last_observed_at, automation.timezone)}
    </p>}
    {attention.proposal && <div className="space-y-2 border-t border-[var(--border-soft)] pt-2">
      <h5 className="font-medium">最近的目标提议</h5>
      <p className="whitespace-pre-wrap">{attention.proposal.goal}</p>
      <Button size="sm" onClick={() => onSelectGoal(attention.proposal!)}>选入目标规划</Button>
    </div>}
    <Button size="sm" variant="secondary" onClick={onOpenLog}>查看观察日志</Button>
  </section>;
}

const observationLabels: Record<SourceObservation["status"], string> = {
  captured: "已采集", conflict: "来源时间冲突", unknown: "采集未确认",
};

export function AttentionLog({ automationId, timezone, lastCheck }: {
  automationId: string; timezone: string; lastCheck: number | null | undefined;
}) {
  const [records, setRecords] = useState<SourceObservation[] | null>(null);
  const [error, setError] = useState("");
  const [refresh, setRefresh] = useState(0);
  useEffect(() => {
    let mounted = true;
    void getAutomationObservations(automationId).then(value => {
      if (!mounted) return;
      setRecords(value.observations);
      setError("");
    }).catch(cause => {
      if (mounted) setError(cause instanceof Error ? cause.message : String(cause));
    });
    return () => { mounted = false; };
  }, [automationId, lastCheck, refresh]);

  return <section role="tabpanel" aria-label="观察日志" className="min-w-0 space-y-3">
    <div className="flex flex-wrap items-center justify-between gap-2">
      <p className="text-xs text-[var(--text-secondary)]">最近最多 100 条来源记录，采集和来源时间分别显示。</p>
      <Button size="sm" variant="secondary" onClick={() => setRefresh(value => value + 1)}>刷新日志</Button>
    </div>
    {error && <p role="alert" className="text-[var(--danger)]">无法读取观察日志：{error}</p>}
    {records === null ? !error && <p role="status">正在读取观察日志…</p>
      : !records.length ? !error && <EmptyState title="尚无观察记录" />
      : records.map(record => <article key={record.id} className="space-y-1 rounded-lg border border-[var(--border-soft)] p-3 [overflow-wrap:anywhere]">
        <div className="flex flex-wrap items-center gap-2">
          <Badge tone={record.status === "captured" ? "default" : "warning"}>{observationLabels[record.status]}</Badge>
          <span>{record.source_kind === "file" ? "本地文件" : "网络来源"}</span>
          {record.http_status !== undefined && <span>HTTP {record.http_status}</span>}
        </div>
        <p>{record.source}</p>
        <p className="text-xs text-[var(--text-secondary)]">
          采集于 {occurrenceTime(record.observed_at, timezone)} · 来源时间 {record.source_at === null ? "未知" : occurrenceTime(record.source_at, timezone)}
        </p>
        <p className="text-xs text-[var(--text-secondary)]">观察 {record.id}{record.previous_observation_id && ` · 关联 ${record.previous_observation_id}`}</p>
      </article>)}
  </section>;
}
