import { describe, expect, it } from "vitest";
import { CYRENE_SEMANTIC_TOKENS } from "./presets";
import { DERIVED_TOKENS } from "./tokens";
import { stylesheetText } from "../styles/stylesheet";

describe("Theme token contract and single source of truth", () => {
  it("freezes all Cyrene semantic and shared keys in light and dark presets", () => {
    const lightKeys = Object.keys(CYRENE_SEMANTIC_TOKENS.light).sort();
    const darkKeys = Object.keys(CYRENE_SEMANTIC_TOKENS.dark).sort();

    // 58 semantic keys + 7 shared keys = 65 total keys
    expect(lightKeys.length).toBe(65);
    expect(darkKeys.length).toBe(65);
    expect(lightKeys).toEqual(darkKeys);
  });

  it("ensures base.css :root matches CYRENE_SEMANTIC_TOKENS.light without value drift", () => {
    const rootBlockMatch = stylesheetText.match(/:root\s*\{([\s\S]*?)\}/);
    expect(rootBlockMatch).toBeTruthy();
    const rootBlock = rootBlockMatch![1];

    const normalize = (val: string) => val.replace(/\s+/g, "").toLowerCase();

    for (const [key, expectedValue] of Object.entries(CYRENE_SEMANTIC_TOKENS.light)) {
      const escaped = key.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
      const varMatch = rootBlock.match(new RegExp(`${escaped}\\s*:\\s*([^;]+);`));
      expect(
        varMatch,
        `Expected ${key} to be declared in base.css :root`,
      ).toBeTruthy();
      const actualValue = varMatch![1].trim();
      expect(
        normalize(actualValue),
        `Value drift on token ${key}: base.css has "${actualValue}" but presets.ts has "${expectedValue}"`,
      ).toBe(normalize(expectedValue));
    }
  });

  it("defines all derived and alias tokens in stylesheet", () => {
    for (const token of Object.keys(DERIVED_TOKENS)) {
      expect(
        stylesheetText,
        `Expected derived token ${token} to be present in stylesheet`,
      ).toContain(token);
    }
  });

  it("derives the focus outline from the frozen palette and never overrides it with a new hue", () => {
    // The global :focus-visible rule must consume the derived outline token, which
    // base.css resolves from --accent-soft-fg (light) / --accent-primary (dark).
    expect(stylesheetText).toContain("outline: 2px solid var(--focus-outline)");
    expect(stylesheetText).toContain("--focus-outline: var(--accent-soft-fg)");
    expect(stylesheetText).toContain("--focus-outline: var(--accent-primary)");
    // A one-off literal focus color must not be reintroduced.
    expect(stylesheetText).not.toMatch(/--focus-ring:\s*#[0-9a-fA-F]{3,8}/);
    expect(stylesheetText).not.toContain("#9D4373");
  });

  it("contains no undefined CSS variables in stylesheet", () => {
    const varUsages = [...stylesheetText.matchAll(/var\(\s*(--[a-zA-Z0-9_-]+)/g)].map((m) => m[1]);
    const uniqueUsages = Array.from(new Set(varUsages));

    const declaredTokens = new Set([
      ...Object.keys(CYRENE_SEMANTIC_TOKENS.light),
      ...Object.keys(CYRENE_SEMANTIC_TOKENS.dark),
      ...Object.keys(DERIVED_TOKENS),
      // typography tokens
      "--font-size-base",
      "--font-size-caption",
      "--font-size-meta",
      "--font-size-secondary",
      "--font-size-control",
      "--font-size-body",
      "--font-size-reading",
      "--font-size-title-sm",
      "--font-size-title-md",
      "--font-size-title-lg",
      "--line-height-caption",
      "--line-height-meta",
      "--line-height-body",
      "--line-height-reading",
      "--line-height-title",
      "--font-mono",
      // standard layout/content tokens
      "--content-reading-max",
      "--content-form-max",
      "--content-dashboard-max",
      "--page-padding-inline",
      "--page-padding-block",
      "--bg-blur",
      "--bg-brightness",
      "--bg-image",
      "--voice-dock-height",
      "--overlay-safe-bottom",
      "--chat-voice-expanded-space",
      "--settings-icon-color",
      "--cap-transition",
      "--bg",
      "--bg-2",
      "--panel",
      "--panel-2",
      "--panel-hover",
      "--text",
      "--text-muted",
      "--border",
      "--accent",
      "--accent-fg",
      "--input-bg",
      "--success",
      "--warning",
      "--danger",
      "--nav",
      "--nav-text",
      "--info",
      "--focus-ring",
      "--backdrop",
      "--shadow-soft",
      "--surface-l1",
      "--surface-l2",
      "--surface-l3",
      "--cyrene-specular-border",
      "--cyrene-gradient-primary",
      "--accent-contrast-vivid",
      "--cyrene-shadow-sm",
      "--cyrene-shadow-lg",
      "--z-ambient",
      "--z-canvas",
      "--z-floating-toolbar",
      "--z-rail-drawer",
      "--z-nav-rail",
      "--z-sticky-header",
      "--z-dropdown-popover",
      "--z-modal-approval",
      "--z-command-palette",
      "--z-toast-tooltip",
      "--status-success",
      "--status-success-bg",
      "--status-warning",
      "--status-warning-bg",
      "--status-danger",
      "--status-danger-bg",
      "--status-running",
      "--status-running-bg",
    ]);

    const missingTokens = uniqueUsages.filter((token) => !declaredTokens.has(token));
    expect(
      missingTokens,
      `Found undefined CSS variable usages: ${missingTokens.join(", ")}`,
    ).toEqual([]);
  });

  it("verifies all CSS variable usages across TSX/TS files are declared and no invalid gradient background classes exist", () => {
    // @ts-expect-error -- Node file access is test-only
    const { readdirSync, statSync, readFileSync } = require("node:fs");
    // @ts-expect-error -- Node file access is test-only
    const { join, resolve } = require("node:path");
    // @ts-expect-error -- Node file access is test-only
    const { fileURLToPath } = require("node:url");

    function getAllSourceFiles(dir: string): string[] {
      const results: string[] = [];
      for (const entry of readdirSync(dir)) {
        const fullPath = join(dir, entry);
        const stat = statSync(fullPath);
        if (stat.isDirectory()) {
          if (entry !== "node_modules" && entry !== "dist") {
            results.push(...getAllSourceFiles(fullPath));
          }
        } else if (fullPath.endsWith(".tsx") || fullPath.endsWith(".ts")) {
          results.push(fullPath);
        }
      }
      return results;
    }

    const currentFilePath = fileURLToPath(import.meta.url);
    const srcDir = resolve(currentFilePath, "../..");
    const sourceFiles = getAllSourceFiles(srcDir);
    const tsxUsages = new Set<string>();
    const invalidGradientUsages: string[] = [];

    const declaredTokens = new Set([
      ...Object.keys(CYRENE_SEMANTIC_TOKENS.light),
      ...Object.keys(CYRENE_SEMANTIC_TOKENS.dark),
      ...Object.keys(DERIVED_TOKENS),
      "--font-size-base",
      "--font-size-caption",
      "--font-size-meta",
      "--font-size-secondary",
      "--font-size-control",
      "--font-size-body",
      "--font-size-reading",
      "--font-size-title-sm",
      "--font-size-title-md",
      "--font-size-title-lg",
      "--line-height-caption",
      "--line-height-meta",
      "--line-height-body",
      "--line-height-reading",
      "--line-height-title",
      "--font-mono",
      "--content-reading-max",
      "--content-form-max",
      "--content-dashboard-max",
      "--page-padding-inline",
      "--page-padding-block",
      "--bg-blur",
      "--bg-brightness",
      "--bg-image",
      "--voice-dock-height",
      "--overlay-safe-bottom",
      "--chat-voice-expanded-space",
      "--settings-icon-color",
      "--cap-transition",
      "--bg",
      "--bg-2",
      "--panel",
      "--panel-2",
      "--panel-hover",
      "--text",
      "--text-muted",
      "--border",
      "--accent",
      "--accent-fg",
      "--input-bg",
      "--success",
      "--warning",
      "--danger",
      "--nav",
      "--nav-text",
      "--info",
      "--focus-ring",
      "--backdrop",
      "--shadow-soft",
      "--surface-l1",
      "--surface-l2",
      "--surface-l3",
      "--cyrene-specular-border",
      "--cyrene-gradient-primary",
      "--accent-contrast-vivid",
      "--cyrene-shadow-sm",
      "--cyrene-shadow-lg",
      "--z-ambient",
      "--z-canvas",
      "--z-floating-toolbar",
      "--z-rail-drawer",
      "--z-nav-rail",
      "--z-sticky-header",
      "--z-dropdown-popover",
      "--z-modal-approval",
      "--z-command-palette",
      "--z-toast-tooltip",
      "--status-success",
      "--status-success-bg",
      "--status-warning",
      "--status-warning-bg",
      "--status-danger",
      "--status-danger-bg",
      "--status-running",
      "--status-running-bg",
    ]);

    for (const file of sourceFiles) {
      if (file.includes(".test.") || file.includes("__tests__")) continue;
      const content = readFileSync(file, "utf8");
      // Check for invalid bg-[var(--...gradient...)] in className
      if (content.includes("bg-[var(--cyrene-gradient")) {
        invalidGradientUsages.push(file);
      }
      for (const match of content.matchAll(/var\(\s*(--[a-zA-Z0-9_-]+)/g)) {
        tsxUsages.add(match[1]);
      }
    }

    expect(
      invalidGradientUsages,
      `Found invalid Tailwind bg-[var(--cyrene-gradient)] in TSX: ${invalidGradientUsages.join(", ")}`,
    ).toEqual([]);

    const missingInTsx = Array.from(tsxUsages).filter((token) => !declaredTokens.has(token));
    expect(
      missingInTsx,
      `Found undefined CSS variable usages in TSX/TS: ${missingInTsx.join(", ")}`,
    ).toEqual([]);
  });
});
