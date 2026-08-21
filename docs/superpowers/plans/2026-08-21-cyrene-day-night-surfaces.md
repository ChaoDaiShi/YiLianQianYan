# Cyrene Day/Night Theme and Surface Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. The user explicitly prohibited subagents, so execute inline. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Expose only Cyrene light/dark appearances with a system-following mode, retain background-image upload, and make every page root a transparent canvas over the shell artwork.

**Architecture:** Extend the existing ThemeProvider with a persisted `ThemeMode` and derived `ColorScheme`; do not add another provider. Emit complete light/dark Cyrene semantic token maps through the existing DOM-variable pipeline, then remove opaque full-page backgrounds while retaining bounded translucent surfaces for readable content.

**Tech Stack:** React 18, TypeScript, Zustand-compatible existing frontend state, Vite, Vitest, Tailwind utility classes, CSS custom properties.

## Global Constraints

- Do not use subagents.
- Keep the existing ThemeProvider as the single theme system.
- Render only Cyrene light and Cyrene dark palettes.
- Expose exactly `system`, `light`, and `dark` appearance modes.
- Keep background image upload and removal.
- Hide alternate presets, custom CSS variables, import/export, blur, panel-opacity, and font-size controls.
- Do not modify backend, API, database, Store, SSE, Agent, approval, or security behavior.
- Do not add dependencies or generate new artwork.
- Do not apply full-viewport `backdrop-filter` or animated blur.
- Keep keyboard focus visible and support `prefers-reduced-motion`.

---

### Task 1: Add the Persisted Cyrene Appearance Mode

**Files:**
- Modify: `frontend/src/theme/types.ts`
- Modify: `frontend/src/theme/presets.ts`
- Modify: `frontend/src/theme/ThemeProvider.tsx`
- Modify: `frontend/src/theme/presets.test.ts`
- Modify: `frontend/src/theme/index.ts`

**Interfaces:**
- Produces: `ThemeMode = "system" | "light" | "dark"`.
- Produces: `ColorScheme = "light" | "dark"`.
- Produces: `resolveColorScheme(mode: ThemeMode, systemPrefersDark: boolean): ColorScheme`.
- Produces through `useTheme()`: `mode`, `resolvedScheme`, and `setMode(mode)`.
- Preserves: background image persistence and `normalizeStoredTheme(raw)`.

- [ ] **Step 1: Replace preset expectations with mode and migration tests**

Rewrite `frontend/src/theme/presets.test.ts` around the new public contract:

```ts
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
```

- [ ] **Step 2: Run the mode tests and verify RED**

Working directory: `frontend`

Run: `npm.cmd test -- --run src/theme/presets.test.ts`

Expected: FAIL because `ThemeConfig.mode`, `ThemeMode`, `ColorScheme`, and `resolveColorScheme` do not exist and legacy presets are still preserved.

- [ ] **Step 3: Add mode types and a single visible preset identity**

In `frontend/src/theme/types.ts`, add:

```ts
export type ThemeMode = "system" | "light" | "dark";
export type ColorScheme = Exclude<ThemeMode, "system">;

export type PresetId = "cyrene-ripple" | "custom";
```

Add `mode: ThemeMode` to `ThemeConfig`. Keep the other fields temporarily for storage compatibility.

In `frontend/src/theme/presets.ts`, retain one `DEFAULT_THEME` with:

```ts
export const CYRENE_LIGHT_COLORS: ThemeColors = {
  bg: "#F8F6FD",
  bg2: "#F2EEFA",
  panel: "rgba(255,255,255,0.52)",
  panel2: "rgba(255,255,255,0.72)",
  panelHover: "rgba(249,243,252,0.62)",
  text: "#292536",
  textMuted: "#696276",
  textFaint: "#9690A1",
  border: "rgba(86,72,117,0.12)",
  accent: "#ea91b9",
  accentFg: "#292536",
  inputBg: "rgba(255,255,255,0.78)",
  success: "#73b99a",
  warning: "#d9b866",
  danger: "#df7995",
  nav: "#29263a",
  navText: "#eeeaf8",
  info: "#84bcdc",
  focusRing: "#ea91b9",
  backdrop: "rgba(41,38,58,0.34)",
};

export const DEFAULT_THEME: ThemeConfig = {
  presetId: "cyrene-ripple",
  mode: "system",
  bgMode: "solid",
  colors: CYRENE_LIGHT_COLORS,
  blur: 0,
  brightness: 1,
  panelOpacity: 0.52,
  fontSize: 14,
  monoTitles: false,
  customVars: {},
};

export const PRESETS = { "cyrene-ripple": DEFAULT_THEME } as const;
```

