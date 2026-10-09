import type { AppConfig } from "../../../types";

interface SkillPathSettingsProps {
  config: AppConfig;
  updateField: (section: keyof AppConfig, key: string, value: any) => void;
}

/** Where skills and subagent definitions are discovered from. */
export default function SkillPathSettings({ config, updateField }: SkillPathSettingsProps) {
  return (
    <>
    <section className="settings-card">
      <header className="settings-card-header"><div><h3>技能目录</h3><p>扫描可复用过程与工具说明，按需载入上下文。</p></div></header>
      <div className="settings-card-body">
      <div>
        <label className="settings-control-label">目录（一行一个）</label>
        <textarea
          value={config.skills.directories.join("\n")}
          onChange={(e) => updateField("skills", "directories", e.target.value.split("\n").filter(Boolean))}
          rows={4}
          placeholder="./skills"
          className="settings-textarea"
        />
      </div>
      <label className="settings-toggle-row">
        <span><strong>渐进加载</strong><small>只在任务需要时读取完整技能说明。</small></span>
        <input
          type="checkbox"
          aria-label="启用渐进加载"
          checked={config.skills.progressive_loading}
          onChange={(e) => updateField("skills", "progressive_loading", e.target.checked)}
          className="h-5 w-5 rounded accent-[var(--accent)]"
        />
      </label>
      </div>
    </section>
    <section className="settings-card">
      <header className="settings-card-header"><div><h3>子智能体目录</h3><p>读取本地 AGENT.md 定义，不在此处创建第二套智能体配置。</p></div></header>
      <div className="settings-card-body">
        <label className="settings-control-label" htmlFor="subagent-directories">目录（每行一个）</label>
        <textarea
          id="subagent-directories"
          aria-label="子智能体目录"
          value={config.subagents.directories.join("\n")}
          onChange={(e) => updateField("subagents", "directories", e.target.value.split("\n").filter(Boolean))}
          rows={4}
          placeholder="./.agents/agents"
          className="settings-textarea"
        />
        <p className="settings-help">扫描目录中的 AGENT.md 文件定义子智能体。</p>
      </div>
    </section>
    </>
  );
}
