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
| R1 | Default conversation and canvas entry | `frontend/src/pages/ChatPage.tsx`, `frontend/src/pages/TaskCenterPage.tsx`, `frontend/src/features/task-world/`, `backend/src/api/task_world.rs` | Backend Task World route tests; frontend Task Center and canvas contract tests | Final Tauri navigation and visual check | IMPLEMENTING | Fix zero/nonzero graph creation entry, avatar-to-current-graph routing, and add strict Planner-to-TaskGraph flow. |
| R2 | Task, project, history, execution and result convergence | `frontend/src/pages/TaskCenterPage.tsx`, `frontend/src/pages/WorkspacesPage.tsx`, `frontend/src/components/execution/`, projection APIs | Existing workspace/task/execution presentation tests | Final usability check | IMPLEMENTING | Complete cross-links from conversations, files and artifacts to tasks/executions; remove empty-only history surfaces. |
| R3 | Reliable graph editing and execution | `backend/src/task/`, `backend/src/db/task_canvas.rs`, `backend/src/db/task_execution.rs`, `frontend/src/features/task-world/` | Task supervisor, Harness, CanvasView, revision, checkpoint, persistence and UI contract tests | Representative execution path in Tauri | IMPLEMENTING | Add grouping/collapse, multiselect operations, auto-layout, user-invoked AI review, and safe invalidation/recovery workflow. |
| R4 | Global voice and custom voice | `backend/src/voice/`, `backend/src/interaction/`, `backend/src/api/voice.rs`, `frontend/src/features/voice/`, Settings voice section | MiniMax STT/TTS, lease, generation, bounded capture, final-only dispatch and continuation tests | Real microphone, audible playback, direct-speech interruption and voice selection | IMPLEMENTING | Preserve anchor across ordinary navigation, implement explicit hands-free speech-start interruption, echo/noise handling and voice preview/speed controls. |
| R5 | Files, context and artifacts | Resource APIs and UI, `backend/src/task/context.rs`, task resource references | Resource ingestion and task context tests | Representative file-to-artifact preview/export | IMPLEMENTING | Finish supported parser matrix, binding/status UI, provenance/version display and one verified downloadable artifact flow. |
| R6 | Capability center, imports and controlled improvement | Capability/Skill/MCP/plugin registries and pages | Registry, security descriptor, MCP and capability tests | Import UX check | IMPLEMENTING | Complete GitHub/ZIP/MD import lifecycle, archive hardening, version/update/uninstall and reviewable improvement candidates. |
| R7 | System modules, configurable sidebar and monitoring toolbox | Capability/System pages and navigation composition | System health, navigation and presentation tests | Layout interaction check | IMPLEMENTING | Add trusted module declarations, guarded enablement, configurable safe sidebar regions and persistent monitoring layouts. |
| R8 | Memory-to-Skill workflow | Memory APIs/pages, memory extraction and capability registry | Memory extraction, redaction, retrieval and persistence tests | Review/edit/confirm flow check | IMPLEMENTING | Add source-linked candidates, explicit validation, Skill version creation, rollback and reuse from Capability Center. |
| R9 | Release quality | v1 profile in `backend/src/lib.rs`, Tauri host, migration registry, build configuration | Fresh/repeat/v0.9 migration tests, Rust/Tauri check, frontend production build | Installer, shutdown/recovery and lightweight-device observation | IMPLEMENTING | Complete browser E2E, migration damage refusal, dependency reachability review, packaging/version alignment and RC artifacts. |

## P0 automated baseline

The following checks are baseline evidence only; they do not close R1-R9:

- Rust/Tauri workspace compilation succeeded.
- Frontend TypeScript and production Vite build succeeded.
- The backend v1 profile uses a dedicated application data directory and `yilianqianyan-v1.db`.
- The v1 migration registrar accepts Shared and v1 1000-1999 ownership only; no v2 2000-2999 product migration is registered.
- Desktop provider references retained only as fail-closed compatibility abstractions report `unavailable`/`not installed`; no provider implementation or native startup is present.
- MiniMax cloud voice, SecretStore references, Task Harness, Workflow, CanvasView, Resources, Event/Command/Presence and safety/approval integrations are present for focused verification in P1-P3.

Exact command results and candidate SHA are recorded in the release verification report when a candidate is created.
