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
    <div className="space-y-6">
      <div>
        <h3 className="font-semibold text-sm uppercase tracking-wider text-[var(--text-muted)]">
          昔涟外观
        </h3>
        <p className="mt-1 text-xs leading-5 text-[var(--text-faint)]">
          保留昔涟 · 涟漪的统一视觉，只切换适合环境的明暗层级。
        </p>
      </div>
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
            className={`rounded-xl border p-4 text-left transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--focus-ring-soft)] ${
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
            <span className="mt-1 block text-xs leading-5 text-[var(--text-secondary)]">
              {item.description}
            </span>
          </button>
          );
        })}
      </div>
      {theme.mode === "system" ? (
        <p className="text-xs text-[var(--text-secondary)]" role="status">
          当前跟随系统：{theme.resolvedScheme === "dark" ? "夜间" : "白天"}
        </p>
      ) : null}

      <div className="border-t border-[var(--divider)] pt-5">
        <h4 className="text-sm font-medium mb-2">背景</h4>
        <div className="flex items-center gap-3 flex-wrap">
          <label className="inline-flex items-center gap-2 px-3 py-2 rounded-lg border border-[var(--border)] hover:bg-[var(--panel-hover)] cursor-pointer text-sm transition-colors">
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
        <p className="mt-2 text-xs leading-5 text-[var(--text-muted)]">
          背景图片会自动叠加可读性遮罩，不改变风险和验证状态颜色。
        </p>
      </div>

      <div className="flex items-center gap-3">
        <Button variant="secondary" size="sm" onClick={theme.resetTheme}>
          <RotateCcw className="w-4 h-4" />
          重置默认
        </Button>
      </div>
    </div>
  );
}
