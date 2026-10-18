import { CYRENE_SEMANTIC_TOKENS, CYRENE_SHARED_TOKENS } from "./presets";
import type { ColorScheme } from "./types";

/**
 * Single source of truth for Cyrene Ripple tokens.
 * Values are frozen in presets.ts - this file exports groupings and derived definitions.
 */
export const CYRENE_TOKENS = CYRENE_SEMANTIC_TOKENS;

export interface TokenGroup {
  name: string;
  label: string;
  keys: string[];
}

export const TOKEN_GROUPS: TokenGroup[] = [
  {
    name: "canvas",
    label: "画布与表面",
    keys: [
      "--bg-app",
      "--bg-soft",
      "--bg-subtle",
      "--surface",
      "--surface-muted",
      "--surface-elevated",
      "--surface-solid",
      "--surface-hover",
      "--titlebar-bg",
    ],
  },
  {
    name: "text",
    label: "文字排版",
    keys: [
      "--text-primary",
      "--text-secondary",
      "--text-faint",
      "--text-disabled",
    ],
  },
  {
    name: "accent",
    label: "品牌强调色",
    keys: [
      "--accent-primary",
      "--accent-primary-hover",
      "--accent-contrast",
      "--accent-soft",
      "--accent-purple",
      "--accent-blue",
      "--accent-gold",
      "--accent-border",
      "--accent-soft-fg",
      "--focus-ring-soft",
    ],
  },
  {
    name: "border",
    label: "边框与分割线",
    keys: [
      "--divider",
      "--border-soft",
    ],
  },
  {
    name: "status",
    label: "状态语义",
    keys: [
      "--success-soft",
      "--success-border",
      "--success-fg",
      "--warning-soft",
      "--warning-border",
      "--warning-fg",
      "--danger-soft",
      "--danger-border",
      "--danger-fg",
      "--info-soft",
      "--info-border",
      "--info-fg",
    ],
  },
  {
    name: "sidebar",
    label: "侧边导航栏",
    keys: [
      "--sidebar-bg",
      "--sidebar-bg-2",
      "--sidebar-text",
      "--sidebar-fg",
      "--sidebar-muted",
      "--sidebar-active",
    ],
  },
  {
    name: "motif",
    label: "昔涟母题与艺术",
    keys: [
      "--cyrene-glow-pink",
      "--cyrene-glow-purple",
      "--cyrene-glow-blue",
      "--cyrene-glass-border",
      "--cyrene-surface-tint",
      "--cyrene-night",
      "--cyrene-water",
      "--cyrene-ripple",
      "--shell-art-opacity",
      "--shell-overlay",
      "--page-wash",
    ],
  },
  {
    name: "shadow_code",
    label: "阴影与代码",
    keys: [
      "--shadow-card",
      "--shadow-float",
      "--code-bg",
      "--code-text",
    ],
  },
  {
    name: "shared",
    label: "共享比例与动效",
    keys: [
      "--radius-xs",
      "--radius-sm",
      "--radius-md",
      "--radius-lg",
      "--radius-xl",
      "--motion-fast",
      "--motion-normal",
    ],
  },
];

/**
 * Derived tokens registered in Appendix B.
 * All derived tokens only use existing Cyrene hues, alphas, or aliases.
 */
export const DERIVED_TOKENS: Record<string, { aliasOf?: string; description: string }> = {
  "--shadow-sm": { description: "Small card/element shadow" },
  "--input-inset-shadow": { description: "Input inner shadow" },
  "--overlay-scrim": { aliasOf: "--backdrop", description: "Modal backdrop alias" },
  "--risk-low-fg": { aliasOf: "--info-fg", description: "Low risk text" },
  "--risk-low-soft": { aliasOf: "--info-soft", description: "Low risk background" },
  "--risk-low-border": { aliasOf: "--info-border", description: "Low risk border" },
  "--risk-medium-fg": { aliasOf: "--warning-fg", description: "Medium risk text" },
  "--risk-medium-soft": { aliasOf: "--warning-soft", description: "Medium risk background" },
  "--risk-medium-border": { aliasOf: "--warning-border", description: "Medium risk border" },
  "--risk-high-fg": { aliasOf: "--danger-fg", description: "High risk text" },
  "--risk-high-soft": { aliasOf: "--danger-soft", description: "High risk background" },
  "--risk-high-border": { aliasOf: "--danger-border", description: "High risk border" },
  "--risk-critical-fg": { aliasOf: "--danger-fg", description: "Critical risk text" },
  "--risk-critical-soft": { aliasOf: "--danger-soft", description: "Critical risk background" },
  "--risk-critical-border": { aliasOf: "--danger", description: "Critical risk strong border" },
  "--z-voice-dock": { description: "Voice dock z-index layer (35)" },
  "--z-savebar": { description: "Settings sticky save bar z-index (55)" },
  "--z-overlay": { description: "General overlay z-index layer (40)" },
  "--code-header-bg": { description: "Code block header background" },
  "--row-selected-bg": { aliasOf: "--sidebar-active", description: "List row selected background" },
  "--shell-readability-gradient": { description: "Shell readability gradient" },
  "--foreground": { aliasOf: "--text-primary", description: "Tailwind compat alias for text-primary" },
  "--foreground-muted": { aliasOf: "--text-secondary", description: "Tailwind compat alias for text-secondary" },
  "--border-strong": { aliasOf: "--divider", description: "Divider border strong alias" },
  "--border-hover": { aliasOf: "--accent-border", description: "Border hover highlight alias" },
  "--accent-subtle": { aliasOf: "--accent-soft", description: "Subtle accent surface alias" },
  "--danger-subtle": { aliasOf: "--danger-soft", description: "Subtle danger surface alias" },
  "--success-subtle": { aliasOf: "--success-soft", description: "Subtle success surface alias" },
  "--warning-subtle": { aliasOf: "--warning-soft", description: "Subtle warning surface alias" },
  "--text-tertiary": { aliasOf: "--text-secondary", description: "Tertiary text alias" },
  "--surface-input": { aliasOf: "--input-bg", description: "Input background alias" },
  "--panel-muted": { aliasOf: "--surface-muted", description: "Muted panel alias" },
};

/**
 * Returns all semantic token keys for Cyrene Ripple.
 */
export function getCyreneTokenKeys(scheme: ColorScheme = "light"): string[] {
  return Object.keys(CYRENE_SEMANTIC_TOKENS[scheme]);
}
