import { BriefcaseBusiness, ChevronDown, MessageCircle, Sparkles } from "lucide-react";
import { workspaceModes, type WorkspaceMode } from "./chatRequestOptions";
import { ComposerPopover } from "./ComposerPopover";
const icons = { chat: MessageCircle, work: BriefcaseBusiness, create: Sparkles };
export function WorkspaceModeControl({ mode, disabled, onChange }: { mode: WorkspaceMode; disabled?: boolean; onChange?: (mode: WorkspaceMode) => Promise<void> }) {
  const Icon = icons[mode];
  return <ComposerPopover label="切换工作模式" disabled={disabled || !onChange} className="composer-workspace-mode" trigger={<><Icon size={15} /><span>{workspaceModes.find(item => item.value === mode)?.label}</span><ChevronDown size={11} /></>}>
    {close => <div className="composer-menu-options">{workspaceModes.map(item => { const ItemIcon = icons[item.value]; return <button key={item.value} type="button" aria-pressed={mode === item.value} onClick={() => { void onChange?.(item.value).then(close).catch(() => undefined); }}><ItemIcon size={16} /><span><strong>{item.label}</strong><small>{item.description}</small></span>{mode === item.value && <span aria-hidden="true">✓</span>}</button>; })}<p className="composer-menu-hint">小涟会根据实际任务自动调整模式。</p></div>}
  </ComposerPopover>;
}
