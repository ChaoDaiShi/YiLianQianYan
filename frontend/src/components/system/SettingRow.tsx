import type { ReactNode } from "react";

export default function SettingRow({
  label,
  description,
  children,
  className = "",
}: {
  label: string;
  description?: string;
  children: ReactNode;
  className?: string;
}) {
  return (
    <div className={`system-setting-row ${className}`}>
      <div className="min-w-0">
        <div className="text-sm font-medium text-[var(--text-primary)]">{label}</div>
        {description ? <p className="mt-1 text-xs leading-5 text-[var(--text-secondary)]">{description}</p> : null}
      </div>
      <div className="system-setting-control">{children}</div>
    </div>
  );
}
