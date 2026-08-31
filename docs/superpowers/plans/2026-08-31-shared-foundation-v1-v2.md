# AI-0 Shared Foundation & Integration Spine Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Subagents are forbidden for this execution. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build and validate the thin Shared Foundation that lets v1 Task World and v2 Desktop World develop from one commit using stable contracts and explicit mocks.

**Architecture:** Add versioned Shared/Core migrations and focused Rust shared services behind protected REST/SSE adapters. Preserve existing v0.9 code through compatible frontend facades and a visual-neutral Surface host extraction.

**Tech Stack:** Rust 2021, Axum 0.7, Tokio broadcast, rusqlite/SQLite, React 18, TypeScript 5, Vite 5, Vitest 2, Tauri 2, GitHub Actions.

## Global Constraints

- Base is `main@7906ce7d8cbc29b207a22a222f24c52f594eae35`; never reset to an older baseline.
- Shared/Core migration versions are `0000-0999`; v1 owns `1000-1999`; v2 owns `2000-2999`.
- React business access remains REST/SSE; Tauri IPC remains native-host-only.
- Workspace is not DesktopSpace; Workflow is not TaskGraph; Capability discovery is not execution.
- No Infinite Canvas, TaskGraph, Task Supervisor, DesktopSpace, App Mount, wallpaper, widgets, desktop pet, high-level parser, or complete voice product.
- Existing behavior and public API compatibility take priority; contract changes are additive first.
- Every real side effect remains behind the Security Execution Gateway; mocks are explicitly simulated.
- Each phase ends with tests, production-relevant build/check, focused commit, and clean status.

---

### Task 0: Baseline and design safety net

**Files:**
- Create: `docs/superpowers/specs/2026-08-31-shared-foundation-v1-v2-design.md`
- Create: `docs/superpowers/plans/2026-08-31-shared-foundation-v1-v2.md`

**Interfaces:**
- Consumes: v0.9 repository at the exact base SHA.
- Produces: fixed scope, rollback boundaries, baseline evidence, and executable phase order.

- [ ] Record `git status`, branch, HEAD, latest log, and remotes.
- [ ] Run `cargo test --workspace --all-targets`; require exit 0 and record test totals/warnings.
- [ ] Run `npm.cmd test` and `npm.cmd run build` in `frontend`; require exit 0 and record totals/assets.
- [ ] Run `cargo check -p yi-lian-qian-yan`; require exit 0.
- [ ] Record the existing NSIS artifact byte size and perform read-only API/startup smoke inspection.
- [ ] Commit the design and plan with `docs(foundation): define shared integration spine`.

### Task 1: Versioned migration foundation

**Files:**
- Create: `backend/src/db/migrations.rs`
- Modify: `backend/src/db/mod.rs`
- Test: `backend/src/db/migrations.rs`

**Interfaces:**
- Produces: `run_versioned_migrations(conn: &mut Connection, adopted_v09: bool) -> rusqlite::Result<()>`, `MigrationRecord`, versions `0` and `1`.

- [ ] Write tests for fresh DB, recognizable v0.9 adoption, ordered records, repeat-open idempotency, namespace rejection, and no v0.9 table loss.
- [ ] Run `cargo test -p yilian-backend db::migrations -- --nocapture`; require RED because the migration module/API is absent.
- [ ] Implement transactional migration application and preserve the current v0.9 bootstrap SQL unchanged.
- [ ] Run the focused tests; require all GREEN.
- [ ] Run `cargo fmt --check`, `cargo check -p yilian-backend`, and the full backend tests.
- [ ] Commit `refactor(db): add versioned migration foundation`.

### Task 2: Frontend API boundary and Surface host

