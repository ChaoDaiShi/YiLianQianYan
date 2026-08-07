export type BgMode = "solid" | "gradient" | "image";

export type PresetId = "claude-dark" | "terminal-green" | "paper-light" | "high-contrast" | "custom";

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
}

export interface ThemeConfig {
  presetId: PresetId;
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
] as const;

export const STORAGE_KEY = "ylqy.theme";
export const BG_IMAGE_DB = "ylqy-theme-db";
export const BG_IMAGE_STORE = "images";
export const BG_IMAGE_KEY = "background";
