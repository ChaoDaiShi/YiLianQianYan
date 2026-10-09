# Context Rail Layout Implementation Plan

> **For agentic workers:** Execute inline. Repository instructions prohibit unrequested subagents. The user prohibited tests, so this plan uses source review and one final production build only.

**Goal:** Make Chat and Task Canvas center-first workspaces with a shared right ContextRail that is collapsed by default and opens only one contextual panel at a time.

**Architecture:** Add one stateless layout component shared by both surfaces. Keep existing Chat drawer state and Canvas `activePanel` as the business state owners. Refactor only composition and CSS; execution, approval, Canvas state and APIs remain untouched.

**Tech Stack:** React, TypeScript, Lucide React, existing CSS variables/Tailwind utilities, Vite.

## Global Constraints

- Base `0ba7a7dddb9814b8ddbd88ca8f7f08ed36ba6353`, branch `feat/v1-context-rail-layout`.
- Chat execution panel defaults collapsed in full mode.
- Canvas right rail defaults collapsed; selecting a node does not open it.
- One panel state and one visible entry per function at each breakpoint.
- No backend, Cargo, DB, REST, Secret, v2, dependency, Canvas geometry/state/save changes.
- No tests or E2E. Run one final `npm.cmd run build` only.

---

### Task 1: Shared Context Rail

**Files:**

- Create: `frontend/src/components/layout/ContextRail.tsx`
- Create: `frontend/src/styles/contextRail.css`
- Modify: `frontend/src/index.css`

**Interfaces:**

- `ContextRailItem { id, label, icon, controls, expanded, badge?, statusTone?, onSelect }`.
- `ContextRail { label, items, className? }` renders one accessible vertical auxiliary rail.

- [x] Implement stateless rail, tooltips, badges and expanded state.
- [x] Add the canonical 52px rail CSS, focus state and compact sizing.
- [x] Commit `feat(layout): add shared context rail`.

### Task 2: Chat Center-First Layout

**Files:**

- Modify: `frontend/src/pages/ChatPage.tsx`
- Modify: `frontend/src/features/execution/ExecutionSidebar.tsx`
- Modify: `frontend/src/styles/workspace.css`
- Modify: `frontend/src/styles/shell.css`

**Interfaces:**

- `executionCollapsed` initializes `true` in full mode.
- `ContextRail` replaces the collapsed ExecutionSidebar rendering in full mode.
- Existing Drawer remains the only compact/narrow execution surface.

- [x] Default the full-mode execution area to a rail.
- [x] Show pending approval count and current status without exposing full history.
- [x] Expand/collapse explicitly and handle Escape only in full expanded mode.
- [x] Simplify ExecutionSidebar to expanded panel content.
- [x] Keep message column centered and bounded.
- [x] Commit `feat(chat): make execution context collapsed by default`.

### Task 3: Canvas Notion-Style Context Rail

**Files:**

- Modify: `frontend/src/features/task-world/TaskWorldPage.tsx`
- Modify: `frontend/src/features/task-world/StudioHeader.tsx`
- Modify: `frontend/src/features/task-world/canvas/studio.css`

**Interfaces:**

- Existing `StudioPanel` remains the only panel state.
- Rail items map exactly to `inspector`, `trail`, and `tools`.

- [x] Add ContextRail beside the canvas and map three explicit actions.
- [x] Remove duplicate panel triggers from StudioHeader.
- [x] Dock every expanded panel to the left of the rail.
- [x] Keep focus restoration, Escape, selection/camera invariants and bottom toolbar safe area.
- [x] Commit `style(canvas): unify auxiliary panels behind context rail`.

### Task 4: Scope Audit and Build

**Files:**

- Create: `docs/ui/context-rail-layout-report.md`
- Create ignored evidence: `target/context-rail-layout/`.

- [x] Confirm one shared rail, Chat default collapsed, Canvas default null and no duplicate StudioHeader actions.
- [x] Confirm protected paths and package manifests unchanged.
- [x] Run exactly one `npm.cmd run build` and retain output.
- [x] Record tests not run and runtime visual/focus limitations.
- [x] Commit `docs(layout): record context rail build result`.