**Files:**
- Move: `frontend/src/api/client.ts` to `frontend/src/api/legacy.ts`
- Create: `frontend/src/api/client.ts`, `transport.ts`, `chat.ts`, `conversations.ts`, `tasks.ts`, `workflows.ts`, `workspaces.ts`, `capabilities.ts`, `memory.ts`, `plugins.ts`, `system.ts`
- Create: `frontend/src/surfaces/workspace/WorkspaceSurface.tsx`
- Create: `frontend/src/surfaces/desktop/DesktopSurfaceSkeleton.tsx`
- Create: `frontend/src/surfaces/AppRoot.tsx`, `surfaceHost.ts`, `surfaceHost.test.ts`
- Modify: `frontend/src/App.tsx`

**Interfaces:**
- Produces: compatibility exports from `api/client.ts`; `SurfaceHostMode = "standalone" | "desktop-skeleton"`; `resolveSurfaceHostMode(value?: string): SurfaceHostMode`.

- [ ] Write tests proving domain entrypoints exist, the compatibility facade remains source-compatible, default host is standalone, invalid host values fail to standalone, and both hosts contain WorkspaceSurface.
- [ ] Run focused Vitest tests and require RED for missing domain/surface modules.
- [ ] Move legacy implementation, add thin domain exports/facade, and extract route ownership into WorkspaceSurface without changing route paths or layout markup.
- [ ] Run focused tests, full frontend tests, and `npm.cmd run build`; require GREEN/exit 0.
- [ ] Commit API and Surface changes as independently revertible commits: `refactor(frontend): split api client by domain`, then `refactor(surface): establish workspace surface host`.

### Task 3: Event and Command spines

**Files:**
- Create: `backend/src/shared/mod.rs`, `contracts.rs`, `event.rs`, `command.rs`
- Create: `backend/src/api/events.rs`, `backend/src/api/commands.rs`
- Modify: `backend/src/lib.rs`, `backend/src/server.rs`, `backend/src/api/mod.rs`
- Create: `frontend/src/api/events.ts`, `commands.ts`
- Test: shared modules, API router tests, `frontend/src/api/events.test.ts`, `commands.test.ts`

**Interfaces:**
- Produces: `YiEvent`, `EventHub::publish/subscribe`, `CommandRequest`, `CommandResult`, `CommandRouter::register/execute`, `subscribeToEvents`, `executeCommand`.

- [ ] Write serialization/additive-field/namespace/router/not-found/mock tests and require RED.
- [ ] Implement a bounded product EventHub and protected `/api/events` SSE route.
- [ ] Implement CommandRouter with `core.echo`, `presence.get`, and explicitly simulated desktop handlers; no side-effect executor is added.
- [ ] Implement frontend SSE parsing and command transport that preserve unknown additive fields.
- [ ] Run focused Rust and Vitest tests, then full Rust/frontend suites and build.
- [ ] Commit `feat(events): add shared product event spine` and `feat(commands): add shared command routing foundation`.

### Task 4: Resource core

**Files:**
- Create: `backend/src/shared/resource.rs`, `backend/src/db/resources.rs`, `backend/src/api/resources.rs`
- Modify: `backend/src/db/mod.rs`, `backend/src/server.rs`, `backend/src/api/mod.rs`
- Create: `frontend/src/api/resources.ts`, `resources.test.ts`

**Interfaces:**
- Produces: `Resource`, `ResourceService::ingest`, `POST /api/resources/ingest`, `GET /api/resources`, `GET /api/resources/:id`, `ingestResource(file)`.

- [ ] Write tests for name/path normalization, max body size, SHA-256, create-new managed storage, DB round-trip, cleanup on failure, and `resource.created` emission; require RED.
- [ ] Implement metadata persistence through migration `0001`, safe managed storage, and metadata-only query APIs.
- [ ] Implement browser `File` upload without persisting the original absolute path.
- [ ] Run focused tests plus full Rust/frontend tests and build.
- [ ] Commit `feat(resources): add shared resource ingestion core`.

### Task 5: Voice, presence, narration, context, projections, and mocks

**Files:**
- Create: `backend/src/shared/voice.rs`, `context.rs`
- Create: `backend/src/api/voice.rs`, `projections.rs`
- Modify: `backend/src/server.rs`, `backend/src/api/mod.rs`
- Create: `frontend/src/api/voice.ts`, `projections.ts`
- Test: Rust shared/API tests and TypeScript contract tests.