Remove `PRESET_META` and alternate entries from the exported preset collection. Legacy names are handled as untyped storage strings in ThemeProvider, not as selectable presets.

- [ ] **Step 4: Normalize storage and derive the effective scheme**

In `frontend/src/theme/ThemeProvider.tsx`, export:

```ts
export function resolveColorScheme(
  mode: ThemeMode,
  systemPrefersDark: boolean,
): ColorScheme {
  if (mode === "system") return systemPrefersDark ? "dark" : "light";
  return mode;
}
```

Use these exact migration sets:

```ts
const LEGACY_DARK_PRESETS = new Set(["graphite-pro", "claude-dark", "terminal-green"]);
const VALID_MODES = new Set<ThemeMode>(["system", "light", "dark"]);
```

`normalizeStoredTheme(raw)` returns a canonical Cyrene config. It preserves only valid `bgMode === "image"` and `bgImageKey === "background"`; it resets colors, blur, brightness, panel opacity, font size, mono titles, and custom variables to `DEFAULT_THEME`. Explicit valid `raw.mode` wins. Without a mode, `cyrene-ripple` maps to light, known dark legacy presets map to dark, and everything else maps to system.

Add system preference state and listener cleanup:

```ts
const readSystemPrefersDark = () =>
  typeof window !== "undefined" &&
  typeof window.matchMedia === "function" &&
  window.matchMedia("(prefers-color-scheme: dark)").matches;

const [systemPrefersDark, setSystemPrefersDark] = useState(readSystemPrefersDark);

useEffect(() => {
  if (theme.mode !== "system" || typeof window.matchMedia !== "function") return;
  const media = window.matchMedia("(prefers-color-scheme: dark)");
  const handleChange = (event: MediaQueryListEvent) => setSystemPrefersDark(event.matches);
  setSystemPrefersDark(media.matches);
  media.addEventListener("change", handleChange);
  return () => media.removeEventListener("change", handleChange);
}, [theme.mode]);
```

Expose:

```ts
mode: theme.mode,
resolvedScheme,
setMode: (mode: ThemeMode) => setTheme((current) => ({ ...current, mode })),
```

`setBackgroundImage` must no longer change `presetId`; clearing an image returns `bgMode` to `solid`. `resetTheme` restores system mode and clears the image.

- [ ] **Step 5: Export the new types and run tests GREEN**

Export `ThemeMode` and `ColorScheme` through the existing `frontend/src/theme/index.ts` barrel.

Run: `npm.cmd test -- --run src/theme/presets.test.ts`

Expected: all Cyrene appearance mode tests PASS.

- [ ] **Step 6: Commit Task 1**

```powershell
git add -- frontend/src/theme/types.ts frontend/src/theme/presets.ts frontend/src/theme/ThemeProvider.tsx frontend/src/theme/presets.test.ts frontend/src/theme/index.ts
git commit -m "feat(ui): add cyrene appearance modes"
```

---

### Task 2: Emit Complete Light and Dark Cyrene Tokens

**Files:**
- Modify: `frontend/src/theme/presets.ts`
- Modify: `frontend/src/theme/applyTheme.ts`
- Modify: `frontend/src/theme/applyTheme.test.ts`
- Modify: `frontend/src/theme/ThemeProvider.tsx`
- Modify: `frontend/src/components/layout/AppShell.tsx`

**Interfaces:**
- Consumes: `ColorScheme` and `resolvedScheme` from Task 1.
- Produces: `CYRENE_SEMANTIC_TOKENS: Record<ColorScheme, Record<string, string>>`.
- Produces: `buildThemeVariables(theme, scheme)` and `applyThemeToDom(theme, scheme, bgImageUrl)`.
- Produces DOM state: `data-theme="cyrene-ripple"`, `data-color-scheme="light|dark"`, and matching compatibility class.

- [ ] **Step 1: Rewrite token tests for two complete palettes**

In `frontend/src/theme/applyTheme.test.ts`, replace alternate-preset/custom-variable tests with:

```ts
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

  it.each(["light", "dark"] as const)("keeps readable status foregrounds in %s", (scheme) => {
    const variables = buildThemeVariables(DEFAULT_THEME, scheme);
    for (const key of ["--success-fg", "--warning-fg", "--danger-fg", "--info-fg"]) {
      expect(variables[key]).toBeTruthy();
      expect(variables[key]).not.toBe(variables[`--${key.slice(2, -3)}`]);
    }
  });
});
```

