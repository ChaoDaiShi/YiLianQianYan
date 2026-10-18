import { describe, expect, it } from "vitest";
import { CYRENE_SEMANTIC_TOKENS, PRESETS } from "./presets";
import { TOKEN_GROUPS } from "./tokens";

function hexToRgb(hex: string): [number, number, number] {
  let clean = hex.replace("#", "");
  if (clean.length === 3) {
    clean = clean.split("").map((c) => c + c).join("");
  }
  const num = parseInt(clean, 16);
  return [(num >> 16) & 255, (num >> 8) & 255, num & 255];
}

function relativeLuminance(r: number, g: number, b: number): number {
  const [rs, gs, bs] = [r, g, b].map((val) => {
    const s = val / 255;
    return s <= 0.03928 ? s / 12.92 : Math.pow((s + 0.055) / 1.055, 2.4);
  });
  return 0.2126 * rs + 0.7152 * gs + 0.0722 * bs;
}

function contrastRatio(hex1: string, hex2: string): number {
  const [r1, g1, b1] = hexToRgb(hex1);
  const [r2, g2, b2] = hexToRgb(hex2);
  const l1 = relativeLuminance(r1, g1, b1);
  const l2 = relativeLuminance(r2, g2, b2);
  const lighter = Math.max(l1, l2);
  const darker = Math.min(l1, l2);
  return (lighter + 0.05) / (darker + 0.05);
}

describe("Theme token coverage and accessibility verification", () => {
  it("keeps cyrene-ripple preset frozen and immutable", () => {
    const cyrene = PRESETS["cyrene-ripple"];
    expect(cyrene).toBeDefined();
    expect(cyrene.presetId).toBe("cyrene-ripple");
    expect(cyrene.colors.accent).toBe("#ea91b9");
    expect(cyrene.colors.bg).toBe("#F8F6FD");
    expect(cyrene.colors.nav.toLowerCase()).toBe("#29263a");
  });

  it("verifies token groups cover all semantic tokens", () => {
    const groupedKeys = new Set(TOKEN_GROUPS.flatMap((g) => g.keys));
    for (const key of Object.keys(CYRENE_SEMANTIC_TOKENS.light)) {
      expect(
        groupedKeys.has(key),
        `Semantic token ${key} should belong to a group in TOKEN_GROUPS`,
      ).toBe(true);
    }
  });

  it("satisfies WCAG AA contrast for text-primary on bg-app in light and dark modes", () => {
    // Light mode text-primary (#292536) on bg-app (#F8F6FD)
    const lightRatio = contrastRatio(
      CYRENE_SEMANTIC_TOKENS.light["--text-primary"],
      CYRENE_SEMANTIC_TOKENS.light["--bg-app"],
    );
    expect(lightRatio).toBeGreaterThanOrEqual(4.5);

    // Dark mode text-primary (#F3EFF8) on bg-app (#171421)
    const darkRatio = contrastRatio(
      CYRENE_SEMANTIC_TOKENS.dark["--text-primary"],
      CYRENE_SEMANTIC_TOKENS.dark["--bg-app"],
    );
    expect(darkRatio).toBeGreaterThanOrEqual(4.5);
  });

  it("satisfies non-text contrast for the palette-derived focus outline in light and dark modes", () => {
    // --focus-outline resolves from the frozen palette: light = --accent-soft-fg,
    // dark = --accent-primary. Both must reach WCAG 1.4.11 non-text contrast (>= 3.0:1)
    // against the application canvas. No extra focus hue is introduced.
    const lightFocusRatio = contrastRatio(
      CYRENE_SEMANTIC_TOKENS.light["--accent-soft-fg"],
      CYRENE_SEMANTIC_TOKENS.light["--bg-app"],
    );
    expect(lightFocusRatio).toBeGreaterThanOrEqual(3.0);

    const darkFocusRatio = contrastRatio(
      CYRENE_SEMANTIC_TOKENS.dark["--accent-primary"],
      CYRENE_SEMANTIC_TOKENS.dark["--bg-app"],
    );
    expect(darkFocusRatio).toBeGreaterThanOrEqual(3.0);
  });
});
