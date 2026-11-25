import { Select, Textarea } from "../../../components/ui";
import type { AppConfig } from "../../../types";

interface SandboxSettingsProps {
  config: AppConfig;
  updateField: (section: keyof AppConfig, key: string, value: any) => void;
}

const writeProfiles = [
  { value: "workspace-write", label: "仅工作区可写" },
  { value: "read-only", label: "只读" },
  { value: "custom", label: "自定义路径" },
  { value: "open", label: "不限写入范围" },
];

export default function SandboxSettings({ config, updateField }: SandboxSettingsProps) {
  const { profile, writable_paths, denied_write_paths } = config.sandbox;
  const custom = profile === "custom";
  const writablePaths = <Textarea label="可写路径" hint="用于自定义模式，每行一个路径。" rows={2}
    value={writable_paths.join("\n")} onChange={(e) => updateField("sandbox", "writable_paths", e.target.value.split("\n"))} />;

  return (
    <div className="space-y-3">
      <Select label="文件写入范围" value={profile} options={writeProfiles} onChange={(e) => updateField("sandbox", "profile", e.target.value)} />
      {profile === "open" && <p className="settings-help">不限写入范围会扩大本机写入权限。禁止写入路径仍优先执行。</p>}
      {custom && writablePaths}

      <details className="u2-disclosure">
        <summary>高级选项</summary>
        <div className="space-y-3 pt-2">
          {!custom && writablePaths}
          <Textarea label="禁止写入路径" hint="每行一个路径，优先于所选写入范围。" rows={2}
            value={denied_write_paths.join("\n")} onChange={(e) => updateField("sandbox", "denied_write_paths", e.target.value.split("\n"))} />
          <p className="settings-help">相对路径以工作区为基准。</p>
        </div>
      </details>
    </div>
  );
}