- [ ] **Step 2: Run token tests and verify RED**

Working directory: `frontend`

Run: `npm.cmd test -- --run src/theme/applyTheme.test.ts`

Expected: FAIL because token functions do not accept a scheme and dark/shell/code tokens do not exist.

- [ ] **Step 3: Define explicit light and dark semantic maps**

In `frontend/src/theme/presets.ts`, replace the single semantic map with two complete maps. Use these required anchors:

```ts
const CYRENE_SHARED_TOKENS = {
  "--radius-xs": "6px",
  "--radius-sm": "10px",
  "--radius-md": "12px",
  "--radius-lg": "16px",
  "--radius-xl": "20px",
  "--motion-fast": "140ms",
  "--motion-normal": "220ms",
} as const;

export const CYRENE_SEMANTIC_TOKENS: Record<ColorScheme, Record<string, string>> = {
  light: {
    ...CYRENE_SHARED_TOKENS,
    "--bg-app": "#F8F6FD",
    "--bg-soft": "#F2EEFA",
    "--bg-subtle": "#F2EEFA",
    "--surface": "rgba(255,255,255,0.42)",
    "--surface-muted": "rgba(252,249,255,0.24)",
    "--surface-elevated": "rgba(255,255,255,0.58)",
    "--surface-solid": "#FFFDFF",
    "--surface-hover": "rgba(249,243,252,0.62)",
    "--titlebar-bg": "rgba(255,255,255,0.32)",
    "--text-primary": "#292536",
    "--text-secondary": "#696276",
    "--text-faint": "#9690A1",
    "--text-disabled": "#BDB8C5",
    "--accent-primary": "#ea91b9",
    "--accent-primary-hover": "#DF7EAA",
    "--accent-contrast": "#292536",
    "--accent-soft": "#F9DCE9",
    "--accent-purple": "#AD9BE8",
    "--accent-blue": "#99CFEA",
    "--accent-gold": "#EBCF8C",
    "--accent-border": "rgba(234,145,185,0.26)",
    "--focus-ring-soft": "rgba(234,145,185,0.30)",
    "--divider": "rgba(91,76,125,0.11)",
    "--border-soft": "rgba(86,72,117,0.12)",
    "--success-soft": "rgba(115,185,154,0.14)",
    "--success-border": "rgba(115,185,154,0.34)",
    "--success-fg": "#315D49",
    "--warning-soft": "rgba(217,184,102,0.16)",
    "--warning-border": "rgba(217,184,102,0.36)",
    "--warning-fg": "#72571D",
    "--danger-soft": "rgba(223,121,149,0.14)",
    "--danger-border": "rgba(223,121,149,0.34)",
    "--danger-fg": "#873F59",
    "--info-soft": "rgba(132,188,220,0.15)",
    "--info-border": "rgba(132,188,220,0.34)",
    "--info-fg": "#37677E",
    "--accent-soft-fg": "#67435A",
    "--sidebar-bg": "#29263A",
    "--sidebar-bg-2": "#312C46",
    "--sidebar-text": "#EEEAF8",
    "--sidebar-fg": "#EEEAF8",
    "--sidebar-muted": "#AAA3BA",
    "--sidebar-active": "rgba(234,145,185,0.17)",
    "--cyrene-glow-pink": "rgba(234,145,185,0.22)",
    "--cyrene-glow-purple": "rgba(173,155,232,0.18)",
    "--cyrene-glow-blue": "rgba(153,207,234,0.14)",
    "--cyrene-glass-border": "rgba(255,255,255,0.52)",
    "--cyrene-surface-tint": "rgba(255,255,255,0.34)",
    "--cyrene-night": "#29263A",
    "--cyrene-water": "rgba(153,207,234,0.18)",
    "--cyrene-ripple": "rgba(173,155,232,0.18)",
    "--shadow-card": "0 4px 16px rgba(54,45,79,0.05)",
    "--shadow-float": "0 12px 36px rgba(48,38,76,0.11)",
    "--shell-art-opacity": "0.72",
    "--shell-overlay": "rgba(248,246,253,0.12)",
    "--page-wash": "rgba(248,246,253,0.18)",
    "--code-bg": "#252132",
    "--code-text": "#F4F0F8",
  },
  dark: {
    ...CYRENE_SHARED_TOKENS,
    "--bg-app": "#171421",
    "--bg-soft": "#1D1929",
    "--bg-subtle": "#1D1929",
    "--surface": "rgba(37,31,53,0.54)",
    "--surface-muted": "rgba(45,38,63,0.42)",
    "--surface-elevated": "rgba(49,41,68,0.68)",
    "--surface-solid": "#252032",
    "--surface-hover": "rgba(67,55,88,0.72)",
    "--titlebar-bg": "rgba(29,25,41,0.72)",
    "--text-primary": "#F3EFF8",
    "--text-secondary": "#C7BED2",
    "--text-faint": "#91879E",
    "--text-disabled": "#6E6678",
    "--accent-primary": "#F0A3C6",
    "--accent-primary-hover": "#F5B3D0",
    "--accent-contrast": "#241D2C",
    "--accent-soft": "rgba(240,163,198,0.18)",
    "--accent-purple": "#B8A7F0",
    "--accent-blue": "#8CC9E8",
    "--accent-gold": "#E5C875",
    "--accent-border": "rgba(240,163,198,0.30)",
    "--focus-ring-soft": "rgba(240,163,198,0.32)",
    "--divider": "rgba(220,210,235,0.12)",
    "--border-soft": "rgba(220,210,235,0.14)",
    "--success-soft": "rgba(116,192,157,0.16)",
    "--success-border": "rgba(116,192,157,0.36)",
    "--success-fg": "#A8E0C5",
    "--warning-soft": "rgba(226,191,103,0.17)",
    "--warning-border": "rgba(226,191,103,0.38)",
    "--warning-fg": "#F2D993",
    "--danger-soft": "rgba(232,126,157,0.17)",
    "--danger-border": "rgba(232,126,157,0.38)",
    "--danger-fg": "#F4B0C3",
    "--info-soft": "rgba(132,197,229,0.16)",
    "--info-border": "rgba(132,197,229,0.36)",
    "--info-fg": "#B5DDF1",
    "--accent-soft-fg": "#F3C4D9",
    "--sidebar-bg": "#1D1929",
    "--sidebar-bg-2": "#252035",
    "--sidebar-text": "#F0EBF7",
    "--sidebar-fg": "#F0EBF7",
    "--sidebar-muted": "#AAA1B7",
    "--sidebar-active": "rgba(240,163,198,0.18)",
    "--cyrene-glow-pink": "rgba(240,163,198,0.18)",
    "--cyrene-glow-purple": "rgba(184,167,240,0.16)",
    "--cyrene-glow-blue": "rgba(140,201,232,0.12)",
    "--cyrene-glass-border": "rgba(239,232,248,0.15)",
    "--cyrene-surface-tint": "rgba(60,50,80,0.30)",
    "--cyrene-night": "#171421",
    "--cyrene-water": "rgba(140,201,232,0.14)",
    "--cyrene-ripple": "rgba(184,167,240,0.16)",
    "--shadow-card": "0 5px 18px rgba(5,4,10,0.20)",
    "--shadow-float": "0 14px 40px rgba(5,4,10,0.32)",
    "--shell-art-opacity": "0.26",
    "--shell-overlay": "rgba(17,14,27,0.66)",
    "--page-wash": "rgba(23,20,33,0.28)",
    "--code-bg": "#11101A",
    "--code-text": "#EEEAF5",
  },
};
```

