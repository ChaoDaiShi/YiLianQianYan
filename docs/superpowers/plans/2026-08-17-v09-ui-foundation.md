# YiLianQianYan v0.9 UI Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Upgrade the existing React/Vite theme and shared UI foundation to the approved `Cyrene Ripple / 昔涟·涟漪` visual system while preserving existing routes, runtime behavior, theme migration, and SSE semantics.

**Architecture:** Extend the existing `ThemeConfig`/`ThemeProvider`/preset architecture. Add a `cyrene-ripple` preset, keep legacy CSS variables as compatibility aliases, and expose semantic v0.9 variables from the existing DOM application point. Keep shared components presentational and migrate their classes to semantic tokens; represent NavRail grouping as pure data so route coverage is testable without introducing a new navigation runtime.

**Tech Stack:** React 18, TypeScript 5, Vite 5, Tailwind CSS 3, Zustand, lucide-react, Vitest 2.

## Global Constraints

- Do not modify backend, Agent loop, tool registry, API endpoints, route paths, or SSE event semantics.
- Preserve existing theme import/export, background-image persistence, custom variable sanitization, and legacy preset migration.
- Use the approved `Cyrene Ripple / 昔涟·涟漪` values from the design spec; do not introduce a second theme engine.
- Use `npm.cmd` on Windows and run `npm.cmd test` plus `npm.cmd run build` before delivery.
- No voice/TTS, Live2D, homepage redesign, composer relocation, or broad business-page refactor in this phase.
- Apply TDD to new pure behavior: write a failing test, observe the expected failure, implement minimally, then rerun the focused and full suites.

---

### Task 1: Add the Cyrene Ripple preset and preserve theme migrations

**Files:**
- Modify: `frontend/src/theme/types.ts`
- Modify: `frontend/src/theme/presets.ts`
- Modify: `frontend/src/theme/ThemeProvider.tsx`
- Test: `frontend/src/theme/presets.test.ts`

**Interfaces:**
- Produces `PresetId = "cyrene-ripple"` and `PRESETS["cyrene-ripple"]` for later DOM application and Settings display.
- Preserves `normalizeStoredTheme(raw)` behavior for `warm-local`, `precision-neutral`, `graphite-pro`, `high-contrast`, `custom`, and existing legacy aliases.

- [ ] **Step 1: Write failing preset tests**

Extend `frontend/src/theme/presets.test.ts` with tests that assert the new default and Settings metadata without removing legacy coverage. Update the import to include `PRESET_META`:

```ts
import { DEFAULT_THEME, PRESETS, PRESET_META } from "./presets";

it("uses cyrene-ripple as the default for new installations", () => {
  expect(DEFAULT_THEME.presetId).toBe("cyrene-ripple");
  expect(DEFAULT_THEME.colors.bg).toBe("#f9f7ff");
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

it("normalizes an unknown stored preset to cyrene-ripple", () => {
  expect(normalizeStoredTheme({ presetId: "missing-preset" }).presetId).toBe(
    "cyrene-ripple",
  );
});
```

- [ ] **Step 2: Run the focused test and verify it fails for the missing preset**

Run from `frontend`:

```powershell
npm.cmd test -- src/theme/presets.test.ts
```

Expected: FAIL because `DEFAULT_THEME` still points to `warm-local`, the preset key is absent, and the expected preset list does not match.

- [ ] **Step 3: Implement the preset and migration-safe default**

Add `"cyrene-ripple"` to `PresetId`. Add a complete preset before the existing presets in `PRESETS` using the approved colors, with `bgMode: "solid"`, `blur: 0`, `brightness: 1`, `panelOpacity: 0.82`, `fontSize: 14`, `monoTitles: false`, and an empty `customVars` object. Use the existing `makeColors` shape so all legacy fields are populated. Set `DEFAULT_THEME = PRESETS["cyrene-ripple"]` and add `{ id: "cyrene-ripple", name: "昔涟 · 涟漪", description: "轻盈、清澈的默认工作台" }` as the first `PRESET_META` entry. Keep `LEGACY_PRESET_MAP` unchanged so current saved IDs still migrate to their current targets; unknown IDs will now normalize to the new default automatically.

- [ ] **Step 4: Run focused tests and then the full frontend suite**

Run:

```powershell
npm.cmd test -- src/theme/presets.test.ts
npm.cmd test
```

Expected: the focused tests and all existing tests pass with zero failures.

- [ ] **Step 5: Commit the preset boundary**

```powershell
git add frontend/src/theme/types.ts frontend/src/theme/presets.ts frontend/src/theme/ThemeProvider.tsx frontend/src/theme/presets.test.ts
git commit -m "feat(theme): add cyrene ripple preset"
```

### Task 2: Add semantic v0.9 tokens at the existing DOM application boundary

**Files:**
- Modify: `frontend/src/theme/types.ts`
- Modify: `frontend/src/theme/applyTheme.ts`
- Modify: `frontend/src/theme/presets.ts`
- Create: `frontend/src/theme/applyTheme.test.ts`

