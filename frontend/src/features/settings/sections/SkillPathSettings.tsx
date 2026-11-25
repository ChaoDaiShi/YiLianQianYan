import { Textarea } from "../../../components/ui";
import type { AppConfig } from "../../../types";
import SkillDiscoverySources from "./SkillDiscoverySources";

interface SkillPathSettingsProps {
  config: AppConfig;
  updateField: (section: keyof AppConfig, key: string, value: any) => void;
  skillRefreshPending: boolean;
  subagentRestartPending: boolean;
  onSkillsRefreshed: () => void;
}

/** Where skills and subagent definitions are discovered from. */
export default function SkillPathSettings({
  config, updateField, skillRefreshPending, subagentRestartPending, onSkillsRefreshed,
}: SkillPathSettingsProps) {
  return (
    <div className="space-y-3">
      <Textarea label="技能目录" rows={2} placeholder="./skills" value={config.skills.directories.join("\n")}
        onChange={(e) => updateField("skills", "directories", e.target.value.split("\n"))} />
      <Textarea label="子智能体目录" rows={2} placeholder="./.agents/agents" value={config.subagents.directories.join("\n")}
        onChange={(e) => updateField("subagents", "directories", e.target.value.split("\n"))} />
      <p className="settings-help">每行一个目录。
        {!skillRefreshPending && "技能目录保存后需刷新发现。"}
        {!subagentRestartPending && "子智能体目录保存后需重启应用。"}
      </p>
      {(skillRefreshPending || subagentRestartPending) && <div role="status" className="space-y-1 text-sm text-[var(--text-secondary)]">
        {skillRefreshPending && <p>技能目录已保存。点击“刷新发现”应用。</p>}
        {subagentRestartPending && <p>子智能体目录已保存。重启应用后生效。</p>}
      </div>}
      <SkillDiscoverySources onRefreshed={onSkillsRefreshed} />
    </div>
  );
}
