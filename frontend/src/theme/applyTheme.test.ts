import { describe, expect, it } from "vitest";
import { buildThemeVariables } from "./applyTheme";
import { DEFAULT_THEME } from "./presets";

describe("Cyrene semantic variables", () => {
  it("emits the restrained light palette", () => {
    const light = buildThemeVariables(DEFAULT_THEME, "light");

    expect(light["--bg-app"]).toBe("#F8F6FD");
    expect(light["--surface"]).toContain("rgba");
    expect(light["--text-primary"]).toBe("#292536");
    expect(light["--accent-primary"]).toBe("#ea91b9");
    expect(light["--shell-art-opacity"]).toBe("0.72");
    expect(light["--code-bg"]).not.toContain("234,145,185");
  });

  it("emits a deep-violet dark palette without pure black", () => {
    const dark = buildThemeVariables(DEFAULT_THEME, "dark");

    expect(dark["--bg-app"]).toBe("#171421");
    expect(dark["--surface"]).toContain("rgba");
    expect(dark["--text-primary"]).toBe("#F3EFF8");
    expect(dark["--accent-primary"]).toBe("#F0A3C6");
    expect(dark["--bg-app"]).not.toBe("#000000");
    expect(dark["--surface-solid"]).not.toBe("#000000");
    expect(dark["--shell-art-opacity"]).toBe("0.26");
  });

  it("uses identical variable keys in light and dark", () => {
    const lightKeys = Object.keys(buildThemeVariables(DEFAULT_THEME, "light")).sort();
    const darkKeys = Object.keys(buildThemeVariables(DEFAULT_THEME, "dark")).sort();

    expect(darkKeys).toEqual(lightKeys);
  });

  it.each(["light", "dark"] as const)(
    "keeps readable status foregrounds in %s",
    (scheme) => {
      const variables = buildThemeVariables(DEFAULT_THEME, scheme);
      for (const key of [
        "--success-fg",
        "--warning-fg",
        "--danger-fg",
        "--info-fg",
      ]) {
        const baseKey = `--${key.slice(2, -3)}`;
        expect(variables[key]).toBeTruthy();
        expect(variables[key]).not.toBe(variables[baseKey]);
      }
    },
  );
});
