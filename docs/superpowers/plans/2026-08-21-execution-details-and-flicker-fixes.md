# Execution Details and Flicker Fixes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. This plan must be executed inline because the user prohibited subagents.

**Goal:** Open execution details to the left and remove route/Windows terminal flashes without changing business semantics.

**Architecture:** A pure placement helper drives a portal-based history detail panel. The persistent shell owns route suspense, while a shared backend command utility suppresses Windows consoles and the monitor caches static GPU metadata.

**Tech Stack:** React 18, TypeScript, React Router, Vitest, Rust, Tokio, Axum, Tauri 2.

## Global Constraints

- Do not use subagents.
- Preserve Agent, SSE, API schema, approval, security, and database behavior.
- Do not add dependencies.
- Use test-first red/green cycles.

---

### Task 1: Left-opening execution detail panel

**Files:**
- Create: `frontend/src/features/execution/executionHistoryPlacement.ts`
- Create: `frontend/src/features/execution/executionHistoryPlacement.test.ts`
- Modify: `frontend/src/features/execution/ExecutionHistory.tsx`
- Modify: `frontend/src/index.css`

**Interfaces:**
- Produces: `getExecutionDetailPlacement(anchor: DOMRectLike, viewport: ViewportSize): DetailPlacement`.

- [ ] Write tests asserting the panel right edge stays left of the card when space permits and remains viewport-clamped on narrow screens.
- [ ] Run `npm.cmd test -- --run src/features/execution/executionHistoryPlacement.test.ts` and confirm the missing implementation fails.
- [ ] Implement the placement helper and portal panel with one selected record, outside-click/Escape closing, and scroll/resize repositioning.
- [ ] Run the focused execution tests and confirm they pass.
- [ ] Commit with `fix(ui): open execution details toward workbench`.

### Task 2: Stable lazy-route loading

**Files:**
- Modify: `frontend/src/App.tsx`
- Modify: `frontend/src/components/layout/AppShell.tsx`
- Modify: `frontend/src/components/layout/RouteLoadingSurface.tsx`
- Modify: `frontend/src/components/layout/workspaceLayout.ts`
- Modify: `frontend/src/pages/routeLoadingContract.test.ts`
- Modify: `frontend/src/index.css`

**Interfaces:**
- `AppShell` keeps the shell mounted and wraps only `Outlet` with `Suspense`.

- [ ] Update the route contract test to require shell-owned suspense, no app-level suspense, no viewport entrance animation, and a delayed loading class.
- [ ] Run the contract test and confirm it fails against the current layout.
- [ ] Move the boundary, simplify the fallback surface, and add delayed opacity reveal with reduced-motion support.
- [ ] Run the route contract test and confirm it passes.
- [ ] Commit with `fix(ui): keep shell stable during route loading`.

### Task 3: Hidden and cached Windows monitoring command

**Files:**
- Create: `backend/src/utils/process.rs`
- Modify: `backend/src/utils/mod.rs`
- Modify: `backend/src/api/system.rs`
- Modify: `backend/src/tools/process.rs`
- Modify: `backend/src/mcp.rs`
- Modify: `backend/src/mcp_runtime/stdio.rs`
- Modify: `backend/src/isolation/mod.rs`

**Interfaces:**
- Produces: `hide_std_command_window(&mut std::process::Command)` and `hide_tokio_command_window(&mut tokio::process::Command)`.

- [ ] Add backend tests for the Windows flag and one-time GPU cache collector.
- [ ] Run focused backend tests and confirm the cache test fails before production changes.
- [ ] Implement the shared no-window utility, apply it to internal child commands, and cache GPU metadata.
- [ ] Run `cargo fmt`, focused tests, `cargo check`, and confirm success.
- [ ] Commit with `fix(runtime): suppress background command windows`.

### Task 4: Full verification

**Files:** No production changes expected.

- [ ] Run `npm.cmd test -- --run` in `frontend`.
- [ ] Run `npm.cmd run build` in `frontend`.
- [ ] Run `cargo fmt --check`, `cargo check`, and relevant `cargo test` in `backend`.
- [ ] Run `git diff --check` and inspect `git status --short --branch`.
