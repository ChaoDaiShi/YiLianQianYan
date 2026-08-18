import { describe, expect, it } from "vitest";
import { buildThemeVariables } from "./applyTheme";
import { CYRENE_SEMANTIC_TOKENS, DEFAULT_THEME, PRESETS } from "./presets";

describe("semantic theme variables", () => {
  it("maps the cyrene preset to semantic and legacy variables", () => {
    const variables = buildThemeVariables(DEFAULT_THEME);

    expect(variables["--bg-app"]).toBe("#F8F6FD");
    expect(variables["--bg-soft"]).toBe("#f4f0fc");
    expect(variables["--bg-subtle"]).toBe("#f4f0fc");
    expect(variables["--surface"]).toBe("rgba(255,255,255,0.42)");
    expect(variables["--surface-muted"]).toBe("rgba(252,249,255,0.20)");
    expect(variables["--titlebar-bg"]).toBe("rgba(255,255,255,0.28)");
    expect(variables["--surface-solid"]).toBe("#ffffff");
    expect(variables["--text-primary"]).toBe("#292536");
    expect(variables["--text-faint"]).toBe("#9690A1");
    expect(variables["--accent-primary"]).toBe("#ea91b9");
    expect(variables["--accent-primary-hover"]).toBe("#df7eaa");
    expect(variables["--accent-soft"]).toBe("#f9dce9");
    expect(variables["--accent-purple"]).toBe("#ad9be8");
    expect(variables["--accent-blue"]).toBe("#99cfea");
    expect(variables["--accent-gold"]).toBe("#ebcf8c");
    expect(variables["--sidebar-bg"]).toBe("#29263a");
    expect(variables["--sidebar-bg-2"]).toBe("#312c46");
    expect(variables["--sidebar-muted"]).toBe("#aaa3ba");
    expect(variables["--sidebar-active"]).toBe("rgba(234,145,185,0.17)");
    expect(variables["--danger-fg"]).toBe("#292536");
    expect(variables["--sidebar-text"]).toBe("#eeeaf8");
    expect(variables["--divider"]).toBe("rgba(91,76,125,0.14)");
    expect(variables["--radius-xs"]).toBe("6px");
    expect(variables["--radius-md"]).toBe("12px");
    expect(variables["--radius-xl"]).toBe("20px");
    expect(variables["--shadow-card"]).toBe(
      "0 4px 16px rgba(54,45,79,0.04)",
    );
    expect(variables["--shadow-float"]).toBe(
      "0 12px 36px rgba(48,38,76,0.10)",
    );
    expect(variables["--bg"]).toBe("#F8F6FD");
    expect(variables["--accent"]).toBe("#ea91b9");
  });

  it("does not let custom variables replace semantic safety tokens", () => {
    const variables = buildThemeVariables({
      ...DEFAULT_THEME,
      customVars: { "--bg-app": "url(javascript:alert(1))", "--accent": "#123456" },
    });

    expect(variables["--bg-app"]).toBe("#F8F6FD");
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
    expect(variables["--bg-soft"]).toBe(PRESETS["precision-neutral"].colors.bg2);
    expect(variables["--surface-solid"]).toBe(PRESETS["precision-neutral"].colors.panel2);
    expect(variables["--text-primary"]).toBe(PRESETS["precision-neutral"].colors.text);
    expect(variables["--accent-primary"]).toBe(PRESETS["precision-neutral"].colors.accent);
    expect(variables["--accent-blue"]).toBe(PRESETS["precision-neutral"].colors.info);
    expect(variables["--accent-gold"]).toBe(PRESETS["precision-neutral"].colors.warning);
    expect(variables["--sidebar-bg"]).toBe(PRESETS["precision-neutral"].colors.nav);
    expect(variables["--danger-fg"]).toBe(PRESETS["precision-neutral"].colors.text);

    expect(variables["--bg-app"]).not.toBe(cyreneVariables["--bg-app"]);
    expect(variables["--accent-primary"]).not.toBe(cyreneVariables["--accent-primary"]);
    expect(variables["--sidebar-bg"]).not.toBe(cyreneVariables["--sidebar-bg"]);
  });

  it.each([
    ["font size", { fontSize: 17 }],
    ["blur", { blur: 18 }],
    ["panel opacity", { panelOpacity: 0.56 }],
    ["background mode", { bgMode: "image" as const, bgImageKey: "background" }],
  ])("keeps the Cyrene semantic base after a %s customization", (_label, patch) => {
    const variables = buildThemeVariables({
      ...DEFAULT_THEME,
      ...patch,
      presetId: "custom",
      colors: { ...DEFAULT_THEME.colors },
      customVars: {},
    });

    expect(variables).toMatchObject(CYRENE_SEMANTIC_TOKENS);
  });

  it("honors explicit color overrides on a Cyrene-origin custom theme", () => {
    const variables = buildThemeVariables({
      ...DEFAULT_THEME,
      presetId: "custom",
      colors: { ...DEFAULT_THEME.colors, accent: "#556677" },
    });

    expect(variables["--accent-primary"]).toBe("#556677");
    expect(variables["--accent-soft"]).toBe("rgba(85,102,119,0.16)");
  });

  it("provides readable semantic foregrounds for every badge tone", () => {
    const tokenNames = [
      "--success-fg",
      "--warning-fg",
      "--danger-fg",
      "--info-fg",
      "--accent-soft-fg",
    ];

    for (const theme of Object.values(PRESETS)) {
      const variables = buildThemeVariables(theme);
      for (const tokenName of tokenNames) {
        expect(variables[tokenName]).toBe(theme.colors.text);
      }
    }
  });

  it("derives semantic variables after allowed legacy custom overrides", () => {
    const variables = buildThemeVariables({
      ...PRESETS["precision-neutral"],
      presetId: "custom",
      customVars: {
        "--bg": "#101112",
        "--bg-2": "#202122",
        "--panel": "rgba(30,31,32,0.91)",
        "--panel-2": "#303132",
        "--panel-hover": "#404142",
        "--text": "#f1f2f3",
        "--text-muted": "#b1b2b3",
        "--text-faint": "#818283",
        "--border": "rgba(241,242,243,0.2)",
        "--accent": "#556677",
        "--accent-fg": "#ffffff",
        "--nav": "#090a0b",
        "--nav-text": "#dedfe0",
        "--warning": "#c0a050",
        "--danger": "#b04050",
        "--info": "#5080b0",
      },
    });

    expect(variables["--bg-app"]).toBe("#101112");
    expect(variables["--bg-soft"]).toBe("#202122");
    expect(variables["--surface"]).toBe("rgba(30,31,32,0.91)");
    expect(variables["--surface-solid"]).toBe("#303132");
    expect(variables["--surface-hover"]).toBe("#404142");
    expect(variables["--text-primary"]).toBe("#f1f2f3");
    expect(variables["--text-secondary"]).toBe("#b1b2b3");
    expect(variables["--text-faint"]).toBe("#818283");
    expect(variables["--border-soft"]).toBe("rgba(241,242,243,0.2)");
    expect(variables["--accent-primary"]).toBe("#556677");
    expect(variables["--accent-soft"]).toBe("rgba(85,102,119,0.16)");
    expect(variables["--accent-blue"]).toBe("#5080b0");
    expect(variables["--accent-gold"]).toBe("#c0a050");
    expect(variables["--sidebar-bg"]).toBe("#090a0b");
    expect(variables["--sidebar-fg"]).toBe("#dedfe0");
    expect(variables["--danger-fg"]).toBe("#f1f2f3");
  });

  it("sets the same variable key set for cyrene and non-cyrene themes", () => {
    const cyreneKeys = Object.keys(buildThemeVariables(DEFAULT_THEME)).sort();
    const nonCyreneKeys = Object.keys(
      buildThemeVariables(PRESETS["graphite-pro"]),
    ).sort();

    expect(nonCyreneKeys).toEqual(cyreneKeys);
  });
});
