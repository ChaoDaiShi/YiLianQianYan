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
| R1 | Default conversation and canvas entry | `frontend/src/components/chat/ChatView.tsx`, `frontend/src/pages/TaskCenterPage.tsx`, `frontend/src/features/task-world/`, `backend/src/api/task_world.rs`, `backend/src/task/planner.rs` | Empty/nonempty creation, current-task avatar, strict planning plus real-backend two-graph browser E2E passed | Final Tauri navigation and visual check | HUMAN_PENDING | Complete the combined V1-H navigation/canvas observation. |
| R2 | Task, project, history, execution and result convergence | Task Center/Detail, Workspaces, execution trail, Resource/Artifact panels and projection APIs | Full projection/attempt/artifact suite and browser history entry path passed | Final usability check | HUMAN_PENDING | Confirm representative non-empty history and cross-links in V1-H. |
| R3 | Reliable graph editing and execution | `backend/src/task/`, `backend/src/db/task_canvas.rs`, migrations 1000-1003/1013, `frontend/src/features/task-world/` | Full Harness/recovery/cancellation/edit/migration suite plus browser node/auto-layout/two-graph path passed | Representative execution and visual grouping path in Tauri | HUMAN_PENDING | Complete V1-H Task execution and group/collapse visual interaction; live AI review needs the configured model. |
| R4 | Global voice and custom voice | `backend/src/voice/`, `backend/src/interaction/`, `backend/src/api/voice.rs`, `frontend/src/features/voice/`, Settings voice section | Current RC full voice/provider/anchor/generation/interruption/approval suite passed | Real microphone, audible playback, direct-speech interruption and voice selection | HUMAN_PENDING | Complete V1-H H1-H4/H5 voice checks. |
| R5 | Files, context and artifacts | `backend/src/resource_input.rs`, Resource binding API/UI, `backend/src/task/context.rs`, artifact provenance/materialization/download/preview and Task panels | Current RC parser/binding/context/real persisted artifact suite passed | Representative file-to-task-to-artifact preview/export | HUMAN_PENDING | Complete V1-H H5; scanned-PDF OCR remains explicitly unsupported. |
| R6 | Capability center, imports and controlled improvement | Capability registry/pages, `capability/import_*`, protected import routes, managed Skill runtime owner | Current RC import/archive/CAS/rollback/Skill-owner tests and protected Capability page E2E passed | Import UX check | HUMAN_PENDING | Complete V1-H managed import interaction; declarative Plugin code remains inert by design. |
| R7 | System modules, configurable sidebar and monitoring toolbox | `capability/system_preferences.rs`, `components/system/`, NavRail, System page | Current RC validation/CAS/metrics/refresh/layout suite, first-run setup and System page E2E passed | Layout interaction check | HUMAN_PENDING | Complete V1-H H5 persistence/layout observation. |
| R8 | Memory-to-Skill workflow | `backend/src/memory_skill.rs`, `api/skill_candidates.rs`, candidate/version UI and managed Skill discovery | Current RC authorization/sensitivity/validation/version/rollback/discovery suite passed | Review/edit/confirm flow check | HUMAN_PENDING | Complete one authorized candidate-to-reuse interaction in V1-H. |
| R9 | Release quality | v1 profile, Tauri host, v1 migration registry, CI/build configuration and release docs | Full frontend/Rust/Tauri/migration/E2E gate, boundary scans, RC build/hash and packaged GUI automation passed | Installer, shutdown/recovery and lightweight-device observation | HUMAN_PENDING | Complete V1-H H6; installer is an unsigned RC, not a formal Release. |

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
