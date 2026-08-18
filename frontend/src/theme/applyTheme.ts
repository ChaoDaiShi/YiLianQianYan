import type { ThemeConfig } from "./types";
import { PROTECTED_THEME_VAR_NAMES, THEME_VAR_WHITELIST } from "./types";
import { CYRENE_SEMANTIC_TOKENS, PRESETS } from "./presets";

function withOpacity(color: string, opacity: number): string {
  if (color.startsWith("rgba(")) {
    return color.replace(/[\d.]+\)$/, `${opacity})`);
  }
  if (color.startsWith("rgb(")) {
    return color.replace("rgb(", "rgba(").replace(")", `, ${opacity})`);
  }
  if (color.startsWith("#") && color.length === 7) {
    const r = parseInt(color.slice(1, 3), 16);
    const g = parseInt(color.slice(3, 5), 16);
    const b = parseInt(color.slice(5, 7), 16);
    return `rgba(${r},${g},${b},${opacity})`;
  }
  return color;
}

function buildSemanticTokens(
  theme: ThemeConfig,
  legacy: Record<string, string>,
  hasLegacyOverrides: boolean,
): Record<string, string> {
  const cyreneColors = PRESETS["cyrene-ripple"].colors;
  const hasCyrenePalette = Object.entries(cyreneColors).every(
    ([key, value]) => theme.colors[key as keyof typeof cyreneColors] === value,
  );

  if (hasCyrenePalette && !hasLegacyOverrides) {
    return { ...CYRENE_SEMANTIC_TOKENS };
  }

  const accent = legacy["--accent"];
  const accentFg = legacy["--accent-fg"];
  const backdrop = legacy["--backdrop"];
  const danger = legacy["--danger"];
  const info = legacy["--info"];
  const nav = legacy["--nav"];
  const navText = legacy["--nav-text"];
  const success = legacy["--success"];
  const warning = legacy["--warning"];

  return {
    "--bg-app": legacy["--bg"],
    "--bg-soft": legacy["--bg-2"],
    "--bg-subtle": legacy["--bg-2"],
    "--surface": legacy["--panel"],
    "--surface-solid": legacy["--panel-2"],
    "--surface-muted": legacy["--panel-hover"],
    "--surface-elevated": legacy["--panel-2"],
    "--surface-hover": legacy["--panel-hover"],
    "--titlebar-bg": legacy["--panel-2"],
    "--text-primary": legacy["--text"],
    "--text-secondary": legacy["--text-muted"],
    "--text-faint": legacy["--text-faint"],
    "--accent-primary": accent,
    "--accent-primary-hover": accent,
    "--accent-contrast": accentFg,
    "--accent-soft": withOpacity(accent, 0.16),
    "--accent-purple": accent,
    "--accent-blue": info,
    "--accent-gold": warning,
    "--accent-border": withOpacity(accent, 0.25),
    "--focus-ring-soft": withOpacity(legacy["--focus-ring"], 0.3),
    "--divider": legacy["--border"],
    "--border-soft": legacy["--border"],
    "--success-soft": withOpacity(success, 0.12),
    "--success-border": withOpacity(success, 0.3),
    "--success-fg": legacy["--text"],
    "--warning-soft": withOpacity(warning, 0.12),
    "--warning-border": withOpacity(warning, 0.3),
    "--warning-fg": legacy["--text"],
    "--danger-soft": withOpacity(danger, 0.12),
    "--danger-border": withOpacity(danger, 0.3),
    "--danger-fg": legacy["--text"],
    "--info-soft": withOpacity(info, 0.12),
    "--info-border": withOpacity(info, 0.3),
    "--info-fg": legacy["--text"],
    "--accent-soft-fg": legacy["--text"],
    "--sidebar-bg": nav,
    "--sidebar-bg-2": nav,
    "--sidebar-text": navText,
    "--sidebar-fg": navText,
    "--sidebar-muted": withOpacity(navText, 0.68),
    "--sidebar-active": withOpacity(accent, 0.17),
    "--cyrene-glow-pink": withOpacity(accent, 0.22),
    "--cyrene-glow-purple": withOpacity(accent, 0.18),
    "--cyrene-glow-blue": withOpacity(info, 0.14),
    "--cyrene-glass-border": withOpacity(legacy["--text"], 0.18),
    "--cyrene-surface-tint": withOpacity(legacy["--panel-2"], 0.34),
    "--cyrene-night": nav,
    "--cyrene-water": withOpacity(info, 0.18),
    "--cyrene-ripple": withOpacity(accent, 0.18),
    "--shadow-card": `0 4px 16px ${withOpacity(backdrop, 0.04)}`,
    "--shadow-float": `0 12px 36px ${withOpacity(backdrop, 0.10)}`,
    "--radius-xs": "6px",
    "--radius-sm": CYRENE_SEMANTIC_TOKENS["--radius-sm"],
    "--radius-md": CYRENE_SEMANTIC_TOKENS["--radius-md"],
    "--radius-lg": CYRENE_SEMANTIC_TOKENS["--radius-lg"],
    "--radius-xl": "20px",
    "--motion-fast": CYRENE_SEMANTIC_TOKENS["--motion-fast"],
    "--motion-normal": CYRENE_SEMANTIC_TOKENS["--motion-normal"],
  };
}

