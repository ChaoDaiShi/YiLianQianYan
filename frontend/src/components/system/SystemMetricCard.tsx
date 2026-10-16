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
  const percent = typeof barPercent === "number" && !Number.isNaN(barPercent)
    ? Math.min(100, Math.max(0, barPercent))
    : null;

  // 3-tier load thresholds: 0 <= p < 70 (normal), 70 <= p <= 88 (attention), 88 < p <= 100 (high load)
  const gaugeColor = percent === null
    ? "var(--text-muted)"
    : percent > 88
      ? "var(--danger)"
      : percent >= 70
        ? "var(--warning)"
        : "var(--accent-primary)";

  const statusText = percent === null
    ? null
    : percent > 88
      ? "高负载"
      : percent >= 70
        ? "需关注"
        : "正常";

  return (
    <Panel className="system-metric-card ui-opt-metric-card">
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <div className="flex items-center gap-2 text-sm font-medium text-[var(--text-secondary)]">
            {icon ? <span className="text-[var(--accent-primary)]" aria-hidden="true">{icon}</span> : null}
            <span>{label}</span>
            {statusText ? (
              <span
                className="ml-1 rounded px-1.5 py-0.5 text-[11px] font-medium"
                style={{
                  color: gaugeColor,
                  backgroundColor: `color-mix(in srgb, ${gaugeColor} 12%, transparent)`,
                }}
              >
                {statusText}
              </span>
            ) : null}
          </div>
          <div className="mt-2 text-2xl font-semibold tracking-tight text-[var(--text-primary)]">
            {value}
          </div>
        </div>

        {percent !== null ? (
          <div className="flex h-12 w-12 shrink-0 items-center justify-center" aria-hidden="true">
            <svg
              className="h-12 w-12 -rotate-90 transform"
              viewBox="0 0 36 36"
              aria-hidden="true"
            >
              <circle
                cx="18"
                cy="18"
                r="15"
                fill="none"
                stroke="var(--surface-muted)"
                strokeWidth="3"
              />
              <circle
                cx="18"
                cy="18"
                r="15"
                fill="none"
                stroke={gaugeColor}
                strokeWidth="3"
                pathLength="100"
                strokeDasharray={`${percent} ${100 - percent}`}
                strokeLinecap="round"
                className="transition-[stroke-dasharray] duration-500 ease-out"
              />
            </svg>
          </div>
        ) : null}
      </div>

      {percent !== null ? (
        <div className="mt-3 h-1.5 overflow-hidden rounded-full bg-[var(--surface-muted)]" aria-hidden="true">
          <div
            className="h-full rounded-full transition-[width] duration-300"
            style={{ width: `${percent}%`, backgroundColor: gaugeColor }}
          />
        </div>
      ) : null}
      {detail ? <div className="mt-3 text-xs text-[var(--text-secondary)]">{detail}</div> : null}
    </Panel>
  );
}