Add the six new names to `PROTECTED_THEME_VAR_NAMES` in `frontend/src/theme/types.ts`.

- [ ] **Step 4: Make DOM application scheme-aware**

Change signatures in `frontend/src/theme/applyTheme.ts`:

```ts
export function buildThemeVariables(
  theme: ThemeConfig,
  scheme: ColorScheme,
): Record<string, string>

export function applyThemeToDom(
  theme: ThemeConfig,
  scheme: ColorScheme,
  bgImageUrl?: string | null,
): void
```

Build legacy variables from the selected semantic map, then return both maps. Stop deriving semantic colors from old presets or custom variables. Set:

```ts
export function buildThemeVariables(
  theme: ThemeConfig,
  scheme: ColorScheme,
): Record<string, string> {
  const semantic = CYRENE_SEMANTIC_TOKENS[scheme];
  const legacy = {
    "--bg": semantic["--bg-app"],
    "--bg-2": semantic["--bg-soft"],
    "--panel": semantic["--surface"],
    "--panel-2": semantic["--surface-elevated"],
    "--panel-hover": semantic["--surface-hover"],
    "--text": semantic["--text-primary"],
    "--text-muted": semantic["--text-secondary"],
    "--text-faint": semantic["--text-faint"],
    "--border": semantic["--border-soft"],
    "--accent": semantic["--accent-primary"],
    "--accent-fg": semantic["--accent-contrast"],
    "--input-bg": semantic["--surface-solid"],
    "--success": scheme === "dark" ? "#74C09D" : "#73B99A",
    "--warning": scheme === "dark" ? "#E2BF67" : "#D9B866",
    "--danger": scheme === "dark" ? "#E87E9D" : "#DF7995",
    "--nav": semantic["--sidebar-bg"],
    "--nav-text": semantic["--sidebar-text"],
    "--info": scheme === "dark" ? "#84C5E5" : "#84BCDC",
    "--focus-ring": semantic["--accent-primary"],
    "--backdrop": scheme === "dark" ? "rgba(5,4,10,0.58)" : "rgba(41,38,58,0.34)",
    "--font-size-base": `${theme.fontSize}px`,
    "--bg-blur": "0px",
    "--bg-brightness": "1",
  };
  return { ...legacy, ...semantic };
}

root.dataset.theme = "cyrene-ripple";
root.dataset.colorScheme = scheme;
root.classList.toggle("dark", scheme === "dark");
root.classList.toggle("light", scheme === "light");
```

