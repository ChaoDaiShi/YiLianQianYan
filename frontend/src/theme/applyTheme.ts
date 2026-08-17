import type { ThemeConfig } from "./types";
import { PROTECTED_THEME_VAR_NAMES, THEME_VAR_WHITELIST } from "./types";
import { CYRENE_SEMANTIC_TOKENS } from "./presets";

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

function buildSemanticTokens(theme: ThemeConfig): Record<string, string> {
  if (theme.presetId === "cyrene-ripple") {
    return { ...CYRENE_SEMANTIC_TOKENS };
  }

  const c = theme.colors;

  return {
    "--bg-app": c.bg,
    "--bg-subtle": c.bg2,
    "--surface": withOpacity(c.panel, theme.panelOpacity),
    "--surface-solid": c.panel2,
    "--surface-muted": c.panelHover,
    "--surface-elevated": c.panel2,
    "--surface-hover": c.panelHover,
    "--text-primary": c.text,
    "--text-secondary": c.textMuted,
    "--text-faint": c.textFaint,
    "--accent-primary": c.accent,
    "--accent-primary-hover": c.focusRing,
    "--accent-contrast": c.accentFg,
    "--accent-soft": withOpacity(c.accent, 0.16),
    "--border-soft": c.border,
    "--sidebar-bg": c.nav,
    "--sidebar-fg": c.navText,
    "--shadow-card": `0 18px 40px ${withOpacity(c.backdrop, 0.14)}`,
    "--shadow-float": `0 28px 72px ${withOpacity(c.backdrop, 0.24)}`,
    "--radius-sm": CYRENE_SEMANTIC_TOKENS["--radius-sm"],
    "--radius-md": CYRENE_SEMANTIC_TOKENS["--radius-md"],
    "--radius-lg": CYRENE_SEMANTIC_TOKENS["--radius-lg"],
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
  const variables: Record<string, string> = {
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
    ...buildSemanticTokens(theme),
  };

  for (const [key, value] of Object.entries(theme.customVars || {})) {
    if (
      (THEME_VAR_WHITELIST as readonly string[]).includes(key) &&
      !(PROTECTED_THEME_VAR_NAMES as readonly string[]).includes(key) &&
      typeof value === "string"
    ) {
      variables[key] = value;
    }
  }

  return variables;
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
