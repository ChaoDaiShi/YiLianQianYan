import { BarChart3 } from "lucide-react";
import type { LlmUsageReport } from "../../types";

function compact(value: number) {
  return new Intl.NumberFormat("zh-CN", { notation: "compact", maximumFractionDigits: 1 }).format(value);
}

export default function TokenUsageChart({ report }: { report: LlmUsageReport | null }) {
  const max = Math.max(...(report?.days ?? []).map((day) => day.total_tokens), 1);
  return (
    <div className="rounded-[var(--radius-lg)] border border-[var(--border-soft)] bg-[var(--surface-solid)] p-4">
      <div className="mb-3 flex items-center justify-between">
        <div className="inline-flex items-center gap-2 text-sm font-medium"><BarChart3 className="h-4 w-4 text-[var(--accent-primary)]" />Token 消耗</div>
        <span className="text-xs text-[var(--text-muted)]">总计 {compact(report?.total_tokens ?? 0)}</span>
      </div>
      <div className="mb-4 grid grid-cols-3 gap-2 text-xs">
        <div className="rounded-md bg-[var(--surface-hover)] p-2"><span className="text-[var(--text-muted)]">输入</span><strong className="mt-1 block text-sm">{compact(report?.prompt_tokens ?? 0)}</strong></div>
        <div className="rounded-md bg-[var(--surface-hover)] p-2"><span className="text-[var(--text-muted)]">输出</span><strong className="mt-1 block text-sm">{compact(report?.completion_tokens ?? 0)}</strong></div>
        <div className="rounded-md bg-[var(--surface-hover)] p-2"><span className="text-[var(--text-muted)]">合计</span><strong className="mt-1 block text-sm">{compact(report?.total_tokens ?? 0)}</strong></div>
      </div>
      {report?.days.length ? (
        <div className="flex h-36 items-end gap-1 overflow-x-auto border-b border-[var(--border-soft)] pb-1" aria-label="每日 Token 柱状图">
          {report.days.map((day) => (
            <div key={day.date} className="group flex min-w-5 flex-1 flex-col items-center justify-end gap-1">
              <span className="pointer-events-none text-[9px] text-[var(--text-muted)] opacity-0 transition-opacity group-hover:opacity-100">{compact(day.total_tokens)}</span>
              <div className="w-full min-w-2 rounded-t bg-[var(--accent-primary)]/75 transition-all group-hover:bg-[var(--accent-primary)]" style={{ height: `${Math.max(6, (day.total_tokens / max) * 100)}%` }} title={`${day.date}: ${day.total_tokens} tokens`} />
              <span className="text-[9px] text-[var(--text-faint)]">{day.date.slice(5)}</span>
            </div>
          ))}
        </div>
      ) : (
        <div className="flex h-36 items-center justify-center text-xs text-[var(--text-muted)]">所选时间段暂无真实 usage 数据</div>
      )}
    </div>
  );
}