export function applyThemeToDom(theme: ThemeConfig, bgImageUrl?: string | null) {
  const root = document.documentElement;
  const variables = buildThemeVariables(theme);

  for (const [key, value] of Object.entries(variables)) {
    root.style.setProperty(key, value);
  }

  if (theme.bgMode === "gradient") {
    const from = theme.gradientFrom || theme.colors.bg;
    const to = theme.gradientTo || theme.colors.bg2;
    root.style.setProperty("--bg-image", `linear-gradient(160deg, ${from}, ${to})`);
  } else if (theme.bgMode === "image" && bgImageUrl) {
    root.style.setProperty("--bg-image", `url(${bgImageUrl})`);
  } else {
    root.style.setProperty("--bg-image", "none");
  }

  root.classList.toggle("theme-mono", theme.monoTitles);
  root.dataset.theme = theme.presetId;
  // Keep these compatibility classes while existing dark: utilities are migrated.
  const isLight =
    theme.presetId === "cyrene-ripple" ||
    theme.presetId === "warm-local" ||
    theme.presetId === "precision-neutral";
  root.classList.toggle("dark", !isLight);
  root.classList.toggle("light", isLight);
}

export function buildThemeVariables(theme: ThemeConfig): Record<string, string> {
  const c = theme.colors;
  const legacyVariables: Record<string, string> = {
    "--bg": c.bg,
    "--bg-2": c.bg2,
    "--panel": withOpacity(c.panel, theme.panelOpacity),
    "--panel-2": c.panel2,
    "--panel-hover": c.panelHover,
    "--text": c.text,
    "--text-muted": c.textMuted,
    "--text-faint": c.textFaint,
    "--border": c.border,
    "--accent": c.accent,
    "--accent-fg": c.accentFg,
    "--input-bg": c.inputBg,
    "--success": c.success,
    "--warning": c.warning,
    "--danger": c.danger,
    "--nav": c.nav,
    "--nav-text": c.navText,
    "--info": c.info,
    "--focus-ring": c.focusRing,
    "--backdrop": c.backdrop,
    "--font-size-base": `${theme.fontSize}px`,
    "--bg-blur": `${theme.blur}px`,
    "--bg-brightness": String(theme.brightness),
  };

  let hasLegacyOverrides = false;
  for (const [key, value] of Object.entries(theme.customVars || {})) {
    if (
      (THEME_VAR_WHITELIST as readonly string[]).includes(key) &&
      !(PROTECTED_THEME_VAR_NAMES as readonly string[]).includes(key) &&
      typeof value === "string"
    ) {
      legacyVariables[key] = value;
      hasLegacyOverrides = true;
    }
  }

  return {
    ...legacyVariables,
    ...buildSemanticTokens(theme, legacyVariables, hasLegacyOverrides),
  };
}

export function sanitizeCustomVars(raw: unknown): Record<string, string> {
  if (!raw || typeof raw !== "object") return {};
  const out: Record<string, string> = {};
  for (const [k, v] of Object.entries(raw as Record<string, unknown>)) {
    if ((THEME_VAR_WHITELIST as readonly string[]).includes(k) && typeof v === "string" && v.length < 200) {
      // Allow colors, lengths, simple values — block url(javascript:) etc.
      if (/javascript:|expression\(|@import/i.test(v)) continue;
      out[k] = v;
    }
  }
  return out;
}
