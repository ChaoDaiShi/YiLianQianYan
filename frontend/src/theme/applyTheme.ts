import type { ThemeConfig } from "./types";
import { THEME_VAR_WHITELIST } from "./types";

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

export function applyThemeToDom(theme: ThemeConfig, bgImageUrl?: string | null) {
  const root = document.documentElement;
  const c = theme.colors;

  root.style.setProperty("--bg", c.bg);
  root.style.setProperty("--bg-2", c.bg2);
  root.style.setProperty("--panel", withOpacity(c.panel, theme.panelOpacity));
  root.style.setProperty("--panel-2", c.panel2);
  root.style.setProperty("--panel-hover", c.panelHover);
  root.style.setProperty("--text", c.text);
  root.style.setProperty("--text-muted", c.textMuted);
  root.style.setProperty("--text-faint", c.textFaint);
  root.style.setProperty("--border", c.border);
  root.style.setProperty("--accent", c.accent);
  root.style.setProperty("--accent-fg", c.accentFg);
  root.style.setProperty("--input-bg", c.inputBg);
  root.style.setProperty("--success", c.success);
  root.style.setProperty("--warning", c.warning);
  root.style.setProperty("--danger", c.danger);
  root.style.setProperty("--nav", c.nav);
  root.style.setProperty("--nav-text", c.navText);
  root.style.setProperty("--info", c.info);
  root.style.setProperty("--focus-ring", c.focusRing);
  root.style.setProperty("--backdrop", c.backdrop);
  root.style.setProperty("--font-size-base", `${theme.fontSize}px`);
  root.style.setProperty("--bg-blur", `${theme.blur}px`);
  root.style.setProperty("--bg-brightness", String(theme.brightness));

  if (theme.bgMode === "gradient") {
    const from = theme.gradientFrom || c.bg;
    const to = theme.gradientTo || c.bg2;
    root.style.setProperty("--bg-image", `linear-gradient(160deg, ${from}, ${to})`);
  } else if (theme.bgMode === "image" && bgImageUrl) {
    root.style.setProperty("--bg-image", `url(${bgImageUrl})`);
  } else {
    root.style.setProperty("--bg-image", "none");
  }

  for (const [key, value] of Object.entries(theme.customVars || {})) {
    if ((THEME_VAR_WHITELIST as readonly string[]).includes(key) && typeof value === "string") {
      root.style.setProperty(key, value);
    }
  }

  root.classList.toggle("theme-mono", theme.monoTitles);
  root.dataset.theme = theme.presetId;
  // Keep these compatibility classes while existing dark: utilities are migrated.
  const isLight =
    theme.presetId === "warm-local" || theme.presetId === "precision-neutral";
  root.classList.toggle("dark", !isLight);
  root.classList.toggle("light", isLight);
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
