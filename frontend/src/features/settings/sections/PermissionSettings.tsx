import type { IsolationStatus, SecurityGrant } from "../../../api/client";
import { Badge } from "../../../components/ui";
import type { AppConfig } from "../../../types";
import { GrantEditor } from "../../security/GrantEditor";
import { AccessMatrix } from "../../security/AccessMatrix";
import { approvalModes } from "../../../components/chat/chatRequestOptions";
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
    <>
      <section className="settings-card settings-card--wide">
        <header className="settings-card-header"><div><h3>操作权限</h3><p>选择小涟执行操作时需要向你请求批准的时机。</p></div></header>
        <div className="settings-card-body">
          <div className="grid gap-3 sm:grid-cols-3" role="radiogroup" aria-label="审批模式">
            {approvalModes.map(mode => <label key={mode.value} className={`cursor-pointer rounded-xl border p-4 ${config.permissions.mode === mode.value ? "border-[var(--accent)] bg-[var(--accent-soft)]" : "border-[var(--border-soft)]"}`}>
              <span className="flex items-center gap-2 font-semibold"><input type="radio" name="approval-mode" value={mode.value} checked={config.permissions.mode === mode.value} onChange={() => updateField("permissions", "mode", mode.value)} />{mode.label}</span>
              <p className="settings-help mt-2">{mode.description}</p>
            </label>)}
          </div>
          <details className="mt-4"><summary className="cursor-pointer text-sm text-[var(--text-muted)]">高级范围约束</summary><div className="mt-3"><AccessMatrix /></div></details>
        </div>
      </section>

      <section className="settings-card">
        <header className="settings-card-header"><div><h3>Windows 隔离状态</h3><p>展示当前运行环境实际启用的隔离能力。</p></div></header>
        <div className="settings-card-body">
          {isolation ? <>
            <div className="settings-status-list">
              {isolationRows(isolation).map(([label, active]) => <div key={label} className="settings-status-row"><span>{label}</span><Badge tone={active ? "success" : "default"}>{active ? "已启用" : "未启用"}</Badge></div>)}
            </div>
            <p className="settings-help">当前安全边界由 Windows 令牌降权、Job Object 和应用层资源授权共同组成；未启用的能力不会显示为已保护。</p>
          </> : <p className="settings-help" role="status">正在读取本机隔离状态…</p>}
        </div>
      </section>

      <section className="settings-card settings-card--wide">
        <header className="settings-card-header"><div><h3>已有资源授权</h3><p>查看原有资源授权；显式禁止仍会阻止匹配操作。新权限范围在上方设置。</p></div></header>
        <div className="settings-card-body">
      <GrantEditor grants={grants} onChanged={onGrantsChanged} />
        </div>
      </section>
    </>
  );
}
