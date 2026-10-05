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
    <>
      <section className="settings-card">
      <header className="settings-card-header"><div><h3>操作确认</h3><p>决定哪些操作直接运行，哪些操作需要用户确认。</p></div></header>
      <div className="settings-card-body">
      <div>
        <label className="settings-control-label">执行模式</label>
        <select
          value={config.permissions.mode}
          onChange={(e) => updateField("permissions", "mode", e.target.value)}
          className="settings-select"
        >
          <option value="yolo">YOLO — 自动执行所有操作</option>
          <option value="ask">Ask — 每次操作前确认</option>
          <option value="plan">Plan — 仅执行已批准的计划</option>
        </select>
      </div>
      <div>
        <label className="settings-control-label">需确认的工具</label>
        <input
          type="text"
          value={config.permissions.interrupt_on.join(", ")}
          onChange={(e) => updateField("permissions", "interrupt_on", e.target.value.split(",").map((s) => s.trim()).filter(Boolean))}
          placeholder="bash, write_file, edit_file"
          className="settings-text-input"
        />
        <p className="settings-help">使用英文逗号分隔工具名称。高风险工具仍受安全网关约束。</p>
      </div>
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
        <header className="settings-card-header"><div><h3>资源授权</h3><p>查看持久化授权；新增授权是显式操作，不会由一次审批自动创建。</p></div></header>
        <div className="settings-card-body">
      <GrantEditor grants={grants} onChanged={onGrantsChanged} />
        </div>
      </section>
    </>
  );
}
