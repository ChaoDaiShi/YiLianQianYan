# Phase R1A Shell and Home Visual Reconstruction Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reconstruct the Tauri Shell/Home composition around the Workbench, remove visible left-rail scrollbars, and restore SkillsPage to a left-list/right-detail layout without changing business behavior.

**Architecture:** Keep the existing React/Tailwind component boundaries, API calls, stores, runtime state, and workspace modes. Add scoped presentation hooks and CSS for Shell, Home, ConversationSidebar, ExecutionSidebar, and SkillsPage; preserve all current event handlers and data contracts. Validate each visual pass with real Tauri screenshots at 1366×768 and 1200×800.

**Tech Stack:** React 18, TypeScript, Vite, Tailwind CSS, existing semantic CSS variables, Tauri 2, Vitest.

## Global Constraints

- UI behavior remains unchanged: Enter sends, Shift+Enter inserts a newline, Send and Stop retain their current handlers.
- Left rails must not show a visible scrollbar; wheel/touch scrolling may remain available where needed so existing conversations and skills remain accessible.
- `SkillsPage` may change layout only; `listSkills`, `loadSkill`, loading/error/empty states, search, selection, and retry behavior remain intact.
- Do not modify Backend, API, SSE, Store business models, Agent Runtime, Memory, Workflow, MCP, Security, Approval, Database, or other Feature Pages.
- Do not add dependencies, fonts, canvas, WebGL, particle systems, video, or large blur animations.
- Keep the existing `frontend/public/favicon.png`; report its 256×256 opaque white-background limitation as an Asset Gap.
- Use the existing `develop` baseline commit `cdc8adf` and the isolated branch `feat/shell-home-visual-reconstruction`.
- The frontend baseline is 43 test files and 189 tests; the backend baseline is 685 tests across the verified crates and integration suites.

---

## File Map

- Modify `frontend/src/components/layout/AppShell.tsx`: add only Shell presentation hooks around the existing background, NavRail, and outlet.
- Modify `frontend/src/components/layout/NavRail.tsx`: refine Dock classes without changing `NAV_GROUPS`, health polling, route targets, tooltips, or status semantics.
- Modify `frontend/src/pages/ChatPage.tsx`: add scoped region classes and soften the existing divider wrappers without changing workspace mode or drawer behavior.
- Modify `frontend/src/components/chat/ConversationSidebar.tsx`: add scoped Context Rail hooks, make the create action neutral, and hide the visual scrollbar while retaining list access.
- Modify `frontend/src/components/chat/WorkbenchHome.tsx`: add visual wrappers/classes and implement the centered vertical composition without changing quick-action data or callbacks.
- Modify `frontend/src/components/chat/ChatInput.tsx`: add a scoped Composer hook only; do not change input state or keyboard/send logic.
- Modify `frontend/src/features/execution/ExecutionSidebar.tsx`: add scoped region/content hooks and idle presentation styles without changing selectors, approval handlers, collapse, or close behavior.
- Modify `frontend/src/pages/SkillsPage.tsx`: wrap the existing list and detail panels in the existing split-layout pattern and add a Skills-specific scope class.
- Modify `frontend/src/index.css`: change Shell/Home visual tokens and add scoped SkillsPage overrides; do not alter global capability styles in a way that changes CapabilitiesPage.
- Modify `frontend/src/components/chat/workbenchHome.test.ts`: retain existing composition assertions and add only a necessary contract for the new Composer/hero hooks if the implementation uses them.
- Modify `frontend/src/pages/skillsCapabilityContract.test.ts`: assert the real APIs and the new split-layout hook remain present without inventing metadata or actions.

## Task 1: Add Shell and Dock presentation hooks

**Files:**
- Modify: `frontend/src/components/layout/AppShell.tsx`
- Modify: `frontend/src/components/layout/NavRail.tsx`
- Modify: `frontend/src/pages/ChatPage.tsx`
- Modify: `frontend/src/index.css`
- Test: `frontend/src/components/layout/navGroups.test.ts`

**Interfaces:**
- Consumes: existing `NAV_GROUPS`, `useTheme`, backend health state, `getWorkspaceMode`, and `WORKBENCH_VIEWPORT_CLASS_NAME`.
- Produces: scoped classes `.app-shell`, `.nav-rail`, `.workspace-region`, `.conversation-region`, and `.execution-region` for CSS only.

- [ ] **Step 1: Add failing presentation contract assertions**

Extend the existing source contract test with these exact assertions:

```ts
import navRailSource from "./NavRail.tsx?raw";

it("keeps the Dock presentation hooks", () => {
  expect(navRailSource).toContain("nav-rail");
  expect(navRailSource).toContain('aria-label="全局导航"');
  expect(navRailSource).toContain("后端状态：");
});
```

Run from `frontend`:

```text
npm.cmd test -- --run src/components/layout/navGroups.test.ts
```

Expected: the new test fails because `nav-rail` is not yet present.

- [ ] **Step 2: Add only presentation hooks**

Add `app-shell` to the AppShell root, `nav-rail` to the existing `<nav>`, `workspace-region` to the `<main>`, and `conversation-region` / `execution-region` to the existing ChatPage wrappers. Keep all children, handlers, route logic, and mode checks unchanged.

- [ ] **Step 3: Reconstruct Shell widths and Dock styling**

In `index.css`, implement the following layout values:

```css
.workbench-grid[data-mode="full"] {
  grid-template-columns: 252px minmax(0, 1fr) 296px;
}

.workbench-grid[data-mode="full"][data-execution-collapsed="true"] {
  grid-template-columns: 252px minmax(0, 1fr) 48px;
}

.workbench-grid[data-mode="compact"] {
  grid-template-columns: 252px minmax(0, 1fr);
}

.nav-rail {
  width: 76px;
  background: linear-gradient(180deg, var(--sidebar-bg), var(--sidebar-bg-2));
}
```

Use existing color variables, reduce active border/glow weight, keep labels centered, and keep backend status as a dot with tooltip only.

- [ ] **Step 4: Run the focused contract and build**

Run:

```text
npm.cmd test -- --run src/components/layout/navGroups.test.ts
npm.cmd run build
```

Expected: focused contract passes and the production frontend build passes.

- [ ] **Step 5: Commit the Shell slice**

```text
git add frontend/src/components/layout/AppShell.tsx frontend/src/components/layout/NavRail.tsx frontend/src/pages/ChatPage.tsx frontend/src/index.css frontend/src/components/layout/navGroups.test.ts
git commit -m "refactor(ui): reconstruct desktop shell proportions"
```

## Task 2: Rebuild Conversation and Execution rails visually

**Files:**
- Modify: `frontend/src/components/chat/ConversationSidebar.tsx`
- Modify: `frontend/src/features/execution/ExecutionSidebar.tsx`
- Modify: `frontend/src/index.css`

**Interfaces:**
- Consumes: existing conversation polling, `onSelect`, `onNew`, `deleteConversation`, execution selectors, approval callbacks, and collapse/close callbacks.
- Produces: `.conversation-sidebar`, `.conversation-list`, `.conversation-row`, `.execution-sidebar`, and `.execution-sidebar-content` presentation scopes.

- [ ] **Step 1: Add scoped class hooks without changing handlers**

Add the named classes to the existing aside/header/list/row/content elements. Remove only the `scrollbar-thin` utility from the ConversationSidebar list element; do not remove its list container or its `overflow-y-auto` behavior until the replacement CSS is present.

- [ ] **Step 2: Make the left rail visually scrollbar-free**

Add this scoped CSS:

```css
.conversation-sidebar,
.conversation-list,
.skills-page .capability-list-scroll {
  scrollbar-width: none;
  -ms-overflow-style: none;
}

.conversation-sidebar::-webkit-scrollbar,
.conversation-list::-webkit-scrollbar,
.skills-page .capability-list-scroll::-webkit-scrollbar {
  display: none;
  width: 0;
  height: 0;
}

.conversation-list {
  overscroll-behavior: contain;
}
```

This keeps wheel/touch access for long real lists but removes the visible scrollbar requested by the user.

- [ ] **Step 3: Reduce rail weight and idle execution contrast**

Style the create action as an existing secondary button surface with a pink icon accent, tighten task rows, use soft selected background, and set the expanded execution region to `background: color-mix(in srgb, var(--bg-app) 88%, var(--accent-purple) 12%)` with low-contrast header/content. Do not modify button callbacks, displayed task title derivation, current-action selectors, or approval actions.

- [ ] **Step 4: Run the frontend regression suite**

Run:

```text
npm.cmd test -- --run
```

Expected: 43 test files and 189 tests pass.

- [ ] **Step 5: Commit the rail slice**

```text
git add frontend/src/components/chat/ConversationSidebar.tsx frontend/src/features/execution/ExecutionSidebar.tsx frontend/src/index.css
git commit -m "refactor(ui): quiet conversation and execution rails"
```

