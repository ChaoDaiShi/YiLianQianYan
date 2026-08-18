# Phase 5A Frontend Bundle and Rendering Performance Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Execute inline in this session; do not use subagents.

**Goal:** Reduce the initial frontend JavaScript loaded by YiLianQianYan through feature-level route splitting and evidence-based local render optimization without changing frozen UI or runtime behavior.

**Architecture:** Keep `ChatPage`, `AppShell`, `NavRail`, Composer, messages, Markdown, and Agent feedback eager. Convert independent feature pages to `React.lazy` imports behind one shared `Suspense` loading surface. Let Vite generate chunks automatically; do not add `manualChunks` unless the measured post-split output proves a specific dependency requires it.

**Tech Stack:** React 18, TypeScript, React Router DOM 6, Vite 5, Vitest, Tauri desktop runtime, existing Skeleton/UI primitives.

## Global Constraints

- Preserve Backend API Schema, Database Schema, Agent Runtime, SSE Event Semantics, Memory/Embedding algorithms, MCP Protocol, Security Gateway, Approval Logic, Workflow Runtime, Task Runtime, and Tool Runtime.
- Preserve Phase 1–4B visual UI, navigation, accessibility, error handling, real-data semantics, and Tauri control-session bootstrap.
- Keep Chat/Workbench core eager; lazy-load only feature/page boundaries, not Button, Badge, Input, Card, or other small primitives.
- Do not add a permanent analyzer dependency, CDN, external runtime chunk, virtualization dependency, framework migration, router migration, state-library migration, Markdown migration, or Workflow Editor rewrite.
- Do not add `manualChunks` before route splitting and build measurement prove a specific stable need.
- Keep `prefers-reduced-motion`, keyboard behavior, ARIA, error handling, Approval, Stop, Streaming, and all existing tests.
- Use `npm.cmd`, not `npm.ps1`, for frontend commands on Windows.

---

### Task 1: Add a failing route-loading contract

**Files:**
- Create: `frontend/src/pages/routeLoadingContract.test.ts`
- Read: `frontend/src/App.tsx`

**Interfaces:** The test observes `src/App.tsx` through Vite `?raw` import. It requires synchronous `ChatPage`, lazy page imports for every low-frequency route, unchanged route paths, and a shared `Suspense` loading surface.

- [ ] **Step 1: Write the failing test**

```ts
import { describe, expect, it } from "vitest";
import appSource from "../App.tsx?raw";

const lazyPages = [
  "TaskCenterPage", "SystemPage", "LogsPage", "SettingsPage", "SkillsPage",
  "PluginsPage", "WorkflowsPage", "WorkspacesPage", "WorkspaceDetailPage",
  "AgentsPage", "CapabilitiesPage", "MemoryPage", "KnowledgePage",
];

describe("route loading contract", () => {
  it("keeps ChatPage eager and loads feature pages lazily", () => {
    expect(appSource).toContain('import ChatPage from "./pages/ChatPage"');
    for (const page of lazyPages) {
      expect(appSource).toContain(`lazy(() => import("./pages/${page}"))`);
      expect(appSource).not.toContain(`import ${page} from "./pages/${page}"`);
    }
  });

  it("preserves route paths and provides a shared Suspense surface", () => {
    for (const path of ["chat", "chat/:id", "tasks", "system", "logs", "settings", "workflows", "workspaces", "workspaces/:id", "memory", "knowledge"]) {
      expect(appSource).toContain(`path="${path}"`);
    }
    expect(appSource).toContain("Suspense");
    expect(appSource).toContain("RouteLoadingSurface");
  });
});
```

- [ ] **Step 2: Run the test to verify it fails for the missing lazy boundary**

Run: `npm.cmd test -- --run src/pages/routeLoadingContract.test.ts`

Expected: FAIL because `App.tsx` still has synchronous feature-page imports and no lazy route loading contract.

- [ ] **Step 3: Commit the test only**

```powershell
git add frontend/src/pages/routeLoadingContract.test.ts
git commit -m "test(perf): define route loading boundaries"
```

### Task 2: Implement feature-level lazy routes and loading surface

