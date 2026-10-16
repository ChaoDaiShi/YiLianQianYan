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
        className="grid grid-cols-1 gap-4 sm:grid-cols-3"
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
            className={`settings-theme-option ui-opt-appearance-card flex flex-col justify-between ${
              selected
                ? "border-[var(--accent-border)] bg-[var(--accent-soft)]"
                : "border-[var(--border-soft)] bg-[var(--surface-muted)] hover:bg-[var(--surface-hover)]"
            }`}
          >
            {/* Micro thumbnail preview */}
            <div className="ui-opt-theme-thumbnail" data-preview-mode={item.id} aria-hidden="true">
              <div className="ui-opt-thumb-header">
                <span className="ui-opt-thumb-dot" />
                <span className="ui-opt-thumb-dot" />
                <span className="ui-opt-thumb-dot" />
              </div>
              <div className="ui-opt-thumb-body">
                <div className="ui-opt-thumb-nav">
                  <div className="ui-opt-thumb-nav-pill active" />
                  <div className="ui-opt-thumb-nav-pill" />
                </div>
                <div className="ui-opt-thumb-main">
                  <div className="ui-opt-thumb-card">
                    <div className="ui-opt-thumb-bar w-3/4" />
                    <div className="ui-opt-thumb-bar w-1/2" />
                  </div>
                  <div className="ui-opt-thumb-btn" />
                </div>
              </div>
            </div>

            <div className="mt-3">
              <div className="mb-1 flex items-center justify-between gap-2">
                <div className="flex items-center gap-2">
                  <Icon className="h-4 w-4 text-[var(--accent-primary)]" />
                  <span className="text-sm font-medium text-[var(--text-primary)]">
                    {item.label}
                  </span>
                </div>
                {selected ? <Check className="h-4 w-4 text-[var(--accent-primary)]" /> : null}
              </div>
              <span className="block text-[13px] leading-5 text-[var(--text-secondary)]">
                {item.description}
              </span>
            </div>
          </button>
          );
        })}
      </div>
      {theme.mode === "system" ? (
        <p className="settings-help mt-3" role="status">
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
        {theme.bgImageUrl ? (
          <div className="mt-3 flex items-center gap-3">
            <div
              className="h-14 w-24 overflow-hidden rounded-lg border border-[var(--border-soft)] bg-cover bg-center shadow-sm"
              style={{ backgroundImage: `url(${theme.bgImageUrl})` }}
              aria-label="当前背景图预览"
            />
            <span className="text-xs text-[var(--text-secondary)]">已应用自定义背景图</span>
          </div>
        ) : null}
        <div className="mt-3">
          <Button variant="secondary" size="sm" onClick={theme.resetTheme}>
            <RotateCcw className="w-4 h-4" />
            重置默认
          </Button>
        </div>
      </div>
      </section>
    </>
  );
}
