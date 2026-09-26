import type { IsolationStatus, SecurityGrant } from "../../../api/client";
import { Badge } from "../../../components/ui";
import type { AppConfig } from "../../../types";
import { GrantEditor } from "../../security/GrantEditor";
import { isolationRows } from "../../security/grantEditorModel";

interface PermissionSettingsProps {
  config: AppConfig;
  updateField: (section: keyof AppConfig, key: string, value: any) => void;
  isolation: IsolationStatus | null;
  grants: SecurityGrant[];
  onGrantsChanged: () => Promise<void>;
}

export default function PermissionSettings({
  config,
  updateField,
  isolation,
  grants,
  onGrantsChanged,
}: PermissionSettingsProps) {
  return (
    <div className="space-y-4">
      <h3 className="font-semibold text-sm uppercase tracking-wider text-[var(--text-muted)]">权限配置</h3>
      {isolation && (
        <div className="rounded-lg border border-[var(--border)] px-3 py-3 space-y-1">
          <h4 className="text-sm font-medium">Windows 隔离能力</h4>
          {isolationRows(isolation).map(([label, active]) => <div key={label} className="flex items-center justify-between text-xs"><span className="text-[var(--text-muted)]">{label}</span><Badge tone={active ? "success" : "default"}>{active ? "Active" : "Not enabled"}</Badge></div>)}
          <p className="text-[11px] text-[var(--text-faint)] mt-2">当前安全边界由 Windows 令牌降权、Job Object 和应用层资源授权共同组成；Restricting-SID、OS 命名空间、GUI 沙箱与容器级隔离尚未启用。</p>
        </div>
      )}
      <div>
        <label className="block text-sm font-medium mb-1">模式</label>
        <select
          value={config.permissions.mode}
          onChange={(e) => updateField("permissions", "mode", e.target.value)}
          className="w-full rounded-lg border border-[var(--border)] bg-[var(--input-bg)] px-3 py-2 text-sm text-[var(--text)] focus:outline-none focus:ring-2 focus:ring-[var(--accent)]/40"
        >
          <option value="yolo">YOLO — 自动执行所有操作</option>
          <option value="ask">Ask — 每次操作前确认</option>
          <option value="plan">Plan — 仅执行已批准的计划</option>
        </select>
      </div>
      <div>
        <label className="block text-sm font-medium mb-1">需确认的工具</label>
        <input
          type="text"
          value={config.permissions.interrupt_on.join(", ")}
          onChange={(e) => updateField("permissions", "interrupt_on", e.target.value.split(",").map((s) => s.trim()).filter(Boolean))}
          placeholder="bash, write_file, edit_file"
          className="w-full rounded-lg border border-[var(--border)] bg-[var(--input-bg)] px-3 py-2 text-sm text-[var(--text)] focus:outline-none focus:ring-2 focus:ring-[var(--accent)]/40"
        />
        <p className="mt-1 text-xs text-[var(--text-muted)]">逗号分隔的工具名称列表</p>
      </div>
      <GrantEditor grants={grants} onChanged={onGrantsChanged} />
    </div>
  );
}
