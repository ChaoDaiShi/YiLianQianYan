import { useEffect, useState } from "react";
import { Link } from "react-router-dom";
import type { IsolationStatus, SecurityGrant } from "../../../api/client";
import { getSecuritySubject, type SecuritySubjectLevel } from "../../../api/securitySubject";
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
  const [subject, setSubject] = useState<SecuritySubjectLevel | null>(null);
  const [subjectError, setSubjectError] = useState("");

  useEffect(() => {
    let active = true;
    void getSecuritySubject().then(result => {
      if (!active) return;
      if (result.ok) setSubject(result.data);
      else setSubjectError(result.error);
    });
    return () => { active = false; };
  }, []);

  return (
    <div className="space-y-3">
      <Link to="/setup?permissions=1" className="underline">管理个人资料使用与设备权限</Link>
      <section aria-label="当前主体级别" className="space-y-1">
        <div className="settings-status-row">
          <h3 className="text-sm font-semibold text-[var(--text)]">当前主体级别</h3>
          {subject ? <Badge tone="info">{subject.label}</Badge>
            : !subjectError && <span role="status">正在读取…</span>}
        </div>
        {subjectError && <p role="alert" className="settings-save-error">主体级别读取失败。{subjectError}</p>}
        <p className="settings-help">级别定义平台权限范围。实际操作还受审批、资源授权和操作系统权限约束。</p>
      </section>
      <fieldset className="min-w-0" role="radiogroup" aria-label="审批模式">
        <legend className="mb-2 text-sm font-semibold">审批模式</legend>
        <div className="divide-y divide-[var(--border-soft)] overflow-hidden rounded-xl border border-[var(--border-soft)]">
          {approvalModes.map((mode) => {
            const selected = config.permissions.mode === mode.value;
            return (
              <label key={mode.value} className={"flex cursor-pointer items-start gap-3 px-3 py-2.5 " + (selected ? "bg-[var(--accent-soft)]" : "hover:bg-[var(--surface-hover)]")}>
                <input className="mt-1 shrink-0 accent-[var(--accent-soft-fg)]" type="radio" name="approval-mode" value={mode.value} checked={selected} onChange={() => updateField("permissions", "mode", mode.value)} />
                <span>
                  <span className="block text-sm font-semibold">{mode.label}</span>
                  <span className="settings-help block">{mode.description}</span>
                </span>
              </label>
            );
          })}
        </div>
      </fieldset>

      <details className="u2-disclosure">
        <summary>高级范围约束</summary>
        <div className="pt-2"><AccessMatrix /></div>
      </details>

      <details className="u2-disclosure">
        <summary>已有资源授权</summary>
        <div className="space-y-2 pt-2">
          <p className="settings-help">显式禁止会阻止匹配的操作。</p>
          <GrantEditor grants={grants} onChanged={onGrantsChanged} />
        </div>
      </details>

      <details className="u2-disclosure">
        <summary>Windows 隔离状态</summary>
        <div className="space-y-2 pt-2">
          {isolation ? <>
            <div className="settings-status-list">
              {isolationRows(isolation).map(([label, active]) => <div key={label} className="settings-status-row"><span>{label}</span><Badge tone={active ? "success" : "default"}>{active ? "已启用" : "未启用"}</Badge></div>)}
            </div>
            <p className="settings-help">当前安全边界由 Windows 令牌降权、Job Object 和应用层资源授权共同组成。</p>
          </> : <p className="settings-help" role="status">正在读取本机隔离状态…</p>}
        </div>
      </details>
    </div>
  );
}
