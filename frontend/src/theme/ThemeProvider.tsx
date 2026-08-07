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

function loadStored(): ThemeConfig {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return DEFAULT_THEME;
    const parsed = JSON.parse(raw) as ThemeConfig;
    return {
      ...DEFAULT_THEME,
      ...parsed,
      colors: { ...DEFAULT_THEME.colors, ...parsed.colors },
      customVars: sanitizeCustomVars(parsed.customVars),
    };
  } catch {
    return DEFAULT_THEME;
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
    const preset = PRESETS[id];
    if (preset) setTheme({ ...preset });
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
    setTheme(DEFAULT_THEME);
    clearBgImage().then(() => setBgImageUrl(null));
  }, []);

  const exportTheme = useCallback(() => JSON.stringify(theme, null, 2), [theme]);

  const importTheme = useCallback((json: string) => {
    try {
      const parsed = JSON.parse(json) as ThemeConfig;
      setTheme({
        ...DEFAULT_THEME,
        ...parsed,
        colors: { ...DEFAULT_THEME.colors, ...parsed.colors },
        customVars: sanitizeCustomVars(parsed.customVars),
      });
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