ThemeProvider calls `applyThemeToDom(theme, resolvedScheme, bgImageUrl)` and includes `resolvedScheme` in the effect dependencies.

- [ ] **Step 5: Route shell artwork through semantic tokens**

In `frontend/src/components/layout/AppShell.tsx`, consume `resolvedScheme`, add `data-color-scheme={resolvedScheme}` to `.app-shell`, and remove runtime background blur. The background layer uses:

```tsx
style={{
  backgroundColor: "var(--bg-app)",
  backgroundImage: "var(--bg-image)",
  backgroundSize: theme.bgMode === "image" ? "cover" : undefined,
  backgroundPosition: "center",
}}
```

The image readability layer uses `backgroundColor: "var(--shell-overlay)"` and is visible only for uploaded images. CSS controls ambient artwork opacity with `--shell-art-opacity`.

- [ ] **Step 6: Run theme tests and build-type check GREEN**

Run: `npm.cmd test -- --run src/theme/applyTheme.test.ts src/theme/presets.test.ts`

Run: `npm.cmd run build`

Expected: tests PASS and TypeScript/Vite build PASS.

- [ ] **Step 7: Commit Task 2**

```powershell
git add -- frontend/src/theme/types.ts frontend/src/theme/presets.ts frontend/src/theme/applyTheme.ts frontend/src/theme/applyTheme.test.ts frontend/src/theme/ThemeProvider.tsx frontend/src/components/layout/AppShell.tsx
git commit -m "feat(ui): add cyrene light and dark tokens"
```

---

### Task 3: Reduce Appearance Settings to Three Modes and Background Image

**Files:**
- Modify: `frontend/src/pages/SettingsPage.tsx`
- Modify: `frontend/src/pages/systemSettingsContract.test.ts`

**Interfaces:**
- Consumes: `mode`, `resolvedScheme`, `setMode`, `bgImageUrl`, `setBackgroundImage`, and `resetTheme` from ThemeProvider.
- Produces: accessible system/light/dark controls and background upload/removal.
- Removes from rendered UI: alternate preset cards and all advanced theme controls.

- [ ] **Step 1: Add failing appearance contract tests**

Append to `frontend/src/pages/systemSettingsContract.test.ts`:

```ts
it("exposes only Cyrene system light and dark appearance choices", () => {
  expect(source).toContain("跟随系统");
  expect(source).toContain("白天");
  expect(source).toContain("夜间");
  expect(source).toContain("theme.setMode");
  expect(source).toContain('role="radiogroup"');
  expect(source).not.toContain("暖色本地");
  expect(source).not.toContain("精密中性");
  expect(source).not.toContain("石墨专业");
  expect(source).not.toContain("高对比");
});

it("keeps background images but hides advanced theme editing", () => {
  expect(source).toContain("上传背景图");
  expect(source).toContain("清除背景");
  expect(source).not.toContain("自定义 CSS 变量");
  expect(source).not.toContain("导出主题");
  expect(source).not.toContain("导入主题");
  expect(source).not.toContain("面板透明度");
  expect(source).not.toContain(">模糊<");
  expect(source).not.toContain(">字号<");
});
```

- [ ] **Step 2: Run Settings contract and verify RED**

