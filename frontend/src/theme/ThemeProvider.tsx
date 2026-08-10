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
import type { PresetId, ThemeConfig } from "./types";
import { STORAGE_KEY } from "./types";
import { DEFAULT_THEME, PRESETS } from "./presets";
import { applyThemeToDom, sanitizeCustomVars } from "./applyTheme";
import { clearBgImage, loadBgImage, saveBgImage } from "./bgImageStore";

interface ThemeContextValue {
  theme: ThemeConfig;
  bgImageUrl: string | null;
  setPreset: (id: PresetId) => void;
  updateTheme: (patch: Partial<ThemeConfig>) => void;
  setBackgroundImage: (dataUrl: string | null) => Promise<void>;
  resetTheme: () => void;
  exportTheme: () => string;
  importTheme: (json: string) => boolean;
}

const ThemeContext = createContext<ThemeContextValue | null>(null);

const LEGACY_PRESET_MAP: Partial<
  Record<string, Exclude<PresetId, "custom">>
> = {
  "paper-light": "warm-local",
  "claude-dark": "graphite-pro",
  "terminal-green": "graphite-pro",
};

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

  const rawPresetId =
    typeof raw.presetId === "string" ? raw.presetId : DEFAULT_THEME.presetId;
  const migratedPresetId = LEGACY_PRESET_MAP[rawPresetId];
  const isKnownPreset = Object.prototype.hasOwnProperty.call(PRESETS, rawPresetId);
  const presetId: PresetId = migratedPresetId
    ?? (rawPresetId === "custom" || isKnownPreset
      ? (rawPresetId as PresetId)
      : DEFAULT_THEME.presetId);
  const base = presetId === "custom" ? DEFAULT_THEME : PRESETS[presetId];
  const storedColors = !migratedPresetId && isRecord(raw.colors) ? raw.colors : {};

  return {
    ...base,
    ...(raw as Partial<ThemeConfig>),
    presetId,
    colors: { ...base.colors, ...storedColors },
    customVars: sanitizeCustomVars(raw.customVars),
  };
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
      setTheme((prev) => ({ ...prev, bgMode: "image", bgImageKey: "background", presetId: "custom" }));
    } else {
      await clearBgImage();
      setBgImageUrl(null);
      setTheme((prev) => ({ ...prev, bgMode: "solid", bgImageKey: null, presetId: "custom" }));
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
      bgImageUrl,
      setPreset,
      updateTheme,
      setBackgroundImage,
      resetTheme,
      exportTheme,
      importTheme,
    }),
    [theme, bgImageUrl, setPreset, updateTheme, setBackgroundImage, resetTheme, exportTheme, importTheme]
  );

  return createElement(ThemeContext.Provider, { value }, children);
}

export function useTheme() {
  const ctx = useContext(ThemeContext);
  if (!ctx) throw new Error("useTheme must be used within ThemeProvider");
  return ctx;
}
