export type BgMode = "solid" | "gradient" | "image";
export type ThemeMode = "system" | "light" | "dark";
export type ColorScheme = Exclude<ThemeMode, "system">;

export type PresetId =
  | "cyrene-ripple"
  | "warm-local"
  | "precision-neutral"
  | "graphite-pro"
  | "high-contrast"
  | "custom";

export interface ThemeColors {
  bg: string;
  bg2: string;
  panel: string;
  panel2: string;
  panelHover: string;
  text: string;
  textMuted: string;
  textFaint: string;
  border: string;
  accent: string;
  accentFg: string;
  inputBg: string;
  success: string;
  warning: string;
  danger: string;
  nav: string;
  navText: string;
  info: string;
  focusRing: string;
  backdrop: string;
}

export interface ThemeConfig {
  presetId: PresetId;
  mode: ThemeMode;
  bgMode: BgMode;
  colors: ThemeColors;
  gradientFrom?: string;
  gradientTo?: string;
  bgImageKey?: string | null;
  blur: number;
  brightness: number;
  panelOpacity: number;
  fontSize: number;
  monoTitles: boolean;
  customVars: Record<string, string>;
}

export const THEME_VAR_WHITELIST = [
  "--bg",
  "--bg-2",
  "--panel",
  "--panel-2",
  "--panel-hover",
  "--text",
  "--text-muted",
  "--text-faint",
  "--border",
  "--accent",
  "--accent-fg",
  "--input-bg",
  "--success",
  "--warning",
  "--danger",
  "--nav",
  "--nav-text",
  "--info",
  "--focus-ring",
  "--backdrop",
] as const;

export const PROTECTED_THEME_VAR_NAMES = [
  "--bg-app",
  "--bg-soft",
  "--bg-subtle",
  "--surface",
  "--surface-solid",
  "--surface-muted",
  "--surface-elevated",
  "--surface-hover",
  "--text-primary",
  "--accent-primary",
  "--accent-primary-hover",
  "--accent-contrast",
  "--accent-soft",
  "--accent-purple",
  "--accent-blue",
  "--accent-gold",
  "--accent-border",
  "--focus-ring-soft",
  "--divider",
  "--text-secondary",
  "--border-soft",
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
  "--accent-soft-fg",
  "--sidebar-bg",
  "--sidebar-bg-2",
  "--sidebar-text",
  "--sidebar-fg",
  "--sidebar-muted",
  "--sidebar-active",
  "--cyrene-glow-pink",
  "--cyrene-glow-purple",
  "--cyrene-glow-blue",
  "--cyrene-glass-border",
  "--cyrene-surface-tint",
  "--cyrene-night",
  "--cyrene-water",
  "--cyrene-ripple",
  "--shadow-card",
  "--shadow-float",
  "--radius-xs",
  "--radius-sm",
  "--radius-md",
  "--radius-lg",
  "--radius-xl",
  "--motion-fast",
  "--motion-normal",
] as const;

export const STORAGE_KEY = "ylqy.theme";
export const BG_IMAGE_DB = "ylqy-theme-db";
export const BG_IMAGE_STORE = "images";
export const BG_IMAGE_KEY = "background";