**Interfaces:**
- Produces `buildThemeVariables(theme: ThemeConfig): Record<string, string>` as a pure, testable mapping used by `applyThemeToDom`.
- `applyThemeToDom` continues to set legacy variables and additionally sets semantic color/surface variables such as `--bg-app`, `--surface`, `--accent-primary`, and `--sidebar-bg`. Shared geometry, shadow, and motion variables are added in Task 3.

- [ ] **Step 1: Write failing pure token mapping tests**

Create `frontend/src/theme/applyTheme.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { DEFAULT_THEME } from "./presets";
import { buildThemeVariables } from "./applyTheme";

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
```

- [ ] **Step 2: Run the new test and verify the missing export failure**

```powershell
npm.cmd test -- src/theme/applyTheme.test.ts
```

Expected: FAIL because `buildThemeVariables` does not exist yet.

- [ ] **Step 3: Implement the pure mapping and DOM application**

In `applyTheme.ts`, export `buildThemeVariables`. Build a record containing every legacy variable currently set and the semantic v0.9 color/surface values. Set semantic defaults from the Cyrene preset values while deriving legacy values from `theme.colors`; this keeps imported non-Cyrene themes functional. Apply only whitelisted custom variables after the base mapping, and do not add semantic names to `THEME_VAR_WHITELIST` in this phase, so user customizations cannot override protected semantic aliases.

Refactor `applyThemeToDom` to iterate the mapping and retain its existing background-image, class, dataset, and light/dark compatibility behavior. Treat `cyrene-ripple` as a light theme for the compatibility class.

- [ ] **Step 4: Run focused and full tests**

```powershell
npm.cmd test -- src/theme/applyTheme.test.ts src/theme/presets.test.ts
npm.cmd test
```

Expected: all tests pass.

- [ ] **Step 5: Commit semantic token support**

```powershell
git add frontend/src/theme/types.ts frontend/src/theme/applyTheme.ts frontend/src/theme/applyTheme.test.ts frontend/src/theme/presets.ts
git commit -m "feat(theme): expose cyrene semantic tokens"
```

### Task 3: Migrate global CSS and shared UI components to the foundation

**Files:**
- Modify: `frontend/src/index.css`
- Modify: `frontend/src/components/ui/Button.tsx`
- Modify: `frontend/src/components/ui/Input.tsx`
- Modify: `frontend/src/components/ui/Textarea.tsx`
- Modify: `frontend/src/components/ui/Panel.tsx`
- Modify: `frontend/src/components/ui/Badge.tsx`
- Modify: `frontend/src/components/ui/Modal.tsx`
- Modify: `frontend/src/components/ui/Drawer.tsx`
- Modify: `frontend/src/components/ui/PageHeader.tsx`
- Modify: `frontend/src/components/ui/EmptyState.tsx`
- Modify: `frontend/src/components/ui/Spinner.tsx`

**Interfaces:**
- Keeps every existing component export and prop shape backward compatible.
- Uses semantic CSS variables for new styling and legacy aliases only where an existing page still depends on them.

- [ ] **Step 1: Add a CSS contract test fixture to the existing token test**

Extend `applyTheme.test.ts` with an assertion that the mapping contains the shared component contract:

```ts
it("contains the shared component geometry and motion contract", () => {
  const variables = buildThemeVariables(DEFAULT_THEME);
  expect(variables["--radius-sm"]).toBe("10px");
  expect(variables["--radius-lg"]).toBe("16px");
  expect(variables["--shadow-card"]).toContain("rgba");
  expect(variables["--motion-fast"]).toBe("140ms");
});
```

- [ ] **Step 2: Run the fixture test before changing CSS**

```powershell
npm.cmd test -- src/theme/applyTheme.test.ts
```

Expected: FAIL because the geometry and motion variables are not yet present in the mapping.

- [ ] **Step 3: Implement the global token and component migration**

Update `index.css` to define fallback semantic variables for the first paint, then use the same variables for body background/text, surface-elevated, focus rings, selection, scrollbar, Markdown surfaces, and workbench backgrounds. Keep the existing `prefers-reduced-motion` block. Replace hard-coded `black/60`, `shadow-2xl`, and old radius references in shared components with `--backdrop`, `--shadow-float`, and the approved radius tokens. Preserve Modal/Drawer keyboard behavior and existing accessible labels. Use `transition-colors`/`transition-[...]` with the token durations; do not add continuous particle or blur animation.

Use the following component style rules:

```text
Button primary: --accent-primary / --accent-primary-hover / --accent-fg
Button secondary: --surface-solid + --border-soft + --surface-hover
Button ghost: transparent + --text-secondary, hover --surface-hover
Input/Textarea: --surface-solid + --border-soft, focus --accent-primary
Panel/PageHeader: --surface + --border-soft + --shadow-card
Badge: --accent-soft and semantic state backgrounds with low opacity
Modal/Drawer: --backdrop + --surface-solid + --shadow-float
Spinner: --border-soft + --accent-primary
```