Run: `npm.cmd test -- --run src/pages/systemSettingsContract.test.ts`

Expected: FAIL because old preset and advanced controls are still rendered.

- [ ] **Step 3: Replace the appearance section**

Remove `PRESET_META`, `PresetId`, `Download`, `handleExport`, and `handleImport` from `SettingsPage.tsx`.

Define:

```ts
const APPEARANCE_MODES = [
  { id: "system", label: "跟随系统", description: "自动使用系统的明暗外观" },
  { id: "light", label: "白天", description: "清透的月光白与淡粉紫界面" },
  { id: "dark", label: "夜间", description: "低眩光的深紫夜色界面" },
] as const;
```

Render a `role="radiogroup"` containing three buttons. Each button uses `role="radio"`, `aria-checked={theme.mode === item.id}`, and `onClick={() => theme.setMode(item.id)}`. Show `当前跟随系统：白天/夜间` only when mode is system. Keep upload, clear, and reset controls. Do not render sliders, custom JSON, import, or export.

- [ ] **Step 4: Run Settings and full page-contract tests GREEN**

Run: `npm.cmd test -- --run src/pages/systemSettingsContract.test.ts src/pages/systemCenterFreezeContract.test.ts`

Expected: both test files PASS.

- [ ] **Step 5: Commit Task 3**

```powershell
git add -- frontend/src/pages/SettingsPage.tsx frontend/src/pages/systemSettingsContract.test.ts
git commit -m "refactor(ui): simplify cyrene appearance settings"
```

---

### Task 4: Enforce Transparent Page Canvas and Bounded Surfaces

**Files:**
- Modify: `frontend/src/index.css`
- Modify: `frontend/src/components/layout/AppShell.tsx`
- Modify: `frontend/src/pages/AgentsPage.tsx`
- Modify: `frontend/src/pages/CapabilitiesPage.tsx`
- Modify: `frontend/src/pages/ChatPage.tsx`
- Modify: `frontend/src/pages/KnowledgePage.tsx`
- Modify: `frontend/src/pages/LogsPage.tsx`
- Modify: `frontend/src/pages/MemoryPage.tsx`
- Modify: `frontend/src/pages/PluginsPage.tsx`
- Modify: `frontend/src/pages/SettingsPage.tsx`
- Modify: `frontend/src/pages/SkillsPage.tsx`
- Modify: `frontend/src/pages/SystemPage.tsx`
- Modify: `frontend/src/pages/TaskCenterPage.tsx`
- Modify: `frontend/src/pages/WorkflowsPage.tsx`
- Modify: `frontend/src/pages/WorkspaceDetailPage.tsx`
- Modify: `frontend/src/pages/WorkspacesPage.tsx`
- Create: `frontend/src/theme/surfaceContract.test.ts`
- Modify: `frontend/src/components/chat/conversationThemeVisual.test.ts`
- Modify: `frontend/src/components/layout/shellRailsVisual.test.ts`

**Interfaces:**
- Consumes: semantic surface and shell tokens from Task 2.
- Produces CSS contract: AppShell owns the canvas; page roots are transparent; bounded components use surface tokens.
- Preserves: workflow grid, conversation atmosphere, background artwork, and reduced-motion behavior.

- [ ] **Step 1: Add failing canvas ownership tests**

Create `frontend/src/theme/surfaceContract.test.ts`:

