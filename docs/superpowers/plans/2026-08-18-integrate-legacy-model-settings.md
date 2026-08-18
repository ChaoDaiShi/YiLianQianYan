# Integrate Legacy Model Settings Implementation Plan

> **For agentic workers:** Execute this plan inline in the current isolated worktree. Do not dispatch subagents.

**Goal:** Move the legacy model configuration controls into `ModelManagerPanel` so the settings page has one unified model configuration entry point.

**Architecture:** Keep `SettingsPage` as the owner of `AppConfig` state and persistence callbacks. Extend `ModelManagerPanel` with a typed legacy-model props object and render the existing legacy fields inside its unified panel. Remove the duplicated legacy JSX from `SettingsPage` without changing backend contracts.

**Tech Stack:** React, TypeScript, Vitest, Vite, Rust/Axum backend.

## Global Constraints

- Preserve the existing `AppConfig` model shape and `/api/settings` behavior.
- Preserve secret redaction, clear-secret, migration-warning, and save behavior.
- Do not add dependencies or change backend code.
- Do not dispatch subagents.

### Task 1: Add the failing frontend composition contract test

**Files:**
- Modify: `frontend/src/pages/systemSettingsContract.test.ts`
- Test: `frontend/src/pages/systemSettingsContract.test.ts`

- [ ] **Step 1: Write the failing test**

Add a test that reads `SettingsPage.tsx` and `ModelManagerPanel.tsx`, then asserts `SettingsPage` passes a `legacyModel` prop to `ModelManagerPanel`, while the standalone settings page source no longer contains the old `API 地址` and `Embedding 配置` labels.

- [ ] **Step 2: Run the focused test**

Run `npm.cmd test -- src/pages/systemSettingsContract.test.ts` from `frontend`.

Expected: FAIL because the current page renders `<ModelManagerPanel />` without legacy props and still contains the standalone legacy labels.

### Task 2: Move legacy controls into the model manager

**Files:**
- Modify: `frontend/src/features/llm/ModelManagerPanel.tsx`
- Modify: `frontend/src/pages/SettingsPage.tsx`

- [ ] **Step 1: Implement typed composition props**

Add a `LegacyModelSettingsProps` interface containing `config: AppConfig`, `updateField`, `clearSecret`, `secretSourceLabel`, and `saveError`/migration status values needed by the existing controls. Render the existing legacy fields in a `运行时兼容配置` block inside `ModelManagerPanel`, reusing the same callbacks and labels.

- [ ] **Step 2: Pass props from SettingsPage and remove duplicate JSX**

Pass `config`, `updateField`, `clearSecret`, `secretSourceLabel`, and the two status flags to `<ModelManagerPanel />`. Delete the old standalone legacy model controls from the `model` switch branch, leaving only the model manager component.

- [ ] **Step 3: Run the focused test**

Run `npm.cmd test -- src/pages/systemSettingsContract.test.ts` from `frontend`.

Expected: PASS.

### Task 3: Verify and integrate

**Files:**
- No additional source files.

- [ ] **Step 1: Run frontend tests**

Run `npm.cmd test` from `frontend`; expect all tests to pass.

- [ ] **Step 2: Run frontend production build**

Run `npm.cmd run build` from `frontend`; expect exit code 0.

- [ ] **Step 3: Run backend verification**

Run `cargo fmt --check` and `cargo test` from `backend`; expect no failures. Existing unrelated warnings may remain.

- [ ] **Step 4: Commit the isolated change**

Run `git add frontend/src/pages/SettingsPage.tsx frontend/src/features/llm/ModelManagerPanel.tsx frontend/src/pages/systemSettingsContract.test.ts docs/superpowers/specs/2026-08-18-integrate-legacy-model-settings-design.md docs/superpowers/plans/2026-08-18-integrate-legacy-model-settings.md` and commit with `fix(settings): integrate legacy model controls into manager`.
