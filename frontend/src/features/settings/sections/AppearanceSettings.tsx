import { Check, RotateCcw, Upload } from "lucide-react";
import type { ChangeEvent } from "react";
import { Button } from "../../../components/ui";
import { useTheme } from "../../../theme";
import { APPEARANCE_MODES } from "../model/config";

interface AppearanceSettingsProps {
  theme: ReturnType<typeof useTheme>;
  onBackgroundUpload: (event: ChangeEvent<HTMLInputElement>) => void;
}

/**
 * Only the Cyrene light/dark levels are switchable. A background image is
 * overlaid with a readability wash that never changes risk or verification
 * colours.
 */
export default function AppearanceSettings({ theme, onBackgroundUpload }: AppearanceSettingsProps) {
  return (
    <>
      <section className="settings-card settings-card--wide">
      <header className="settings-card-header"><div><h3>界面模式</h3><p>切换适合当前环境的明暗层级，状态颜色保持一致。</p></div></header>
      <div className="settings-card-body">
      <div
        className="grid grid-cols-1 gap-3 sm:grid-cols-3"
        role="radiogroup"
        aria-label="外观模式"
      >
        {APPEARANCE_MODES.map((item) => {
          const Icon = item.icon;
          const selected = theme.mode === item.id;
          return (
          <button
            key={item.id}
            onClick={() => theme.setMode(item.id)}
            type="button"
            role="radio"
            aria-checked={selected}
            className={`settings-theme-option ${
              selected
                ? "border-[var(--accent-border)] bg-[var(--accent-soft)]"
                : "border-[var(--border-soft)] bg-[var(--surface-muted)] hover:bg-[var(--surface-hover)]"
            }`}
          >
            <div className="mb-2 flex items-center justify-between gap-2">
              <Icon className="h-4 w-4 text-[var(--accent-primary)]" />
              {selected ? <Check className="h-4 w-4 text-[var(--accent-primary)]" /> : null}
            </div>
            <span className="block text-sm font-medium text-[var(--text-primary)]">
              {item.label}
            </span>
            <span className="mt-1 block text-[13px] leading-5 text-[var(--text-secondary)]">
              {item.description}
            </span>
          </button>
          );
        })}
      </div>
      {theme.mode === "system" ? (
        <p className="settings-help" role="status">
          当前跟随系统：{theme.resolvedScheme === "dark" ? "夜间" : "白天"}
        </p>
      ) : null}
      </div>
      </section>
      <section className="settings-card">
        <header className="settings-card-header"><div><h3>背景与重置</h3><p>背景图会自动加可读性遮罩，不改变风险与验证状态颜色。</p></div></header>
        <div className="settings-card-body">
        <div className="flex items-center gap-3 flex-wrap">
          <label className="settings-upload-action">
            <Upload className="w-4 h-4" />
            上传背景图
            <input type="file" accept="image/*" onChange={onBackgroundUpload} className="hidden" />
          </label>
          {theme.bgImageUrl && (
            <Button variant="secondary" size="sm" onClick={() => theme.setBackgroundImage(null)}>
              清除背景
            </Button>
          )}
        </div>
        <Button variant="secondary" size="sm" onClick={theme.resetTheme}>
          <RotateCcw className="w-4 h-4" />
          重置默认
        </Button>
      </div>
      </section>
    </>
  );
}
