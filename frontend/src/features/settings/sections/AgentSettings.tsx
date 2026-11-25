import { Link } from "react-router-dom";
import type { AppConfig } from "../../../types";

interface AgentSettingsProps {
  config: AppConfig & { ai_repair_enabled?: boolean };
  onAiRepairChange: (enabled: boolean) => void;
  linksDisabled: boolean;
}

const ownerLinks = [
  { path: "/agents?agent=default", label: "编辑伙伴名称与角色" },
  { path: "/workspaces", label: "管理工作空间目录" },
];

export default function AgentSettings({ config, onAiRepairChange, linksDisabled }: AgentSettingsProps) {
  return (
    <div className="space-y-4">
      <div className="flex flex-wrap items-center gap-x-4 gap-y-2 text-sm">
        {ownerLinks.map(({ path, label }) => <Link key={path} className="focus-ring-token underline aria-disabled:opacity-50" to={path}
          aria-disabled={linksDisabled || undefined} onClick={event => {
            if (linksDisabled) event.preventDefault();
          }}>{label}</Link>)}
      </div>
      {linksDisabled && <p className="settings-help">保存或放弃当前修改后可打开其他管理页面。</p>}
      <details className="u2-disclosure">
        <summary>高级选项</summary>
        <label className="flex cursor-pointer items-start gap-3 py-2">
          <input className="mt-1 shrink-0 accent-[var(--accent-soft-fg)]" type="checkbox" aria-label="AI 辅助修复"
            checked={config.ai_repair_enabled === true} onChange={(event) => onAiRepairChange(event.target.checked)} />
          <span>
            <span className="block text-sm font-medium">AI 辅助修复</span>
            <span className="settings-help block">用当前模型分析失败原因，用量计入任务预算。应用修复前需要确认。</span>
          </span>
        </label>
      </details>
    </div>
  );
}
