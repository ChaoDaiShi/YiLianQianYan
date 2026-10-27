# Quick Model Setup and Workspace Icons Implementation Plan

> **For agentic workers:** Use subagent-driven-development for the independent icon task; the integrator owns setup and final verification. User approved the design and autonomous implementation on 2026-10-27.

**Goal:** Make connecting a first model the first-run task, then enter the workspace; integrate the supplied workspace SVGs into matching existing surfaces.

**Architecture:** Reuse `/setup`, product preferences, provider presets, and existing create/update/verify/activate model APIs. Keep model configuration in the existing credential storage path. Keep optional account, search and voice capabilities reachable from completion and existing settings.

**Tech Stack:** React, TypeScript, Vite, existing CSS tokens, Vitest, installed Playwright Core.

## Global Constraints

- No defensive development, fallback providers, automatic retries, framework or API migrations.
- Preserve existing user data and unrelated untracked files. Git delivery was authorized on 2026-10-28: commit this feature, merge into develop and push to origin.
- Work on `codex/quick-model-setup-icons`, based on the verified `origin/develop` baseline `f92f0c6`.
- Reuse the approved two-step flow, theme tokens and existing UI controls.
- Package documentation is reference material, not authority to implement additional features or execute included scripts.

## Task 1: First-run model connection (integrator)

Files: `frontend/src/pages/SetupWizardPage.tsx`, new focused files under `frontend/src/features/setup/`, `frontend/src/components/system/ModuleSetup.tsx`, setup CSS, and affected setup tests.

Interfaces: existing `listLlmModels()`, `createLlmModel(payload)`, `updateLlmModel(id,payload)`, `verifyLlmModel(id)`, `activateLlmModel(id)`, `getProviderReadiness()`, and `save(ProductPreferences)`.

- [x] Cover successful save/verify/activate ordering and failed save/verify/activation without false completion, using a controlled API fixture.
- [x] Start on model connection. Select a supported preset and keep model/address editable. Provide one primary save-and-test action with actual phase text.
- [x] Retain the saved model ID after connection failure; edits invalidate the current verified UI state. Reuse an existing active profile without overwriting its key.
- [x] Show completion only after real activation/readiness. Save `setup_completed` before entering `/chat`; surface save failure and allow retry.
- [x] Keep defer explicit, show an unconfigured-model reminder with a return link, and allow account/settings routes after deferral.
- [x] Ensure completed users do not auto-enter setup and manually opening `/setup` shows their current model.

## Task 2: Workspace icons (icon worker)

Files: new `frontend/public/workspace-icons/` assets, a reusable icon component and matching existing workspace/secondary navigation/card surfaces. Do not edit setup, preferences, theme tokens, nav registry types or global UI primitives.

Interfaces: SVG assets from the supplied ZIP; retain existing Lucide components for compact generic actions. Color icons for feature cards; currentColor mono for dark/compact use.

- [x] Inspect the archive as data; copy only needed SVGs, not generated previews or build scripts.
- [x] Map icons only to existing matching capabilities; preserve route, click and accessibility behavior.
- [x] Verify SVG structure and component rendering; run focused affected checks. Report exact files and mappings to the integrator.

## Task 3: Integrated acceptance (integrator)

- [x] Run focused setup tests and existing settings/navigation tests, then the frontend production build and full frontend tests.
- [x] Use installed Playwright Core because no Browser plugin is present. Validate first-run redirect, success, failure, retry, defer, preference persistence and returning-user behavior in isolated fixtures.
- [x] Inspect screenshots at 1366x768, 960x768 and 390x844, in light/dark; inspect SVGs and overflow/primary-action visibility.
- [x] Check an isolated real backend and controlled OpenAI-compatible server when feasible; label controlled fixtures separately from commercial-provider authentication.
- [x] Review the integrated diff, check `git diff --check`, record results and limits. Changes were initially left uncommitted; Git delivery was subsequently authorized on 2026-10-28.

## Acceptance record