**Files:**
- Create: `frontend/src/components/layout/RouteLoadingSurface.tsx`
- Modify: `frontend/src/App.tsx`
- Test: `frontend/src/pages/routeLoadingContract.test.ts`

**Interfaces:** `RouteLoadingSurface` is a presentational component using existing `Skeleton` and Theme tokens. `App.tsx` keeps the existing BrowserRouter, route paths, `AppShell`, `Navigate` behavior, and eager `ChatPage`; listed feature pages are `lazy(() => import(...))` values rendered under one `Suspense` fallback.

- [ ] **Step 1: Implement the minimal loading surface**

Create `RouteLoadingSurface.tsx` with a full-height token-based loading panel containing one heading-sized Skeleton, two text-sized Skeletons, and `aria-label="页面加载中"`. Do not add a new animation or dependency.

- [ ] **Step 2: Replace only feature-page imports in `App.tsx`**

Use:

```tsx
import { lazy, Suspense } from "react";
import ChatPage from "./pages/ChatPage";
import RouteLoadingSurface from "./components/layout/RouteLoadingSurface";

const TaskCenterPage = lazy(() => import("./pages/TaskCenterPage"));
const SystemPage = lazy(() => import("./pages/SystemPage"));
const LogsPage = lazy(() => import("./pages/LogsPage"));
const SettingsPage = lazy(() => import("./pages/SettingsPage"));
const SkillsPage = lazy(() => import("./pages/SkillsPage"));
const PluginsPage = lazy(() => import("./pages/PluginsPage"));
const WorkflowsPage = lazy(() => import("./pages/WorkflowsPage"));
const WorkspacesPage = lazy(() => import("./pages/WorkspacesPage"));
const WorkspaceDetailPage = lazy(() => import("./pages/WorkspaceDetailPage"));
const AgentsPage = lazy(() => import("./pages/AgentsPage"));
const CapabilitiesPage = lazy(() => import("./pages/CapabilitiesPage"));
const MemoryPage = lazy(() => import("./pages/MemoryPage"));
const KnowledgePage = lazy(() => import("./pages/KnowledgePage"));
```

Wrap the existing `<Routes>` tree with `<Suspense fallback={<RouteLoadingSurface />}>`. Do not move or rename any route.

- [ ] **Step 3: Run focused tests and TypeScript build**

Run: `npm.cmd test -- --run src/pages/routeLoadingContract.test.ts`

Expected: PASS with 2 tests.

Run: `npm.cmd run build`

Expected: PASS and multiple `dist/assets/*.js` route chunks in addition to the smaller entry chunk.

- [ ] **Step 4: Commit the route split**

```powershell
git add frontend/src/App.tsx frontend/src/components/layout/RouteLoadingSurface.tsx frontend/src/pages/routeLoadingContract.test.ts
git commit -m "perf(frontend): lazy load feature routes"
```

### Task 3: Measure chunks and inspect rendering hot paths

**Files:**
- Read: `frontend/dist/assets/*`, `frontend/src/components/chat/MessageBubble.tsx`, `frontend/src/components/chat/StreamingText.tsx`, `frontend/src/components/chat/ToolCallCard.tsx`, `frontend/src/components/chat/MessageList.tsx`, `frontend/src/features/execution/ExecutionSidebar.tsx`, `frontend/src/pages/LogsPage.tsx`, `frontend/src/pages/MemoryPage.tsx`, `frontend/src/pages/CapabilitiesPage.tsx`, `frontend/src/components/workflow/WorkflowGraphEditor.tsx`
- Modify: only a narrowly scoped frontend component/test file if measurement identifies repeated expensive rendering.

**Interfaces:** Preserve all existing props, store selectors, API calls, reducer actions, SSE event handling, list semantics, and workflow graph data. A render optimization must be local to a stable component boundary and must not change visible output.

- [ ] **Step 1: Record post-split build output**

Run: `npm.cmd run build`

Record entry JS, CSS, gzip sizes, route chunks, and total JS using Vite output and `Get-ChildItem dist/assets`.

- [ ] **Step 2: Check Markdown and Streaming paths**

