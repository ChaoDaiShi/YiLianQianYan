import { Input } from "../../../components/ui";
import type { AppConfig } from "../../../types";

interface CompactionSettingsProps {
  config: AppConfig;
  updateField: (section: keyof AppConfig, key: string, value: any) => void;
}

export default function CompactionSettings({ config, updateField }: CompactionSettingsProps) {
  return (
    <div className="space-y-3">
      <label className="settings-toggle-row">
        <span><strong>自动压缩长对话</strong><small>达到压缩阈值时整理早期内容。</small></span>
        <input
          type="checkbox"
          aria-label="启用对话压缩"
          checked={config.compaction.enabled}
          onChange={(e) => updateField("compaction", "enabled", e.target.checked)}
          className="h-5 w-5 rounded accent-[var(--accent)]"
        />
      </label>
      <details className="u2-disclosure">
        <summary>高级选项</summary>
        <div className="space-y-3 pt-2">
          <Input label="上下文窗口（Token）" type="number" min={1} value={String(config.compaction.context_window)} onChange={(e) => updateField("compaction", "context_window", parseInt(e.target.value) || 0)} />
          <Input label="触发阈值" type="number" min={0} max={1} step="any" value={String(config.compaction.trigger_threshold)} onChange={(e) => updateField("compaction", "trigger_threshold", parseFloat(e.target.value) || 0)} hint="上下文占用达到此比例时触发压缩（0–1）。" />
          <Input label="保留最近的内容（Token）" type="number" min={0} value={String(config.compaction.keep_recent_tokens)} onChange={(e) => updateField("compaction", "keep_recent_tokens", parseInt(e.target.value) || 0)} />
        </div>
      </details>
    </div>
  );
}
