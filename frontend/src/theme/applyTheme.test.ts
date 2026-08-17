import { describe, expect, it } from "vitest";
import { buildThemeVariables } from "./applyTheme";
import { DEFAULT_THEME, PRESETS } from "./presets";

describe("semantic theme variables", () => {
  it("maps the cyrene preset to semantic and legacy variables", () => {
    const variables = buildThemeVariables(DEFAULT_THEME);

    expect(variables["--bg-app"]).toBe("#f9f7ff");
    expect(variables["--surface-solid"]).toBe("#ffffff");
    expect(variables["--accent-primary"]).toBe("#ea91b9");
    expect(variables["--sidebar-bg"]).toBe("#29263a");
    expect(variables["--radius-md"]).toBe("12px");
    expect(variables["--bg"]).toBe("#f9f7ff");
    expect(variables["--accent"]).toBe("#ea91b9");
  });

  it("does not let custom variables replace semantic safety tokens", () => {
    const variables = buildThemeVariables({
      ...DEFAULT_THEME,
      customVars: { "--bg-app": "url(javascript:alert(1))", "--accent": "#123456" },
    });

    expect(variables["--bg-app"]).toBe("#f9f7ff");
    expect(variables["--accent"]).toBe("#123456");
  });

  it("contains the shared component geometry and motion contract", () => {
    const variables = buildThemeVariables(DEFAULT_THEME);

    expect(variables["--radius-sm"]).toBe("10px");
    expect(variables["--radius-lg"]).toBe("16px");
    expect(variables["--shadow-card"]).toContain("rgba");
    expect(variables["--motion-fast"]).toBe("140ms");
  });

  it("derives non-cyrene semantic variables from the active preset palette", () => {
    const variables = buildThemeVariables(PRESETS["precision-neutral"]);
    const cyreneVariables = buildThemeVariables(DEFAULT_THEME);

    expect(variables["--bg-app"]).toBe(PRESETS["precision-neutral"].colors.bg);
    expect(variables["--surface-solid"]).toBe(PRESETS["precision-neutral"].colors.panel2);
    expect(variables["--accent-primary"]).toBe(PRESETS["precision-neutral"].colors.accent);
    expect(variables["--sidebar-bg"]).toBe(PRESETS["precision-neutral"].colors.nav);

    expect(variables["--bg-app"]).not.toBe(cyreneVariables["--bg-app"]);
    expect(variables["--accent-primary"]).not.toBe(cyreneVariables["--accent-primary"]);
    expect(variables["--sidebar-bg"]).not.toBe(cyreneVariables["--sidebar-bg"]);
  });
});