## Task 3: Reconstruct Workbench Home and Composer hierarchy

**Files:**
- Modify: `frontend/src/components/chat/WorkbenchHome.tsx`
- Modify: `frontend/src/components/chat/ChatInput.tsx`
- Modify: `frontend/src/components/chat/workbenchHome.test.ts`
- Modify: `frontend/src/index.css`

**Interfaces:**
- Consumes: existing `connection`, `pendingApprovals`, `isLoading`, `onSend`, `onStop`, `suggestedText`, quick-action prompts, and `ChatInput` keyboard behavior.
- Produces: `.home-content`, `.home-hero`, `.home-character`, `.home-character-image`, `.home-status`, `.home-composer`, and `.home-quick-actions` visual hooks.

- [ ] **Step 1: Extend the minimal composition contract**

Add these assertions to `workbenchHome.test.ts`:

```ts
expect(workbenchHomeSource).toContain("home-hero");
expect(workbenchHomeSource).toContain("home-status");
expect(workbenchHomeSource).toContain("home-composer");
```

Run the focused test and expect it to fail before hooks are added.

- [ ] **Step 2: Add wrappers and scoped hooks**

Wrap the existing greeting and character in `home-hero`, add `home-status` to the existing status pill, add `home-composer` to the Composer wrapper, and retain all current quick-action buttons and prompts. Add no new controls or behavior.

- [ ] **Step 3: Implement the centered visual rhythm**

Use a single centered column with a bounded width and unequal gaps. The target CSS is:

```css
.home-content {
  width: min(100%, 760px);
  margin-inline: auto;
  gap: 0;
  padding: clamp(20px, 4vh, 36px) 24px 28px;
}

.home-hero {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: clamp(12px, 2.2vh, 20px);
}

.home-character-image {
  width: clamp(180px, 22vh, 220px);
  height: clamp(180px, 22vh, 220px);
  mix-blend-mode: multiply;
}

.home-quick-actions {
  display: grid;
  width: min(100%, 680px);
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 10px;
  margin-top: clamp(18px, 3vh, 30px);
}
```

Keep the Composer around 640–700px, use a light surface and subtle focus ring, remove the per-card shadow emphasis from Quick Actions, and use only existing semantic colors.

- [ ] **Step 4: Validate the focused home contract and build**

Run:

```text
npm.cmd test -- --run src/components/chat/workbenchHome.test.ts
npm.cmd run build
```

Expected: the home contract passes and the production build succeeds.

- [ ] **Step 5: Commit the Home slice**

```text
git add frontend/src/components/chat/WorkbenchHome.tsx frontend/src/components/chat/ChatInput.tsx frontend/src/components/chat/workbenchHome.test.ts frontend/src/index.css
git commit -m "refactor(ui): center cyrene home composition"
```

## Task 4: Restore SkillsPage left-list/right-detail composition

**Files:**
- Modify: `frontend/src/pages/SkillsPage.tsx`
- Modify: `frontend/src/pages/skillsCapabilityContract.test.ts`
- Modify: `frontend/src/index.css`

**Interfaces:**
- Consumes: existing `listSkills`, `loadSkill`, `Skill` shape, selection state, loading/error/empty states, and shared UI components.
- Produces: `.skills-page`, `.skills-page-body`, and `.skills-split-layout` presentation hooks.

- [ ] **Step 1: Add the layout contract**

Extend the existing test with:

```ts
expect(skillsPageSource).toContain('className="capability-page skills-page"');
expect(skillsPageSource).toContain("skills-split-layout");
expect(skillsPageSource).toContain("listSkills");
expect(skillsPageSource).toContain("loadSkill");
```

Run the focused test and expect the new layout assertions to fail before the wrapper is added.

- [ ] **Step 2: Wrap the existing panels in the shared split layout**

Change only the outer presentation structure to:

```tsx
<div className="capability-page skills-page">
  <PageHeader title="技能" description="管理小昔涟可以使用的技能能力。" />
  <div className="capability-page-body skills-page-body">
    <div className="capability-split-layout skills-split-layout">
      {/* existing list aside, unchanged API/state handlers */}
      {/* existing detail section, unchanged content and retry handlers */}
    </div>
  </div>
</div>
```

The comments above describe the existing nodes to move; no data or handlers are added or removed.

- [ ] **Step 3: Scope the two-column CSS to SkillsPage**

Add:

