import { Input } from "../../../components/ui";
import type { AppConfig } from "../../../types";

interface CompactionSettingsProps {
  config: AppConfig;
  updateField: (section: keyof AppConfig, key: string, value: any) => void;
}

export default function CompactionSettings({ config, updateField }: CompactionSettingsProps) {
  return (
    <section className="settings-card">
      <header className="settings-card-header">
        <div><h3>上下文压缩</h3><p>控制长对话何时压缩，以及保留多少近期上下文。</p></div>
      </header>
      <div className="settings-card-body">
      <label className="settings-toggle-row">
        <span><strong>启用对话压缩</strong><small>上下文达到阈值后自动整理早期内容。</small></span>
        <input
          type="checkbox"
          aria-label="启用对话压缩"
          checked={config.compaction.enabled}
          onChange={(e) => updateField("compaction", "enabled", e.target.checked)}
          className="h-5 w-5 rounded accent-[var(--accent)]"
        />
      </label>
      <Input label="上下文窗口 (tokens)" type="number" value={String(config.compaction.context_window)} onChange={(e) => updateField("compaction", "context_window", parseInt(e.target.value) || 0)} />
      <Input label="触发阈值" type="number" value={String(config.compaction.trigger_threshold)} onChange={(e) => updateField("compaction", "trigger_threshold", parseFloat(e.target.value) || 0)} hint="上下文占用比例超过此值时触发压缩 (0–1)" />
      <Input label="保留最近 (tokens)" type="number" value={String(config.compaction.keep_recent_tokens)} onChange={(e) => updateField("compaction", "keep_recent_tokens", parseInt(e.target.value) || 0)} hint="压缩时保留最近的 token 数量" />
      </div>
    </section>
  );
}
