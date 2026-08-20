import {
  createContext,
  createElement,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";
import type { ColorScheme, PresetId, ThemeConfig, ThemeMode } from "./types";
import { STORAGE_KEY } from "./types";
import { DEFAULT_THEME, PRESETS } from "./presets";
import { applyThemeToDom, sanitizeCustomVars } from "./applyTheme";
import { clearBgImage, loadBgImage, saveBgImage } from "./bgImageStore";

interface ThemeContextValue {
  theme: ThemeConfig;
  mode: ThemeMode;
  resolvedScheme: ColorScheme;
  bgImageUrl: string | null;
  setMode: (mode: ThemeMode) => void;
  setPreset: (id: PresetId) => void;
  updateTheme: (patch: Partial<ThemeConfig>) => void;
  setBackgroundImage: (dataUrl: string | null) => Promise<void>;
  resetTheme: () => void;
  exportTheme: () => string;
  importTheme: (json: string) => boolean;
}

const ThemeContext = createContext<ThemeContextValue | null>(null);

const LEGACY_DARK_PRESETS = new Set([
  "graphite-pro",
  "claude-dark",
  "terminal-green",
]);
const VALID_MODES = new Set<ThemeMode>(["system", "light", "dark"]);

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

export function normalizeStoredTheme(raw: unknown): ThemeConfig {
  if (!isRecord(raw)) {
    return {
      ...DEFAULT_THEME,
      colors: { ...DEFAULT_THEME.colors },
      customVars: {},
    };
  }

  const rawPresetId = typeof raw.presetId === "string" ? raw.presetId : "";
  const rawMode = typeof raw.mode === "string" ? raw.mode : "";
  const mode: ThemeMode = VALID_MODES.has(rawMode as ThemeMode)
    ? (rawMode as ThemeMode)
    : rawPresetId === "cyrene-ripple"
      ? "light"
      : LEGACY_DARK_PRESETS.has(rawPresetId)
        ? "dark"
        : "system";
  const keepBackgroundImage =
    raw.bgMode === "image" && raw.bgImageKey === "background";

  return {
    ...DEFAULT_THEME,
    presetId: "cyrene-ripple",
    mode,
    bgMode: keepBackgroundImage ? "image" : "solid",
    bgImageKey: keepBackgroundImage ? "background" : null,
    colors: { ...DEFAULT_THEME.colors },
    customVars: {},
  };
}

export function resolveColorScheme(
  mode: ThemeMode,
  systemPrefersDark: boolean,
): ColorScheme {
  if (mode === "system") return systemPrefersDark ? "dark" : "light";
  return mode;
}

function readSystemPrefersDark(): boolean {
  return typeof window !== "undefined"
    && typeof window.matchMedia === "function"
    && window.matchMedia("(prefers-color-scheme: dark)").matches;
}

function loadStored(): ThemeConfig {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return normalizeStoredTheme(DEFAULT_THEME);
    return normalizeStoredTheme(JSON.parse(raw));
  } catch {
    return normalizeStoredTheme(DEFAULT_THEME);
  }
}

export function ThemeProvider({ children }: { children: ReactNode }) {
  const [theme, setTheme] = useState<ThemeConfig>(loadStored);
  const [bgImageUrl, setBgImageUrl] = useState<string | null>(null);
  const [systemPrefersDark, setSystemPrefersDark] = useState(readSystemPrefersDark);
  const resolvedScheme = resolveColorScheme(theme.mode, systemPrefersDark);

  useEffect(() => {
    if (theme.mode !== "system" || typeof window.matchMedia !== "function") return;
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const handleChange = (event: MediaQueryListEvent) => {
      setSystemPrefersDark(event.matches);
    };
    setSystemPrefersDark(media.matches);
    media.addEventListener("change", handleChange);
    return () => media.removeEventListener("change", handleChange);
  }, [theme.mode]);

  useEffect(() => {
    loadBgImage().then((url) => {
      setBgImageUrl(url);
      applyThemeToDom(theme, url);
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    applyThemeToDom(theme, bgImageUrl);
    localStorage.setItem(STORAGE_KEY, JSON.stringify(theme));
  }, [theme, bgImageUrl]);

  const setPreset = useCallback((id: PresetId) => {
    if (id === "custom") return;
    const preset = PRESETS[id];
    setTheme({ ...preset, colors: { ...preset.colors }, customVars: {} });
  }, []);

  const setMode = useCallback((mode: ThemeMode) => {
    setTheme((current) => ({ ...current, mode }));
  }, []);

  const updateTheme = useCallback((patch: Partial<ThemeConfig>) => {
    setTheme((prev) => ({
      ...prev,
      ...patch,
      presetId: patch.presetId ?? "custom",
      colors: patch.colors ? { ...prev.colors, ...patch.colors } : prev.colors,
      customVars: patch.customVars !== undefined
        ? sanitizeCustomVars(patch.customVars)
        : prev.customVars,
    }));
  }, []);

  const setBackgroundImage = useCallback(async (dataUrl: string | null) => {
    if (dataUrl) {
      await saveBgImage(dataUrl);
      setBgImageUrl(dataUrl);
      setTheme((prev) => ({ ...prev, bgMode: "image", bgImageKey: "background" }));
    } else {
      await clearBgImage();
      setBgImageUrl(null);
      setTheme((prev) => ({ ...prev, bgMode: "solid", bgImageKey: null }));
    }
  }, []);

  const resetTheme = useCallback(() => {
    setTheme(normalizeStoredTheme(DEFAULT_THEME));
    clearBgImage().then(() => setBgImageUrl(null));
  }, []);

  const exportTheme = useCallback(() => JSON.stringify(theme, null, 2), [theme]);

  const importTheme = useCallback((json: string) => {
    try {
      setTheme(normalizeStoredTheme(JSON.parse(json)));
      return true;
    } catch {
      return false;
    }
  }, []);

  const value = useMemo(
    () => ({
      theme,
      mode: theme.mode,
      resolvedScheme,
      bgImageUrl,
      setMode,
      setPreset,
      updateTheme,
      setBackgroundImage,
      resetTheme,
      exportTheme,
      importTheme,
    }),
    [theme, resolvedScheme, bgImageUrl, setMode, setPreset, updateTheme, setBackgroundImage, resetTheme, exportTheme, importTheme]
  );

  return createElement(ThemeContext.Provider, { value }, children);
}

export function useTheme() {
  const ctx = useContext(ThemeContext);
  if (!ctx) throw new Error("useTheme must be used within ThemeProvider");
  return ctx;
}