```ts
import { describe, expect, it } from "vitest";
// @ts-expect-error -- Node file access is test-only and not bundled.
import { readFileSync } from "node:fs";
import appShellSource from "../components/layout/AppShell.tsx?raw";
import agentsSource from "../pages/AgentsPage.tsx?raw";
import capabilitiesSource from "../pages/CapabilitiesPage.tsx?raw";
import chatSource from "../pages/ChatPage.tsx?raw";
import knowledgeSource from "../pages/KnowledgePage.tsx?raw";
import logsSource from "../pages/LogsPage.tsx?raw";
import memorySource from "../pages/MemoryPage.tsx?raw";
import pluginsSource from "../pages/PluginsPage.tsx?raw";
import settingsSource from "../pages/SettingsPage.tsx?raw";
import skillsSource from "../pages/SkillsPage.tsx?raw";
import systemSource from "../pages/SystemPage.tsx?raw";
import tasksSource from "../pages/TaskCenterPage.tsx?raw";
import workflowsSource from "../pages/WorkflowsPage.tsx?raw";
import workspaceDetailSource from "../pages/WorkspaceDetailPage.tsx?raw";
import workspacesSource from "../pages/WorkspacesPage.tsx?raw";

const css = readFileSync(new URL("../index.css", import.meta.url), "utf8");

function rule(selector: string): string {
  const escaped = selector.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  return css.match(new RegExp(`${escaped}\\s*\\{([\\s\\S]*?)\\}`))?.[1] ?? "";
}

describe("Cyrene canvas and surface contract", () => {
  it("keeps AppShell as the single canvas owner", () => {
    expect(appShellSource).toContain("shell-ambient");
    expect(appShellSource).toContain("var(--bg-app)");
    expect(appShellSource).toContain("var(--shell-overlay)");
  });

  it.each([
    agentsSource,
    capabilitiesSource,
    chatSource,
    knowledgeSource,
    logsSource,
    memorySource,
    pluginsSource,
    settingsSource,
    skillsSource,
    systemSource,
    tasksSource,
    workflowsSource,
    workspaceDetailSource,
    workspacesSource,
  ])("marks every business page as a transparent page canvas", (source) => {
    expect(source).toContain("page-canvas");
  });

  it("defines page canvas without an opaque fill", () => {
    const body = rule(".page-canvas");
    expect(body).toContain("background: transparent");
    expect(body).not.toContain("var(--bg-app)");
    expect(body).not.toContain("var(--surface-solid)");
  });

  it("keeps the workflow grid without an opaque canvas color", () => {
    const body = rule(".workflow-canvas");
    expect(body).toContain("background-image");
    expect(body).not.toContain("background-color: var(--bg-app)");
  });

  it("uses a translucent conversation wash", () => {
    const body = rule('.workbench-grid[data-chat-view="conversation"]');
    expect(body).toContain("var(--page-wash)");
    expect(body).not.toMatch(/rgba\([^)]*,\s*0\.9[2-9]\)/);
  });

  it("keeps full viewport blur disabled", () => {
    expect(rule(".workbench-grid")).not.toContain("backdrop-filter");
    expect(rule(".app-shell")).not.toContain("backdrop-filter");
  });
});
```

- [ ] **Step 2: Run surface contracts and verify RED**

Run: `npm.cmd test -- --run src/theme/surfaceContract.test.ts`

Expected: FAIL on the opaque memory background, workflow canvas, near-opaque conversation wash, and full-grid backdrop filter.

- [ ] **Step 3: Make the shell and page canvas transparent**

In `frontend/src/index.css`:

- Keep `body` and the AppShell background layer on `--bg-app`.
- Set `.app-shell`, `.workspace-region`, `.memory-page`, `.memory-page-body`, `.knowledge-page`, `.knowledge-page-body`, `.capability-page`, `.system-center-page`, and `.system-settings-page` to transparent backgrounds.
- Add `.page-canvas { min-width: 0; min-height: 0; background: transparent; }`.
- Change `.shell-ambient-strong` opacity to `var(--shell-art-opacity)`.
- Change `.shell-ambient-overlay` to `background: var(--shell-overlay)` plus a subtle radial highlight.
- Remove `backdrop-filter` and `-webkit-backdrop-filter` from `.workbench-grid`.
- Change `.memory-page-body` to a low-opacity radial gradient ending in `transparent`.
- Change `.workflow-canvas` to `background-color: color-mix(in srgb, var(--page-wash) 72%, transparent)` while retaining its grid images.
- Change the active conversation root to radial accents over `var(--page-wash)`, with no hardcoded opacity above `0.80`.

Do not remove borders or surface backgrounds from PageHeader, Panel, cards, dialogs, inputs, code blocks, or list/detail panes.

Add `page-canvas` to each outer page root using these exact class changes:

```text
AgentsPage:              "capability-page page-canvas"
CapabilitiesPage:        "capability-page page-canvas"
ChatPage:                `${WORKBENCH_VIEWPORT_CLASS_NAME} page-canvas`
KnowledgePage:           "knowledge-page page-canvas"
LogsPage:                "system-center-page page-canvas flex h-full min-h-0 flex-col"
MemoryPage:              "memory-page page-canvas"
PluginsPage loading:     "capability-loading-page page-canvas"
PluginsPage main:        "page-canvas flex h-full flex-col"
SettingsPage:            "system-settings-page page-canvas flex h-full min-h-0 flex-col"
SkillsPage:              "capability-page skills-page page-canvas"
SystemPage:              "system-center-page page-canvas flex h-full min-h-0 flex-col"
TaskCenterPage:          "page-canvas flex h-full min-h-0 flex-col"
WorkflowsPage:           "page-canvas flex h-full flex-col"
WorkspaceDetailPage:     "page-canvas flex h-full min-h-0 flex-col"
WorkspacesPage:          "page-canvas flex h-full flex-col"
```