```css
.skills-page-body {
  overflow: hidden;
}

.skills-split-layout {
  min-height: 0;
  flex: 1;
  grid-template-columns: minmax(220px, 260px) minmax(0, 1fr);
  gap: 18px;
}

.skills-page .capability-list-panel,
.skills-page .capability-detail-panel {
  border-color: var(--border-soft);
  background: color-mix(in srgb, var(--surface) 84%, var(--bg-app) 16%);
  box-shadow: none;
}

@media (max-width: 900px) {
  .skills-page-body {
    overflow-y: auto;
  }

  .skills-split-layout {
    grid-template-columns: minmax(0, 1fr);
  }
}
```

Do not alter the global `.capability-split-layout` rules used by CapabilitiesPage.

- [ ] **Step 4: Run SkillsPage regression and full frontend tests**

Run:

```text
npm.cmd test -- --run src/pages/skillsCapabilityContract.test.ts
npm.cmd test -- --run
```

Expected: the focused contract passes and the full suite remains at least 189 passing tests.

- [ ] **Step 5: Commit the SkillsPage slice**

```text
git add frontend/src/pages/SkillsPage.tsx frontend/src/pages/skillsCapabilityContract.test.ts frontend/src/index.css
git commit -m "refactor(ui): restore skills split layout"
```

## Task 5: Real Tauri screenshot rounds and final verification

**Files:**
- Modify: any of the presentation files above only when a screenshot identifies a visual regression.
- Create: screenshot artifacts outside the repository under `C:\Users\25113\.codex\visualizations\2026\08\18\01a013c3-dbe7-76e3-926a-ceb9b1d1eebe\r1a\`.

**Interfaces:**
- Consumes: the built frontend, Tauri dev window, existing backend startup, and fixed window sizes.
- Produces: Round 1 and Round 2 screenshots plus a written visual review for each round.

- [ ] **Step 1: Start the isolated Tauri dev app**

Run from the worktree root:

```text
npm.cmd run tauri dev
```

Expected: the Tauri window opens with the isolated worktree frontend and its embedded local backend.

- [ ] **Step 2: Capture Round 1 at both fixed sizes**

Resize the actual Tauri top-level window to `1366×768` and `1200×800`, capture the window bounds, and save PNGs under the artifact directory. Record answers to: first visual focus, Workbench dominance, character presence, Composer discoverability, sidebar competition, card count, border density, pink density, Desktop App feeling, and Generic Admin Dashboard feeling.

- [ ] **Step 3: Fix only the largest three to five visual defects**

Use the Round 1 screenshots to adjust spacing, character size, side-rail contrast, Composer width, Quick Action grid, or border/surface hierarchy. Do not add controls, change copy semantics, or touch frozen feature pages.

- [ ] **Step 4: Capture Round 2 and compare both sizes**

Repeat the same Tauri capture at `1366×768` and `1200×800`. Expected: the Workbench is the clear visual center, the character is larger and no longer presents as a hard white square, Composer is immediately recognizable, both left rails have no visible scrollbar, and SkillsPage retains a left-list/right-detail composition. If the screen still reads as a Generic Admin Dashboard, perform one additional visual round before completion.

- [ ] **Step 5: Run final verification**

Run from the worktree root/frontend:

```text
npm.cmd test -- --run
npm.cmd run build
git diff --check
```

Expected: all frontend tests pass, build passes, and `git diff --check` produces no output.

- [ ] **Step 6: Inspect scope and commit the final visual pass**

Run:

```text
git status --short
git diff --name-only develop...HEAD
```

Expected changed paths are limited to the design doc, implementation plan, and the scoped frontend presentation/test files listed above. Commit with:

```text
git add frontend/src docs/superpowers/specs/2026-08-18-shell-home-visual-reconstruction-design.md docs/superpowers/plans/2026-08-18-shell-home-visual-reconstruction.md
git commit -m "refactor(ui): reconstruct cyrene shell and home composition"
```

## Plan Self-Review

- Spec coverage: Shell proportions, NavRail, ConversationSidebar, Workbench/Home, Character, Composer, Quick Actions, ExecutionSidebar, surface hierarchy, typography/color via scoped CSS, responsive behavior, no visible left-rail scrollbar, and SkillsPage left/right layout are covered by Tasks 1–5.
- Placeholder scan: no incomplete marker or unspecified implementation step is used; each code change has named files, selectors, commands, and expected results.
- Type/contract consistency: all tasks preserve existing callback and API names; new names are presentation-only CSS hooks (`app-shell`, `nav-rail`, `conversation-sidebar`, `home-*`, `skills-*`).
- Scope check: no backend, API, store, runtime, security, database, or unrelated feature-page work is included.
