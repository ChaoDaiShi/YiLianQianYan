# Shell Global Background and Adaptive NavRail Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Apply the supplied Cyrene visual language as a translucent shell-wide background and make the NavRail progressively compact so all real navigation remains accessible at low window heights.

**Architecture:** Keep `AppShell` responsible for global background layers and `NavRail` responsible for height-aware presentation. Use CSS media queries and existing DOM structure; do not add runtime state, routing changes, or dependencies.

**Tech Stack:** React, TypeScript, Tailwind utilities, `index.css`, Vitest, Vite, Tauri WebView screenshots.

## Global Constraints

- Presentation and layout only; backend, API, store semantics, runtime, persistence, and security remain unchanged.
- Preserve every existing NavRail item and route.
- The NavRail must not display a visible scrollbar.
- Use deterministic crops from the supplied visual sheet; do not use generated character art.
- Verify at 1366x768, 1200x800, and a lower-height non-fullscreen window.

---

### Task 1: Add failing layout contracts

**Files:**
- Modify: `frontend/src/components/layout/shellRailsVisual.test.ts`
- Modify: `frontend/src/components/chat/workbenchHome.test.ts`

- [ ] **Step 1: Write failing tests**

Import raw sources for `AppShell.tsx`, `NavRail.tsx`, and `index.css`. Assert:

```ts
expect(appShellSource).toContain('className="shell-ambient"');
expect(appShellSource).toContain('className="shell-ambient-overlay"');
expect(navRailSource).toContain("nav-rail-compact");
expect(navRailSource).toContain("nav-rail-item");
expect(indexCssSource).toContain("@media (max-height: 820px)");
expect(indexCssSource).toContain("@media (max-height: 700px)");
expect(workbenchHomeSource).not.toContain("cyrene-home-background.png");
```

- [ ] **Step 2: Verify RED**

```powershell
cd frontend
npm.cmd test -- --run src/components/layout/shellRailsVisual.test.ts src/components/chat/workbenchHome.test.ts
```

Expected: focused tests fail because the new shell and compact-mode hooks do not exist.

- [ ] **Step 3: Commit the contract**

```powershell
git add frontend/src/components/layout/shellRailsVisual.test.ts frontend/src/components/chat/workbenchHome.test.ts
git commit -m "test(ui): define global background and adaptive rail contracts"
```

### Task 2: Move artwork to the global shell

**Files:**
- Modify: `frontend/src/components/layout/AppShell.tsx`
- Modify: `frontend/src/index.css`
- Modify: `frontend/src/components/chat/WorkbenchHome.tsx`

- [ ] **Step 1: Add shell layers**

In `AppShell.tsx`, add these layers behind the existing theme background behavior:

```tsx
<div className="shell-ambient pointer-events-none absolute inset-0 -z-10" aria-hidden="true" />
<div className="shell-ambient-overlay pointer-events-none absolute inset-0 -z-10" aria-hidden="true" />
```

- [ ] **Step 2: Add translucent palette rules**

```css
.app-shell { background: transparent; }
.shell-ambient {
  background-image: url("/cyrene-home-background.png");
  background-position: center;
  background-repeat: no-repeat;
  background-size: cover;
  opacity: 0.42;
}
.shell-ambient-overlay {
  background:
    linear-gradient(135deg, rgba(248, 246, 253, 0.78), rgba(239, 232, 250, 0.56)),
    radial-gradient(circle at 50% 35%, rgba(255, 255, 255, 0.42), transparent 58%);
}
.workbench-grid,
.conversation-sidebar,
.execution-sidebar {
  background-color: color-mix(in srgb, var(--bg-app) 76%, transparent);
}
```

Leave `.home-ambient` as a transparent local layer and keep the home-only star points. The primary artwork declaration must move to `.shell-ambient`.

- [ ] **Step 3: Run focused tests and build**

```powershell
cd frontend
npm.cmd test -- --run src/components/layout/shellRailsVisual.test.ts src/components/chat/workbenchHome.test.ts
npm.cmd run build
```

- [ ] **Step 4: Commit**

```powershell
git add frontend/src/components/layout/AppShell.tsx frontend/src/index.css frontend/src/components/chat/WorkbenchHome.tsx
git commit -m "refactor(ui): apply cyrene artwork to the global shell"
```

### Task 3: Implement three-density NavRail

**Files:**
- Modify: `frontend/src/components/layout/NavRail.tsx`
- Modify: `frontend/src/index.css`

- [ ] **Step 1: Add presentation hooks**

Add `nav-rail-compact` to the existing list and `nav-rail-item` to each existing `NavLink`. Keep all routes, labels, tooltips, aria labels, and active behavior unchanged.

- [ ] **Step 2: Bound the rail**

```css
.nav-rail { min-height: 0; overflow: hidden; }
.nav-rail-list {
  display: flex;
  min-height: 0;
  flex: 1 1 auto;
  flex-direction: column;
  overflow: hidden;
}
.nav-rail-item { min-height: 3.5rem; }
```

- [ ] **Step 3: Add density states**

```css
@media (max-height: 820px) {
  .nav-rail-list > div { margin-top: .5rem; padding-top: .5rem; }
  .nav-rail-item { min-height: 3rem; gap: .125rem; font-size: .625rem; }
}
@media (max-height: 780px) {
  .nav-rail-list > div { margin-top: .125rem; padding-top: .125rem; }
  .nav-rail-item { min-height: 2rem; width: 2.75rem; gap: 0; font-size: 0; }
  .nav-rail-item svg { height: .95rem; width: .95rem; }
  .nav-rail-item span:not([aria-hidden="true"]) { display: none; }
}
```

The icon-first state uses the existing Tooltip and aria label, so no real item disappears.

- [ ] **Step 4: Run all frontend checks**

```powershell
cd frontend
npm.cmd test -- --run
npm.cmd run build
git diff --check
```

- [ ] **Step 5: Commit**

```powershell
git add frontend/src/components/layout/NavRail.tsx frontend/src/index.css
git commit -m "fix(ui): make nav rail adapt to compact window heights"
```

### Task 4: Verify actual Tauri windows

**Files:**
- Create: screenshot evidence under `C:/Users/25113/.codex/visualizations/`; no runtime source changes.

- [ ] **Step 1: Capture 1366x768, 1200x800, and a lower-height non-fullscreen Tauri WebView.**
- [ ] **Step 2: Confirm global artwork, readable translucent surfaces, complete or icon-first NavRail, no visible rail scrollbar, and Workbench dominance.**
- [ ] **Step 3: Run `git status --short`, `git log -3 --oneline --decorate`, and `git diff --check`; expect a clean worktree.**
