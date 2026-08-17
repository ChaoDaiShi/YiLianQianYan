import { describe, expect, it } from "vitest";
import { buildThemeVariables } from "./applyTheme";
import { DEFAULT_THEME } from "./presets";

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
});