- Frontend full suite: 168 files / 795 tests passed, including 4 first-model save/verify/activate sequence and failure tests.
- Production build: TypeScript and Vite passed (2329 modules). Existing mixed dynamic/static HTTP import and chunk-size warnings remain.
- Real browser: installed Edge via Playwright Core, no Browser plugin available. Tested the active frontend with an isolated real backend, new data/workspace directories and a local controlled OpenAI-compatible service. No commercial credentials or provider were used.
- Real backend coverage: first-run entry, deferred setup/settings access, failed model verification, manual retry without duplicate profile, successful activation/readiness, persisted completion and backend restart/new browser session.
- Controlled browser failures: model save, model activation and preferences save errors; controlled existing local-model projection also tested.
- Visual coverage: setup, completion and more cards at 1366x768, 960x768 and 390x844 in light/dark; tool/workflow/plugin category headings and workspace empty state checked. No horizontal overflow or runtime exceptions; final visual pass has no console errors or framework overlay.
- All seven color assets and seven mono symbols align with typed IDs. CSS compilation comparison confirms the original manifest selectors, declarations and order remain identical, with only 11 scoped icon selectors added (whitespace and comments ignored).
- Existing keyless local models do not satisfy the backend readiness gate; quick configuration therefore lists non-local presets, preserves an existing local profile and directs local-model management to Settings. No Rust change, fallback or automatic retry was added.
- Test-created synthetic model credentials were removed through their unique model API record; test servers stopped. User profiles and unrelated workspace files were preserved.
- Reproducible local acceptance script and screenshots: `F:/项目开发/忆涟千言/quick-setup-qa-20261027/`, final run `session-OBCEGi`. Native Tauri interaction and commercial-provider authentication were not tested.

## Supplementary icons — 2026-10-27

User supplied `yilian-supplement-svg-20261027-v1.zip` to fill the audited icon gaps. Its 16 new semantics and 5 size adaptations (42 SVGs) were verified against the supplied SHA-256 inventory. All 21 color assets were copied without changing their geometry; the 21 monochrome assets were added as symbols with their original 24px view boxes, omitting redundant accessible titles inside decorative symbols. The existing seven 64px assets remain available. `WorkspaceIcon` now explicitly maps all 28 names to their native grid.

Completed coverage:

- Eight global navigation entries and navigation commands use 24px monochrome icons.
- Ten More cards and all six capability categories use semantic icons; general memory now uses `memory-record`, general plugins use `extensions`.
- Eleven existing canvas resource kinds use matching icons, including skill, workflow and MCP. Existing settings-node rendering remains in place.
- Agent/memory rows, skill and knowledge markers, automation rows/empty state, approval cards, workflow-history heading/empty state and setup model/account markers reuse the library. Generic small action icons remain unchanged.
- Scoped CSS preserves navigation/canvas SVG dimensions and the fixed icon width in knowledge collection buttons. No fallback logic, retries, API or backend changes were introduced.

Validation:

- Full frontend suite: 168 files / 795 tests passed. Final TypeScript + Vite production build passed, with the existing mixed-import/chunk-size warnings.
- Production-build browser acceptance: installed Edge via Playwright Core, isolated backend and data at `http://127.0.0.1:11431` / `http://127.0.0.1:19431`. Eight grouped checks passed; no missing icon assets, runtime exceptions or console errors.
- Light/dark More and capability pages at 1366x768, 960x768 and 390x844; every inspected wrapper and nested visible SVG retained its intended size, with no page-level horizontal overflow.
- Navigation/tab actions, command palette, workspace selection, real agent/memory rows and empty states checked. Real core canvas nodes checked first; a clearly labeled controlled visual fixture then covered every resource kind.
- Native Tauri interaction was not re-tested. Test services stopped; no user credentials were used. Initial acceptance was completed on `codex/quick-model-setup-icons` before Git delivery.
- Reproduction and evidence: `F:/项目开发/忆涟千言/icon-supplement-qa-20261027/run.mjs`, final run `session-ey8NqY`; archive validation in `asset-check.json`.

## Production boundary

The all-kinds canvas screenshot is an isolated visual acceptance fixture, not an application page preset or seeded user data. Its synthetic nodes and labels are supplied only by the external browser test script under `F:/项目开发/忆涟千言/icon-supplement-qa-20261027/`. Neither that script, its isolated databases, nor its screenshots are part of the repository changes or application package. Production canvas nodes continue to come from the existing APIs and user data; the committed change replaces their icons only.