- [ ] **Step 4: Run focused tests, build, and inspect the diff**

```powershell
npm.cmd test -- src/theme/applyTheme.test.ts
npm.cmd run build
git diff --check
```

Expected: tests and build pass; `git diff --check` produces no output.

- [ ] **Step 5: Commit the shared foundation migration**

```powershell
git add frontend/src/index.css frontend/src/components/ui
git commit -m "feat(ui): apply cyrene foundation to shared components"
```

### Task 4: Group NavRail visually without changing navigation behavior

**Files:**
- Modify: `frontend/src/components/layout/NavRail.tsx`
- Create: `frontend/src/components/layout/navGroups.ts`
- Create: `frontend/src/components/layout/navGroups.test.ts`

**Interfaces:**
- Produces `NAV_GROUPS`, a readonly grouped navigation model consumed by `NavRail`.
- Each visible item retains the existing `to`, `id`, `label`, and icon component; no new route is introduced.

- [ ] **Step 1: Write the failing grouping coverage test**

Create `navGroups.test.ts` with the current route set:

```ts
import { describe, expect, it } from "vitest";
import { NAV_GROUPS } from "./navGroups";

describe("NavRail groups", () => {
  it("keeps every current navigation route in one visual group", () => {
    const items = NAV_GROUPS.flatMap((group) => group.items);
    expect(items.map((item) => item.to)).toEqual([
      "/chat",
      "/workflows",
      "/workspaces",
      "/skills",
      "/plugins",
      "/agents",
      "/capabilities",
      "/knowledge",
      "/system",
      "/logs",
      "/settings",
    ]);
    expect(NAV_GROUPS.map((group) => group.label)).toEqual(["核心", "能力", "系统"]);
  });
});
```

- [ ] **Step 2: Run the test and verify the missing module failure**

```powershell
npm.cmd test -- src/components/layout/navGroups.test.ts
```

Expected: FAIL because `navGroups.ts` does not exist.

- [ ] **Step 3: Extract the current items into grouped data and render the groups**

Create `navGroups.ts` with the current Lucide icon imports and group the existing entries as `核心: chat, workflows, workspaces`, `能力: skills, plugins, agents, capabilities, knowledge`, and `系统: system, logs, settings`. Do not add the proposed `任务` or `记忆中心` routes because they are not currently registered in `App.tsx`. Export a readonly `NAV_GROUPS` value. In `NavRail.tsx`, replace the single `NAV_ITEMS.map` with nested group rendering. Add subdued group labels/dividers, keep the current active left highlight, change active colors to `--accent-primary`/`--sidebar-active`, and preserve backend health polling and labels.

- [ ] **Step 4: Run focused tests and production build**

```powershell
npm.cmd test -- src/components/layout/navGroups.test.ts
npm.cmd test
npm.cmd run build
```

Expected: all tests pass and the production build exits with code 0.

- [ ] **Step 5: Commit the grouped navigation**

```powershell
git add frontend/src/components/layout/NavRail.tsx frontend/src/components/layout/navGroups.ts frontend/src/components/layout/navGroups.test.ts
git commit -m "feat(ui): group navigation for cyrene theme"
```

### Task 5: Run final verification and visual smoke check

**Files:**
- No planned source changes.
- Review: `frontend/src/App.tsx`, `frontend/src/components/layout/AppShell.tsx`, `frontend/src/components/layout/NavRail.tsx`, `frontend/src/theme/*`.

- [ ] **Step 1: Run the complete frontend test suite**

```powershell
cd frontend
npm.cmd test
```

Expected: zero failed tests.

- [ ] **Step 2: Run the production build**

```powershell
npm.cmd run build
```

Expected: TypeScript compilation and Vite production build both exit successfully.

- [ ] **Step 3: Start a standalone Vite preview and exercise routes**

Use the existing frontend dev/preview command from the worktree and verify the built app loads the `/chat`, `/settings`, and `/logs` routes without a console crash. Confirm the default DOM root has `data-theme="cyrene-ripple"` on a fresh storage state and that navigation labels remain present. If a browser automation runtime is unavailable, record that limitation instead of claiming direct visual evidence.

- [ ] **Step 4: Verify scope and working tree**

```powershell
git diff develop...HEAD --stat
git diff develop...HEAD -- frontend/src/api frontend/src/pages frontend/src/features
git status --short --branch
```

Expected: the diff is limited to the theme foundation, shared UI, NavRail, tests, and documentation; no backend/API/page business logic changes are present; the worktree is clean.

- [ ] **Step 5: Request review and prepare the branch handoff**

Use the requesting-code-review skill against the final branch range. Resolve all critical/important findings, rerun the full test/build commands, and report the branch, commits, validation evidence, known visual limitations, and integration options. Do not merge into `develop` or `main` automatically.
