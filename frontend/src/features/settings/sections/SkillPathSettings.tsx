import type { AppConfig } from "../../../types";

interface SkillPathSettingsProps {
  config: AppConfig;
  updateField: (section: keyof AppConfig, key: string, value: any) => void;
}

/** Where skills and subagent definitions are discovered from. */
export default function SkillPathSettings({ config, updateField }: SkillPathSettingsProps) {
  return (
    <div className="space-y-4">
      <h3 className="font-semibold text-sm uppercase tracking-wider text-[var(--text-muted)]">技能路径</h3>
      <div>
        <label className="block text-sm font-medium mb-1">技能目录 (一行一个)</label>
        <textarea
          value={config.skills.directories.join("\n")}
          onChange={(e) => updateField("skills", "directories", e.target.value.split("\n").filter(Boolean))}
          rows={4}
          placeholder="./skills"
          className="w-full rounded-lg border border-[var(--border)] bg-[var(--input-bg)] px-3 py-2 text-sm text-[var(--text)] focus:outline-none focus:ring-2 focus:ring-[var(--accent)]/40 resize-none"
        />
      </div>
      <div className="flex items-center gap-3">
        <label className="text-sm">渐进加载</label>
        <input
          type="checkbox"
          aria-label="启用渐进加载"
          checked={config.skills.progressive_loading}
          onChange={(e) => updateField("skills", "progressive_loading", e.target.checked)}
          className="w-4 h-4 rounded accent-[var(--accent)]"
        />
      </div>
      <div className="border-t border-[var(--border-soft)] pt-4">
        <h3 className="font-semibold text-sm uppercase tracking-wider text-[var(--text-muted)]">子智能体目录</h3>
        <label className="mt-3 block text-sm font-medium mb-1" htmlFor="subagent-directories">目录（每行一个）</label>
        <textarea
          id="subagent-directories"
          aria-label="子智能体目录"
          value={config.subagents.directories.join("\n")}
          onChange={(e) => updateField("subagents", "directories", e.target.value.split("\n").filter(Boolean))}
          rows={4}
          placeholder="./.agents/agents"
          className="w-full rounded-lg border border-[var(--border)] bg-[var(--input-bg)] px-3 py-2 text-sm text-[var(--text)] focus:outline-none focus:ring-2 focus:ring-[var(--accent)]/40 resize-none"
        />
        <p className="mt-1 text-xs text-[var(--text-muted)]">扫描目录中的 AGENT.md 文件定义子智能体</p>
      </div>
    </div>
  );
}
