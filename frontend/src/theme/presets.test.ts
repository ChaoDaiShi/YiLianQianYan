import { describe, expect, it } from "vitest";
import { DEFAULT_THEME, PRESETS, PRESET_META } from "./presets";
import { normalizeStoredTheme } from "./ThemeProvider";

describe("theme presets", () => {
  it("uses cyrene-ripple as the default for new installations", () => {
    expect(DEFAULT_THEME.presetId).toBe("cyrene-ripple");
    expect(DEFAULT_THEME.colors.bg).toBe("#f9f7ff");
    expect(DEFAULT_THEME.colors.bg2).toBe("#f4f0fc");
    expect(DEFAULT_THEME.colors.panel).toBe("rgba(255,255,255,0.78)");
    expect(DEFAULT_THEME.colors.panelHover).toBe("#f9f3fc");
    expect(DEFAULT_THEME.colors.textMuted).toBe("#696276");
    expect(DEFAULT_THEME.colors.textFaint).toBe("#9690A1");
    expect(DEFAULT_THEME.colors.accent).toBe("#ea91b9");
  });

  it("keeps all approved presets including cyrene-ripple", () => {
    expect(Object.keys(PRESETS)).toEqual([
      "cyrene-ripple",
      "warm-local",
      "precision-neutral",
      "graphite-pro",
      "high-contrast",
    ]);
  });

  it("exposes cyrene-ripple in the appearance preset metadata", () => {
    expect(PRESET_META[0]).toMatchObject({
      id: "cyrene-ripple",
      name: "昔涟 · 涟漪",
    });
  });

  it("migrates legacy paper-light to warm-local", () => {
    expect(normalizeStoredTheme({ presetId: "paper-light" }).presetId).toBe(
      "warm-local"
    );
  });

  it("migrates legacy claude-dark to graphite-pro", () => {
    expect(normalizeStoredTheme({ presetId: "claude-dark" }).presetId).toBe(
      "graphite-pro"
    );
  });

  it("normalizes an unknown stored preset to cyrene-ripple", () => {
    expect(normalizeStoredTheme({ presetId: "missing-preset" }).presetId).toBe(
      "cyrene-ripple"
    );
  });

  it("preserves custom settings while normalizing storage", () => {
    const normalized = normalizeStoredTheme({
      presetId: "custom",
      fontSize: 17,
      panelOpacity: 0.72,
      customVars: { "--accent": "#123456" },
    });

    expect(normalized.presetId).toBe("custom");
    expect(normalized.fontSize).toBe(17);
    expect(normalized.panelOpacity).toBe(0.72);
    expect(normalized.customVars).toEqual({ "--accent": "#123456" });
  });

  it("uses the canonical Cyrene palette instead of stale persisted preset colors", () => {
    const normalized = normalizeStoredTheme({
      presetId: "cyrene-ripple",
      colors: {
        ...DEFAULT_THEME.colors,
        panel: "rgba(255,255,255,0.82)",
        panelHover: "#faf5ff",
      },
    });

    expect(normalized.colors.panel).toBe("rgba(255,255,255,0.78)");
    expect(normalized.colors.panelHover).toBe("#f9f3fc");
  });
});
