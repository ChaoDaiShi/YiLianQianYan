import type { AppConfig } from "../../../types";

interface SandboxSettingsProps {
  config: AppConfig;
  updateField: (section: keyof AppConfig, key: string, value: any) => void;
}

export default function SandboxSettings({ config, updateField }: SandboxSettingsProps) {
  return (
    <section className="settings-card settings-card--wide">
      <header className="settings-card-header">
        <div><h3>文件访问边界</h3><p>限制本地执行可以写入的位置。开放模式会扩大主机访问范围。</p></div>
      </header>
      <div className="settings-card-body">
      <div>
        <label className="settings-control-label">安全配置</label>
        <select
          value={config.sandbox.profile}
          onChange={(e) => updateField("sandbox", "profile", e.target.value)}
          className="settings-select"
        >
          <option value="custom">自定义</option>
          <option value="workspace-write">Workspace Write — 仅工作区可写</option>
          <option value="read-only">Read Only — 只读</option>
          <option value="open">Open — 无限制</option>
        </select>
      </div>
      <div className="settings-two-column">
      <div>
        <label className="settings-control-label">可写路径（一行一个）</label>
        <textarea
          value={config.sandbox.writable_paths.join("\n")}
          onChange={(e) => updateField("sandbox", "writable_paths", e.target.value.split("\n").filter(Boolean))}
          rows={3}
          className="settings-textarea"
        />
      </div>
      <div>
        <label className="settings-control-label">禁止写入路径（一行一个）</label>
        <textarea
          value={config.sandbox.denied_write_paths.join("\n")}
          onChange={(e) => updateField("sandbox", "denied_write_paths", e.target.value.split("\n").filter(Boolean))}
          rows={3}
          className="settings-textarea"
        />
      </div>
      </div>
      </div>
    </section>
  );
}
