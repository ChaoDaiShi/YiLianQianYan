import { useState } from "react";
import { FolderOpen } from "lucide-react";
import { Button, Input } from "../../components/ui";

interface WorkspaceFolderFieldProps {
  value: string;
  onChange: (path: string) => void;
  onPick?: (path: string) => void;
  disabled?: boolean;
}

export default function WorkspaceFolderField({ value, onChange, onPick, disabled }: WorkspaceFolderFieldProps) {
  const [picking, setPicking] = useState(false);
  const [error, setError] = useState("");
  const nativePicker = typeof window !== "undefined" && window.__TAURI_INTERNALS__ !== undefined;

  async function pickFolder() {
    setPicking(true);
    setError("");
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      const path = await invoke<string | null>("pick_workspace_folder");
      if (path !== null) {
        onChange(path);
        onPick?.(path);
      }
    } catch (cause) {
      setError("无法选择文件夹：" + (cause instanceof Error ? cause.message : String(cause)));
    } finally {
      setPicking(false);
    }
  }

  return <div className="space-y-2">
    <div className="flex flex-wrap items-end gap-2">
      <div className="min-w-[160px] flex-1">
        <Input label="本地根目录（可选）" value={value} onChange={event => onChange(event.target.value)}
          placeholder="例如：C:\Projects\MyProject" disabled={disabled || picking} />
      </div>
      {nativePicker && <Button type="button" variant="secondary" disabled={disabled || picking} loading={picking} onClick={() => void pickFolder()}>
        <FolderOpen size={16} />选择文件夹
      </Button>}
    </div>
    {error && <p className="text-xs text-[var(--danger-fg)]" role="alert">{error}</p>}
  </div>;
}
