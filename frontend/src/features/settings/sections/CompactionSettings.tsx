import { Input } from "../../../components/ui";
import type { AppConfig } from "../../../types";

interface CompactionSettingsProps {
  config: AppConfig;
  updateField: (section: keyof AppConfig, key: string, value: any) => void;
}

export default function CompactionSettings({ config, updateField }: CompactionSettingsProps) {
  return (
    <div className="space-y-4">
      <h3 className="font-semibold text-sm uppercase tracking-wider text-[var(--text-muted)]">对话压缩</h3>
      <div className="flex items-center gap-3">
        <label className="text-sm">启用压缩</label>
        <input
          type="checkbox"
          aria-label="启用对话压缩"
          checked={config.compaction.enabled}
          onChange={(e) => updateField("compaction", "enabled", e.target.checked)}
          className="w-4 h-4 rounded accent-[var(--accent)]"
        />
      </div>
      <Input label="上下文窗口 (tokens)" type="number" value={String(config.compaction.context_window)} onChange={(e) => updateField("compaction", "context_window", parseInt(e.target.value) || 0)} />
      <Input label="触发阈值" type="number" value={String(config.compaction.trigger_threshold)} onChange={(e) => updateField("compaction", "trigger_threshold", parseFloat(e.target.value) || 0)} hint="上下文占用比例超过此值时触发压缩 (0–1)" />
      <Input label="保留最近 (tokens)" type="number" value={String(config.compaction.keep_recent_tokens)} onChange={(e) => updateField("compaction", "keep_recent_tokens", parseInt(e.target.value) || 0)} hint="压缩时保留最近的 token 数量" />
    </div>
  );
}