**Interfaces:**
- Produces: provider-neutral `VoiceSession`, `STTProvider`, `TTSProvider`, deterministic providers, `PresenceSnapshot`, `NarrationRequest/Result`, `VoiceAttentionPolicy`, `ContextProvider`, `TaskProjection`, `DesktopContextProjection`, mock projection providers.

- [ ] Write lifecycle tests for start/stop/interrupt/cancel, invalid transitions, deterministic STT/TTS, layered presence, narration policies, bounded context, projection additive fields, and explicit mock metadata; require RED.
- [ ] Implement the minimal state machines/contracts and protected APIs; do not add audio streaming or cloud providers.
- [ ] Ensure voice emits product events but never invokes Task/Desktop methods directly.
- [ ] Run focused and full suites/build/check.
- [ ] Commit `feat(voice): establish voice and presence contracts` and `test(foundation): add cross-domain contract mocks`.

### Task 6: CI, architecture docs, ownership, and handoffs

**Files:**
- Create: `.github/workflows/shared-foundation.yml`
- Create: `docs/architecture/shared-foundation.md`, `surface-boundary.md`, `event-command-contract.md`, `resource-model.md`, `voice-presence.md`, `v1-v2-boundary.md`
- Create: `docs/development/parallel-development.md`, `ownership.md`, `integration-gates.md`, `migration-policy.md`, `handoff-v1.md`, `handoff-v2.md`

**Interfaces:**
- Produces: Windows Rust/frontend/Tauri CI; frozen/evolving zones; rolling integration; L0-L3 maturity; Gate 0-3 commands; immutable tag-based handoff.

- [ ] Add CI jobs for Rust fmt/check/test, frontend `npm ci`/test/build, and Windows Tauri check without NSIS packaging.
- [ ] Document every mandatory architecture invariant, event/command/resource/voice contract, restricted WebView rule, ownership path, migration range, and mock replacement rule.
- [ ] Document v1 and v2 handoffs using the immutable `shared-foundation-v1-v2` tag and exact commands to verify its commit.
- [ ] Search docs for forbidden scope drift and ambiguous mock-success language; fix all findings.
- [ ] Commit `ci: add shared foundation validation` and `docs: document v1 v2 parallel development contract`.

### Task 7: Final integration, cleanup, immutable handoff, and delivery

**Files:**
- Create: `backend/tests/shared_foundation.rs`
- Create: `frontend/src/surfaces/gate0.test.ts`
- Update: documentation only for verified command/results if needed.

**Interfaces:**
- Produces: automated Gate 0 harness, exact final report evidence, clean pushed branch, immutable foundation tag.

- [ ] Add a real router-level Gate 0 integration test covering Event, Command, Resource, Presence, and mock projections with the existing control session.
- [ ] Run `cargo fmt`, `cargo fmt --check`, `cargo check`, `cargo test --workspace --all-targets`, frontend `npm.cmd test`, frontend `npm.cmd run build`, and `cargo check -p yi-lian-qian-yan` fresh.
- [ ] Run live backend `/api/health` and protected-route smoke plus Windows GUI startup smoke; classify unavailable environmental checks as `BLOCKED`, never as PASS.
- [ ] Verify fresh/v0.9/idempotent migration tests individually and Gate 0 individually.
- [ ] Inspect `git diff`, tracked files, forbidden features, secret patterns, and architecture invariants.
- [ ] Remove only ignored/rebuildable cache output (`frontend/dist`, project `target`, and task-created temporary smoke directories) after recording validation evidence; preserve dependencies, databases, installer deliverables, and user files unless separately proven redundant.
- [ ] Commit the final harness, confirm clean status, create annotated tag `shared-foundation-v1-v2`, push the branch and tag, and verify remote refs match local SHAs.
- [ ] Output the exact `AI-0 SHARED FOUNDATION REPORT` sections with PASS/FAILED/PARTIAL/BLOCKED and both handoffs, then stop.
