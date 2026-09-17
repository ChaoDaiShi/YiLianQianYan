# v1.0 requirements and evidence matrix

This file is the authoritative v1 release checklist. A status is evidence-based:

- `TODO`: implementation has not started.
- `IMPLEMENTING`: code exists, but the complete requirement is not yet verified.
- `AUTO_VERIFIED`: the complete automatable portion passed on the current candidate.
- `HUMAN_PENDING`: automation passed; specified physical or perceptual checks remain.
- `ACCEPTED`: automation and required human checks both passed.
- `BLOCKED`: a concrete external prerequisite prevents progress.

P0 established an independent v1 lineage from the public Foundation. The default product profile is `v1`, its database is isolated, and no Windows observation/control/launch provider or DesktopSpace runtime is initialized. Historical evidence is not treated as current-branch verification.

| ID | Requirement | Implementation paths and entry | Automated evidence | Human dependency | Status | Remaining work |
| --- | --- | --- | --- | --- | --- | --- |
| R1 | Default conversation and canvas entry | `frontend/src/components/chat/ChatView.tsx`, `frontend/src/pages/TaskCenterPage.tsx`, `frontend/src/features/task-world/`, `backend/src/api/task_world.rs`, `backend/src/task/planner.rs` | Empty/nonempty Task Center creation, current-task avatar routing, strict natural-language graph planning and graph identity/revision tests | Final Tauri navigation and visual check | IMPLEMENTING | Run the current candidate through the real-backend browser path and final Tauri navigation check. |
| R2 | Task, project, history, execution and result convergence | Task Center/Detail, Workspaces, execution trail, Resource/Artifact panels and projection APIs | Workspace/task/execution projection, attempt history and artifact provenance tests | Final usability check | IMPLEMENTING | Verify normal-mode cross-links and non-empty history behavior in browser/Tauri on the candidate. |
| R3 | Reliable graph editing and execution | `backend/src/task/`, `backend/src/db/task_canvas.rs`, migrations 1000-1003/1013, `frontend/src/features/task-world/` | Real Harness, cancellation, active-edit guard, atomic rerun/restore, CanvasView, multiselect, persisted groups/collapse, dependency-aware auto-layout and revision-bound AI suggestion tests | Representative execution and visual grouping path in Tauri | IMPLEMENTING | Run current-candidate browser E2E and representative Tauri execution; AI review still requires a configured real model to exercise its provider call. |
| R4 | Global voice and custom voice | `backend/src/voice/`, `backend/src/interaction/`, `backend/src/api/voice.rs`, `frontend/src/features/voice/`, Settings voice section | Global host/anchor/generation, bounded MiniMax capture, provider-final dispatch, hands-free interruption, echo guard and approval-attestation tests | Real microphone, audible playback, direct-speech interruption and voice selection | IMPLEMENTING | Re-run the voice technical targets at RC, then move to `HUMAN_PENDING` for V1-H microphone/hearing/interruption checks. |
| R5 | Files, context and artifacts | `backend/src/resource_ingest.rs`, Resource binding API/UI, `backend/src/task/context.rs`, artifact provenance/materialization/download/preview and Task panels | Bounded parser, binding CAS, context selection and persisted artifact tests | Representative file-to-task-to-artifact preview/export | IMPLEMENTING | Run one real browser/Tauri file-to-download flow; scanned-PDF OCR remains explicitly unsupported. |
| R6 | Capability center, imports and controlled improvement | Capability registry/pages, `capability/import_*`, protected import routes, managed Skill runtime owner | GitHub/ZIP/MD validation, inert preview/confirm, CAS lifecycle, rollback, archive hardening and owned Skill activation tests | Import UX check | IMPLEMENTING | Exercise the protected HTTP/UI lifecycle in browser; declarative Plugin packages remain inert and do not load arbitrary React/native code. |
| R7 | System modules, configurable sidebar and monitoring toolbox | `capability/system_preferences.rs`, `components/system/`, NavRail, System page | Mandatory entry validation, revision-CAS preferences, real metrics conversion, visibility-aware refresh and layout tests | Layout interaction check | IMPLEMENTING | Browser/Tauri interaction check for setup, sidebar ordering/visibility and grid/free monitoring layouts. |
| R8 | Memory-to-Skill workflow | `backend/src/memory_skill.rs`, `api/skill_candidates.rs`, candidate/version UI and managed Skill discovery | Source-linked candidate authorization, sensitivity checks, validation, confirm, version, rollback and deactivate tests | Review/edit/confirm flow check | IMPLEMENTING | Exercise one authorized completed-task candidate through confirmation and later reuse in browser/Tauri. |
| R9 | Release quality | v1 profile, Tauri host, v1 migration registry, CI/build configuration and release docs | Focused compile/build checks plus existing fresh/repeat/v0.9 migration and dependency audit evidence | Installer, shutdown/recovery and lightweight-device observation | IMPLEMENTING | Add/run real-backend browser E2E, align version/package identifiers, perform one current-SHA full gate, scan deliverables and build the RC artifact. |

## P0 automated baseline

The following checks are baseline evidence only; they do not close R1-R9:

- Rust/Tauri workspace compilation succeeded.
- Frontend TypeScript and production Vite build succeeded.
- The backend v1 profile uses a dedicated application data directory and `yilianqianyan-v1.db`.
- The v1 migration registrar accepts Shared and v1 1000-1999 ownership only; no v2 2000-2999 product migration is registered.
- Desktop provider references retained only as fail-closed compatibility abstractions report `unavailable`/`not installed`; no provider implementation or native startup is present.
- MiniMax cloud voice, SecretStore references, Task Harness, Workflow, CanvasView, Resources, Event/Command/Presence and safety/approval integrations are present for focused verification in P1-P3.

Exact command results and candidate SHA are recorded in the release verification report when a candidate is created.

## Focused implementation evidence before RC

Development checks are deliberately narrow until the candidate is frozen:

- P1 integration commit `30c3400d31b385978c0853b2a34bd6d51df5bc40` had a full Rust/frontend gate before later P2/P3 changes.
- P2 Resource/Artifact/Memory and Capability/System branches supplied their focused service and UI test evidence; the controller independently rebuilt the integrated frontend and checked the backend after wiring protected routes.
- Functional commit `0209a50` adds migration 1013, persisted visual groups/collapse, dependency-aware auto-layout and revision-bound AI review suggestions. The affected four frontend targets passed 23 tests, the production frontend build passed, backend formatting/check passed, and the new parser/migration/projection targets passed individually.
- These focused runs are not represented as the final V1-RC full gate. A single complete current-candidate gate remains required after P3 code and packaging stop changing.
