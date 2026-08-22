import type { ReactNode } from "react";
import Panel from "../ui/Panel";

export default function SystemMetricCard({
  label,
  value,
  detail,
  icon,
  barPercent,
}: {
  label: string;
  value: ReactNode;
  detail?: ReactNode;
  icon?: ReactNode;
  barPercent?: number;
}) {
  const percent = typeof barPercent === "number"
    ? Math.min(100, Math.max(0, barPercent))
    : null;

  return (
    <Panel className="system-metric-card">
      <div className="flex items-start justify-between gap-3">
        <div className="flex min-w-0 items-center gap-2 text-sm font-medium text-[var(--text-secondary)]">
          {icon ? <span className="text-[var(--accent-primary)]" aria-hidden="true">{icon}</span> : null}
          <span>{label}</span>
        </div>
        <span className="text-2xl font-semibold tracking-tight text-[var(--text-primary)]">{value}</span>
      </div>
      {percent !== null ? (
        <div className="mt-4 h-1.5 overflow-hidden rounded-full bg-[var(--surface-muted)]" aria-hidden="true">
          <div
            className="h-full rounded-full bg-[var(--accent-primary)] transition-[width] duration-[var(--motion-fast)]"
            style={{ width: `${percent}%` }}
          />
        </div>
      ) : null}
      {detail ? <div className="mt-3 text-xs text-[var(--text-secondary)]">{detail}</div> : null}
    </Panel>
  );
}
