import { describe, expect, it } from "vitest";
import { DEFAULT_THEME } from "./presets";
import { normalizeStoredTheme, resolveColorScheme } from "./ThemeProvider";

describe("Cyrene appearance modes", () => {
  it("uses system mode for a new installation", () => {
    expect(DEFAULT_THEME.presetId).toBe("cyrene-ripple");
    expect(DEFAULT_THEME.mode).toBe("system");
  });

  it.each([
    ["system", false, "light"],
    ["system", true, "dark"],
    ["light", true, "light"],
    ["dark", false, "dark"],
  ] as const)("resolves %s with systemDark=%s to %s", (mode, systemDark, expected) => {
    expect(resolveColorScheme(mode, systemDark)).toBe(expected);
  });

  it("preserves an explicit stored mode", () => {
    expect(normalizeStoredTheme({ mode: "dark" }).mode).toBe("dark");
  });

  it.each(["graphite-pro", "claude-dark", "terminal-green"])(
    "migrates legacy dark preset %s to dark",
    (presetId) => expect(normalizeStoredTheme({ presetId }).mode).toBe("dark"),
  );

  it("keeps an existing Cyrene installation visually light", () => {
    expect(normalizeStoredTheme({ presetId: "cyrene-ripple" }).mode).toBe("light");
  });

  it.each(["warm-local", "precision-neutral", "high-contrast", "custom", "missing"])(
    "normalizes legacy preset %s to system",
    (presetId) => expect(normalizeStoredTheme({ presetId }).mode).toBe("system"),
  );

  it("keeps only background image customization from legacy storage", () => {
    const normalized = normalizeStoredTheme({
      presetId: "custom",
      bgMode: "image",
      bgImageKey: "background",
      blur: 22,
      panelOpacity: 0.91,
      fontSize: 18,
      customVars: { "--accent": "#123456" },
    });

    expect(normalized.bgMode).toBe("image");
    expect(normalized.bgImageKey).toBe("background");
    expect(normalized.blur).toBe(0);
    expect(normalized.panelOpacity).toBe(DEFAULT_THEME.panelOpacity);
    expect(normalized.fontSize).toBe(DEFAULT_THEME.fontSize);
    expect(normalized.customVars).toEqual({});
  });
});
