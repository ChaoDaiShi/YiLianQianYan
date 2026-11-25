import { BriefcaseBusiness, ChevronDown, MessageCircle, Sparkles, WandSparkles } from "lucide-react";
import { workspaceModes, type WorkspaceMode } from "./chatRequestOptions";
import { ComposerPopover } from "./ComposerPopover";
import { ComposerOption } from "./ComposerOption";
const icons = { auto: WandSparkles, chat: MessageCircle, work: BriefcaseBusiness, create: Sparkles };
export function WorkspaceModeControl({ mode, disabled, onChange }: { mode: WorkspaceMode; disabled?: boolean; onChange?: (mode: WorkspaceMode) => Promise<void> }) {
  const Icon = icons[mode];
  return <ComposerPopover label="切换工作模式" disabled={disabled || !onChange} className="composer-workspace-mode" trigger={<><Icon size={15} /><span>{workspaceModes.find(item => item.value === mode)?.label}</span><ChevronDown size={11} /></>}>
    {close => <div className="composer-menu-options">
      {workspaceModes.map(item => <ComposerOption key={item.value} icon={icons[item.value]} label={item.label} description={item.description}
        selected={mode === item.value} disabled={disabled} onClick={() => { void onChange?.(item.value).then(close).catch(() => undefined); }} />)}
    </div>}
  </ComposerPopover>;
}