- [ ] **Step 4: Make bounded surfaces scheme-safe**

Replace light-only full-width and high-priority content declarations with semantic tokens:

```css
.glass-surface,
.workbench-grid .glass-header {
  background: linear-gradient(90deg, var(--surface-elevated), var(--surface-muted));
}

.conversation-message-user {
  background: color-mix(in srgb, var(--accent-purple) 14%, var(--surface-elevated)) !important;
}

.conversation-message-assistant,
.conversation-composer .composer-card {
  background: var(--surface-elevated) !important;
}

.prose pre,
.workflow-technical-details pre {
  background: var(--code-bg);
  color: var(--code-text);
}
```

Keep cards translucent enough to show a hint of the shell art but opaque enough for text. Use `--surface-solid` only for compact controls, menus, dialogs, and input fields.

- [ ] **Step 5: Update existing visual contracts**

In `conversationThemeVisual.test.ts`, assert `indexCssSource` contains `var(--page-wash)` and `var(--code-bg)`. In `shellRailsVisual.test.ts`, assert AppShell contains `data-color-scheme` and `var(--shell-overlay)`.

- [ ] **Step 6: Run visual and page contracts GREEN**

Run: `npm.cmd test -- --run src/theme/surfaceContract.test.ts src/components/chat/conversationThemeVisual.test.ts src/components/layout/shellRailsVisual.test.ts src/pages/memoryKnowledgeContract.test.ts`

Expected: all selected tests PASS.

- [ ] **Step 7: Commit Task 4**

```powershell
git add -- frontend/src/index.css frontend/src/components/layout/AppShell.tsx frontend/src/pages/AgentsPage.tsx frontend/src/pages/CapabilitiesPage.tsx frontend/src/pages/ChatPage.tsx frontend/src/pages/KnowledgePage.tsx frontend/src/pages/LogsPage.tsx frontend/src/pages/MemoryPage.tsx frontend/src/pages/PluginsPage.tsx frontend/src/pages/SettingsPage.tsx frontend/src/pages/SkillsPage.tsx frontend/src/pages/SystemPage.tsx frontend/src/pages/TaskCenterPage.tsx frontend/src/pages/WorkflowsPage.tsx frontend/src/pages/WorkspaceDetailPage.tsx frontend/src/pages/WorkspacesPage.tsx frontend/src/theme/surfaceContract.test.ts frontend/src/components/chat/conversationThemeVisual.test.ts frontend/src/components/layout/shellRailsVisual.test.ts
git commit -m "style(ui): reveal cyrene canvas through page surfaces"
```

---

### Task 5: Regression, Responsive Inspection, and Build

**Files:**
- Verify only unless a regression requires a scoped correction in a file already listed above.

**Interfaces:**
- Confirms the theme foundation without changing business behavior.

- [ ] **Step 1: Run focused theme and visual tests**

Working directory: `frontend`

Run: `npm.cmd test -- --run src/theme/presets.test.ts src/theme/applyTheme.test.ts src/theme/surfaceContract.test.ts src/pages/systemSettingsContract.test.ts`

Expected: all selected tests PASS.

- [ ] **Step 2: Run the complete frontend suite**

Run: `npm.cmd test -- --run`

Expected: all frontend tests PASS with no removed business-behavior coverage.

- [ ] **Step 3: Run the production build**

Run: `npm.cmd run build`

Expected: TypeScript and Vite build PASS with no new warning.

- [ ] **Step 4: Inspect the three target viewport sizes**

Run the existing frontend with its normal development command and inspect 1920×1080, 1600×900, and 1366×768 in light and dark. Confirm:

- no horizontal scrollbar;
- shell artwork remains visible but subdued;
- page roots do not look like opaque white or black sheets;
- cards, tables, forms, Markdown, code, approval, success, warning, and error states remain readable;
- navigation remains legible in both schemes;
- uploaded-background overlay preserves readability;
- following-system mode reacts to a simulated scheme change.

- [ ] **Step 5: Verify repository hygiene**

Run: `git diff --check`

Run: `git status --short --branch`

Expected: no whitespace errors and no uncommitted implementation changes after task commits.

Report any remaining page-specific visual weakness as a follow-up rather than expanding this shared-theme pass into a full page redesign.
