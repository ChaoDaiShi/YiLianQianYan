import { useState, type ChangeEvent } from "react";
import { Button } from "../../../components/ui";
import { useTheme } from "../../../theme";
import { APPEARANCE_MODES } from "../model/config";

function readBackground(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(reader.result as string);
    reader.onerror = () => reject(reader.error);
    reader.readAsDataURL(file);
  });
}

export default function AppearanceSettings({ theme }: { theme: ReturnType<typeof useTheme> }) {
  const [backgroundBusy, setBackgroundBusy] = useState(false);
  const [backgroundError, setBackgroundError] = useState("");
  const { fontSize, panelOpacity, blur } = theme.theme;
  const opacityPercent = Math.round(panelOpacity * 100);

  async function saveBackground(action: () => Promise<void>, errorMessage: string) {
    setBackgroundBusy(true);
    setBackgroundError("");
    try {
      await action();
    } catch {
      setBackgroundError(errorMessage);
    } finally {
      setBackgroundBusy(false);
    }
  }

  function uploadBackground(event: ChangeEvent<HTMLInputElement>) {
    const file = event.currentTarget.files?.[0];
    if (!file) return;
    event.currentTarget.value = "";
    void saveBackground(async () => theme.setBackgroundImage(await readBackground(file)), "无法读取或保存背景图，请重试。");
  }

  return (
    <div className="space-y-4">
      <fieldset role="radiogroup" aria-label="外观模式" className="min-w-0">
        <legend className="mb-2 text-sm font-semibold">界面模式</legend>
        <div className="flex flex-wrap gap-2">
          {APPEARANCE_MODES.map((mode) => (
            <label key={mode.id} className={"flex cursor-pointer items-center gap-2 rounded-lg border px-3 py-2 text-sm " +
              (theme.mode === mode.id ? "border-[var(--accent-border)] bg-[var(--accent-soft)]" : "border-[var(--border-soft)]")}>
              <input type="radio" name="appearance-mode" value={mode.id} checked={theme.mode === mode.id}
                className="accent-[var(--accent-soft-fg)]" onChange={() => theme.setMode(mode.id)} />
              {mode.label}
            </label>
          ))}
        </div>
      </fieldset>

      <label className="block text-sm">
        <span className="flex justify-between gap-2"><span>界面字号</span><span>{fontSize} px</span></span>
        <input aria-label="界面字号" type="range" min="12" max="20" step="1" value={fontSize}
          onChange={(event) => theme.updateTheme({ fontSize: Number(event.target.value) })}
          className="mt-2 block w-full accent-[var(--accent-primary)]" />
      </label>

      <div className="flex flex-wrap items-center gap-2">
        <label className="settings-upload-action focus-within:outline focus-within:outline-2 focus-within:outline-[var(--focus-outline)]">
          上传背景图
          <input aria-label="上传背景图" type="file" accept="image/*" disabled={backgroundBusy} onChange={uploadBackground} className="sr-only" />
        </label>
        {theme.bgImageUrl && <Button variant="secondary" size="sm" disabled={backgroundBusy}
          onClick={() => void saveBackground(() => theme.setBackgroundImage(null), "无法恢复内置背景，请重试。")}>恢复内置背景</Button>}
      </div>
      {backgroundBusy && <p className="settings-help" role="status">正在保存背景设置…</p>}
      {backgroundError && <p className="settings-save-error" role="alert">{backgroundError}</p>}
      <p className="settings-help">外观修改即时应用并自动保存。</p>

      <details className="u2-disclosure">
        <summary>高级选项</summary>
        <div className="space-y-3 pt-2">
          <label className="block text-sm">
            <span className="flex justify-between gap-2"><span>面板不透明度</span><span>{opacityPercent}%</span></span>
            <input aria-label="面板不透明度" type="range" min="35" max="100" value={opacityPercent}
              onChange={(event) => theme.updateTheme({ panelOpacity: Number(event.target.value) / 100 })}
              className="mt-2 block w-full accent-[var(--accent-primary)]" />
          </label>
          <label className="block text-sm">
            <span className="flex justify-between gap-2"><span>背景模糊</span><span>{blur} px</span></span>
            <input aria-label="背景模糊" type="range" min="0" max="24" value={blur}
              onChange={(event) => theme.updateTheme({ blur: Number(event.target.value) })}
              className="mt-2 block w-full accent-[var(--accent-primary)]" />
          </label>
          <Button variant="secondary" size="sm" disabled={backgroundBusy}
            onClick={() => void saveBackground(theme.resetTheme, "无法重置外观，请重试。")}>重置默认</Button>
        </div>
      </details>
    </div>
  );
}