Confirm `react-markdown` remains in the eager conversation path and no token handler reparses unrelated messages. Do not replace the Markdown renderer or add per-token animation.

- [ ] **Step 3: Check list and execution paths**

Inspect message rows, tool cards, execution rows, log rows, memory rows, capability cards, and workflow nodes for stable props and identifiable repeated filter/sort/parse work on unrelated updates.

- [ ] **Step 4: Apply only evidence-backed local optimization**

If a stable child is demonstrably rerendered by unchanged props, add one local `React.memo` boundary and a focused behavior test. If a stable derived filter is recomputed from unchanged inputs, add one `useMemo` around that value and a focused behavior test. If inspection finds no concrete repeated work, make no speculative production render change and record that outcome.

- [ ] **Step 5: Run focused tests**

Run: `npm.cmd test -- --run src/pages/routeLoadingContract.test.ts`

Expected: PASS, plus any focused test for an actual render optimization.

### Task 4: Full regression, build, and scope verification

**Files:**
- Create or modify: focused frontend contract test only if a measured route-loading assertion is missing
- Read: `frontend/dist/assets/*`

- [ ] **Step 1: Run the complete frontend test suite**

Run: `npm.cmd test -- --run`

Expected: at least baseline 185 tests plus route-loading tests pass; no behavior test is removed.

- [ ] **Step 2: Run production build and inspect warnings**

Run: `npm.cmd run build`

Expected: exit 0, local dynamic chunks emitted, no CDN imports, and any remaining `>500 kB` warning identified by actual chunk/dependency.

- [ ] **Step 3: Check formatting and scope**

Run from repository root: `git diff --check` and `git diff --name-only 0571925f0a6061b4d68c42709f7154f87b7592bc..HEAD`.

Confirm changes are limited to frontend route loading, measured rendering, tests, and Phase 5A design/plan documents; no backend, API, security, runtime, or frozen UI files are changed.

### Task 5: Tauri lazy-chunk runtime and final delivery

**Files:**
- No planned source changes; screenshots/evidence go under `C:\Users\25113\.codex\visualizations\2026\08\18\`.

**Interfaces:** Validate the built local Tauri runtime, not only Vite development behavior. Do not alter settings, keys, security grants, workflows, tasks, or backend data during validation.

- [ ] **Step 1: Start the Tauri runtime**

Run from repository root: `npm.cmd run tauri dev`.

Confirm the application opens to `/chat` and Workbench Home/Conversation remains usable.

- [ ] **Step 2: Exercise lazy routes**

Open first and second time: `/tasks`, `/workspaces`, `/workflows`, `/memory`, `/knowledge`, `/skills`, `/plugins`, `/agents`, `/capabilities`, `/system`, `/logs`, and `/settings`. Confirm each resolves from local assets, the loading surface clears, and repeat navigation has no blank screen, undefined route, or chunk-load error. Explicitly verify `/workflows`, `/memory`, and `/settings`.

- [ ] **Step 3: Stop the temporary runtime**

Send Ctrl+C to the exact Tauri session and confirm no `yi-lian-qian-yan` process remains. Do not terminate unrelated processes.

- [ ] **Step 4: Commit and push the Phase 5A implementation**

```powershell
git add frontend/src frontend/package.json frontend/vite.config.ts
git commit -m "perf(frontend): reduce initial bundle and render overhead"
git push origin develop
```

Do not stage generated `frontend/dist` output unless it is already tracked by the repository.

- [ ] **Step 5: Verify delivery state**

Run:

```powershell
git status --short --branch
git log -1 --format="%H%n%s"
git ls-remote origin refs/heads/develop
```

Expected: clean `develop`, matching local/remote HEAD, full test/build/diff-check evidence, and a report distinguishing initial entry JS from total route bundle size.

## Self-Review Checklist

- Route paths and core Chat behavior remain unchanged.
- Every low-frequency page named in the spec has a lazy boundary.
- The fallback uses existing Skeleton/UI tokens and does not add a loading dependency.
- The plan does not require `manualChunks`, virtualization, or speculative memoization.
- Tauri local dynamic-chunk loading is explicitly verified.
- Test count is not reduced and no backend/API/security/runtime file is in scope.
