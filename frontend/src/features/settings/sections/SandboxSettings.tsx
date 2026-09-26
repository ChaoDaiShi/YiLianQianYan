import type { AppConfig } from "../../../types";

interface SandboxSettingsProps {
  config: AppConfig;
  updateField: (section: keyof AppConfig, key: string, value: any) => void;
}

export default function SandboxSettings({ config, updateField }: SandboxSettingsProps) {
  return (
    <div className="space-y-4">
      <h3 className="font-semibold text-sm uppercase tracking-wider text-[var(--text-muted)]">沙箱配置</h3>
      <div>
        <label className="block text-sm font-medium mb-1">配置文件</label>
        <select
          value={config.sandbox.profile}
          onChange={(e) => updateField("sandbox", "profile", e.target.value)}
          className="w-full rounded-lg border border-[var(--border)] bg-[var(--input-bg)] px-3 py-2 text-sm text-[var(--text)] focus:outline-none focus:ring-2 focus:ring-[var(--accent)]/40"
        >
          <option value="custom">自定义</option>
          <option value="workspace-write">Workspace Write — 仅工作区可写</option>
          <option value="read-only">Read Only — 只读</option>
          <option value="open">Open — 无限制</option>
        </select>
      </div>
      <div>
        <label className="block text-sm font-medium mb-1">可写路径 (一行一个)</label>
        <textarea
          value={config.sandbox.writable_paths.join("\n")}
          onChange={(e) => updateField("sandbox", "writable_paths", e.target.value.split("\n").filter(Boolean))}
          rows={3}
          className="w-full rounded-lg border border-[var(--border)] bg-[var(--input-bg)] px-3 py-2 text-sm text-[var(--text)] focus:outline-none focus:ring-2 focus:ring-[var(--accent)]/40 resize-none"
        />
      </div>
      <div>
        <label className="block text-sm font-medium mb-1">禁止写入路径 (一行一个)</label>
        <textarea
          value={config.sandbox.denied_write_paths.join("\n")}
          onChange={(e) => updateField("sandbox", "denied_write_paths", e.target.value.split("\n").filter(Boolean))}
          rows={3}
          className="w-full rounded-lg border border-[var(--border)] bg-[var(--input-bg)] px-3 py-2 text-sm text-[var(--text)] focus:outline-none focus:ring-2 focus:ring-[var(--accent)]/40 resize-none"
        />
      </div>
    </div>
  );
}
