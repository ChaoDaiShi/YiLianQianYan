import { useState } from "react";
import PersonaDialog from "../../agents/PersonaDialog";
import { Button } from "../../../components/ui";
import { Input } from "../../../components/ui";
import type { AppConfig } from "../../../types";

interface AgentSettingsProps {
  config: AppConfig & { ai_repair_enabled?: boolean };
  onAiRepairChange?: (enabled: boolean) => void;
  updateField: (section: keyof AppConfig, key: string, value: any) => void;
}

export default function AgentSettings({ config, updateField, onAiRepairChange }: AgentSettingsProps) {
  const [roleOpen,setRoleOpen]=useState(false);
  return (
    <section className="settings-card">
      <header className="settings-card-header">
        <div><h3>身份与工作区</h3><p>定义智能体名称、默认目录、用户提示词与角色。</p></div>
      </header>
      <div className="settings-card-body">
      <label className="flex items-start gap-3 rounded border border-[var(--border-soft)] p-3">
        <input type="checkbox" aria-label="AI 辅助修复" checked={config.ai_repair_enabled === true} disabled={!onAiRepairChange} onChange={(event) => onAiRepairChange?.(event.target.checked)} />
        <span><span className="block font-medium">AI 辅助修复</span><span className="text-xs text-[var(--text-secondary)]">开启后使用当前模型分析真实失败原因，消耗计入任务预算；应用修复前仍需确认。</span></span>
      </label>
      <Input label="名称" value={config.agent.name} onChange={(e) => updateField("agent", "name", e.target.value)} />
      <Input label="工作区根目录" value={config.agent.workspace_root} onChange={(e) => updateField("agent", "workspace_root", e.target.value)} placeholder="默认当前目录" />
      <div><label className="settings-control-label">角色与用户提示词</label><p className="settings-help">设置小涟的表达方式和角色，保存后用于下一轮文字与语音对话。</p><Button onClick={()=>setRoleOpen(true)}>编辑用户提示词与角色</Button><PersonaDialog open={roleOpen} agentId="default" onClose={()=>setRoleOpen(false)}/></div>
      </div>
    </section>
  );
}
