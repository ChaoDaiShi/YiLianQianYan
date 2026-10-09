import { Input } from "../../../components/ui";
import type { AppConfig } from "../../../types";

interface AgentSettingsProps {
  config: AppConfig;
  updateField: (section: keyof AppConfig, key: string, value: any) => void;
}

export default function AgentSettings({ config, updateField }: AgentSettingsProps) {
  return (
    <section className="settings-card">
      <header className="settings-card-header">
        <div><h3>身份与工作区</h3><p>定义智能体名称、默认目录和系统行为说明。</p></div>
      </header>
      <div className="settings-card-body">
      <Input label="名称" value={config.agent.name} onChange={(e) => updateField("agent", "name", e.target.value)} />
      <Input label="工作区根目录" value={config.agent.workspace_root} onChange={(e) => updateField("agent", "workspace_root", e.target.value)} placeholder="默认当前目录" />
      <div>
        <label className="settings-control-label">系统提示词</label>
        <textarea
          value={config.agent.system_prompt}
          onChange={(e) => updateField("agent", "system_prompt", e.target.value)}
          rows={10}
          className="settings-textarea min-h-56 font-mono"
        />
      </div>
      </div>
    </section>
  );
}
