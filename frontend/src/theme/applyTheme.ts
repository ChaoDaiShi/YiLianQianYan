import type { ColorScheme, ThemeConfig } from "./types";
import { THEME_VAR_WHITELIST } from "./types";
import { CYRENE_SEMANTIC_TOKENS } from "./presets";

export function applyThemeToDom(
  theme: ThemeConfig,
  scheme: ColorScheme,
  bgImageUrl?: string | null,
) {
  const root = document.documentElement;
  const variables = buildThemeVariables(theme, scheme);

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
  root.dataset.theme = "cyrene-ripple";
  root.dataset.colorScheme = scheme;
  root.classList.toggle("dark", scheme === "dark");
  root.classList.toggle("light", scheme === "light");
}

export function buildThemeVariables(
  theme: ThemeConfig,
  scheme: ColorScheme,
): Record<string, string> {
  const semantic = CYRENE_SEMANTIC_TOKENS[scheme];
  const legacyVariables: Record<string, string> = {
    "--bg": semantic["--bg-app"],
    "--bg-2": semantic["--bg-soft"],
    "--panel": semantic["--surface"],
    "--panel-2": semantic["--surface-elevated"],
    "--panel-hover": semantic["--surface-hover"],
    "--text": semantic["--text-primary"],
    "--text-muted": semantic["--text-secondary"],
    "--text-faint": semantic["--text-faint"],
    "--border": semantic["--border-soft"],
    "--accent": semantic["--accent-primary"],
    "--accent-fg": semantic["--accent-contrast"],
    "--input-bg": semantic["--surface-solid"],
    "--success": scheme === "dark" ? "#74C09D" : "#73B99A",
    "--warning": scheme === "dark" ? "#E2BF67" : "#D9B866",
    "--danger": scheme === "dark" ? "#E87E9D" : "#DF7995",
    "--nav": semantic["--sidebar-bg"],
    "--nav-text": semantic["--sidebar-text"],
    "--info": scheme === "dark" ? "#84C5E5" : "#84BCDC",
    "--focus-ring": semantic["--accent-primary"],
    "--backdrop": scheme === "dark"
      ? "rgba(5,4,10,0.58)"
      : "rgba(41,38,58,0.34)",
    "--font-size-base": `${theme.fontSize}px`,
    "--bg-blur": "0px",
    "--bg-brightness": "1",
  };

  return { ...legacyVariables, ...semantic };
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
