import { Input } from "../../../components/ui";
import type { AppConfig } from "../../../types";

interface AgentSettingsProps {
  config: AppConfig;
  updateField: (section: keyof AppConfig, key: string, value: any) => void;
}

export default function AgentSettings({ config, updateField }: AgentSettingsProps) {
  return (
    <div className="space-y-4">
      <h3 className="font-semibold text-sm uppercase tracking-wider text-[var(--text-muted)]">智能体配置</h3>
      <Input label="名称" value={config.agent.name} onChange={(e) => updateField("agent", "name", e.target.value)} />
      <Input label="工作区根目录" value={config.agent.workspace_root} onChange={(e) => updateField("agent", "workspace_root", e.target.value)} placeholder="默认当前目录" />
      <div>
        <label className="block text-sm font-medium mb-1">系统提示词</label>
        <textarea
          value={config.agent.system_prompt}
          onChange={(e) => updateField("agent", "system_prompt", e.target.value)}
          rows={10}
          className="w-full rounded-lg border border-[var(--border)] bg-[var(--input-bg)] px-3 py-2 text-sm text-[var(--text)] placeholder:text-[var(--text-faint)] focus:outline-none focus:ring-2 focus:ring-[var(--accent)]/40 resize-none font-mono"
        />
      </div>
    </div>
  );
}
