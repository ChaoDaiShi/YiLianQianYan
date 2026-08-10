import { describe, expect, it } from "vitest";
import { DEFAULT_THEME, PRESETS } from "./presets";
import { normalizeStoredTheme } from "./ThemeProvider";

describe("theme presets", () => {
  it("uses warm-local as the default", () => {
    expect(DEFAULT_THEME.presetId).toBe("warm-local");
  });

  it("exposes the four approved presets", () => {
    expect(Object.keys(PRESETS)).toEqual([
      "warm-local",
      "precision-neutral",
      "graphite-pro",
      "high-contrast",
    ]);
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
});
