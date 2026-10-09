# R0 — Current Architecture Inventory

> **Stage:** R0 (Inventory) — read-only. No production code is changed by this document.
> **Branch:** `refactor/v1-architecture-foundation`
> **Base:** `origin/v1/release-work` @ `78f3755c7cd680d53990d006c4b60dbf3e2622a3`
> ("docs(release): record rc2 packaged acceptance", 2026-09-18)
> **Product candidate under refactor:** v1.0.0-rc.2 (frozen)
> **Toolchain:** cargo/rustc 1.97.1, Node/Vite 5, React 18, Tauri 2

This document is the factual baseline the R1–R8 refactor stages are measured
against. It records *what exists today*, not what should exist.

---

## 1. Baseline size

| Surface | Files | LOC |
|---|---:|---:|
| Backend (`backend/src/**/*.rs`) | 240 | 82,283 |
| Frontend (`frontend/src/**/*.ts{,x}`) | — | 26,848 |

Build note: the repository intentionally runs Cargo serialized
(`.cargo/config.toml` → `[build] jobs = 1`) because of the documented
Windows low-memory dev-build constraint
(`docs/superpowers/plans/2026-08-14-windows-low-memory-dev-build.md`).
Cold builds are therefore slow and full gates are expensive; dev-stage
verification must stay focused (see §9).

---

## 2. Current backend directory layout

```
backend/src/
├── agent/          14    ReAct engine, subagent runtime
├── api/            33    HTTP transport: 178 route registrations in mod.rs
├── capability/     12    Capability registry + providers
├── config/          2    AppConfig / ModelConfig types
├── db/             20    SQLite persistence (7,925 LOC, ~45 tables)
├── execution/       5    Execution records
├── interaction/     7    Voice dispatch / approval adapter
├── isolation/       2    Process isolation (windows.rs 709)
├── llm/             5    OpenAI-compatible client
├── mcp_runtime/    16    MCP session/manager runtime
├── plugin/          5    Plugin discovery
├── resource_input/  1    (+ resource_input/documents.rs) Resource ingestion
├── safety/         29    Security kernel: gateway, policy, rbac, audit, sandbox
├── secret/          8    Secret store / resolver / migration
├── shared/          8    Shared foundation: event, command, context, resource, voice
├── task/           30    Task World: supervisor, runtime, projection, harness
├── tools/          16    Tool registry + builtin tools
├── utils/           3    Leaf helpers
├── voice/           4    Voice runtime + providers
├── workflow/        9    Workflow graph + runtime
├── workspace/       4    Workspace model/service
├── lib.rs                Crate root: module list + server bootstrap
├── main.rs               Standalone binary entry
├── mcp.rs          1384  MCP protocol (single file)
├── memory_skill.rs  615  Memory-to-Skill candidate pipeline (single file)
├── server.rs       1160  Composition root: AppServer + bootstrap + helpers
└── skill_management.rs 220 Managed skill store
```

## 3. Current frontend directory layout

```
frontend/src/
├── App.tsx              BrowserRouter + ThemeProvider + AppRoot
├── api/                 legacy.ts (1335) + per-domain clients (taskWorld 601, voice 536, …)
├── components/          chat/, tasks/, workflow/, approval/, capabilities/, system/, layout/, ui/
├── features/            capabilities, execution, llm, mcp, memory, resources, security,
│                        skills, task-world, tasks, voice
├── pages/               PluginsPage(973), SettingsPage(726), TaskCenterPage(512), …
├── stores/              Zustand stores
├── surfaces/            AppRoot.tsx, surfaceHost.ts, workspace/WorkspaceSurface.tsx, desktop/
├── theme/               presets.ts (336), ThemeProvider.tsx
├── types/
└── index.css            Monolithic global stylesheet
```

Frontend routing is path-compatible and owned by
`surfaces/workspace/WorkspaceSurface.tsx`
(`chat`, `chat/:id`, `tasks`, `task-world/:graphId`, `system`, `logs`, `skills`,
`plugins`, `workflows`, `workspaces`, `workspaces/:id`, `agents`, `capabilities`,
`memory`, `knowledge`, `settings`, `*`).

---

## 4. TOP 15 hotspot files

### Backend

| # | File | LOC | Why it is a hotspot |
|---|---|---:|---|
| 1 | `safety/execution_gateway.rs` | 3049 | **Protected Kernel.** Sole security execution path. |
| 2 | `task/runtime.rs` | 2910 | Task World runtime + persistence coordination |
| 3 | `api/approvals.rs` | 2093 | Approval transport + state machine in one file |
| 4 | `workflow/tests.rs` | 1737 | Test mass (not production weight) |
| 5 | `api/memories.rs` | 1611 | Memory HTTP + query logic |
| 6 | **`api/task_world.rs`** | **1514** | graph/node/canvas/execution/review/checkpoint/rerun/projection in one HTTP file |
| 7 | **`mcp.rs`** | **1384** | Protocol + transport + runtime + registry in one file |
| 8 | **`api/settings.rs`** | **1365** | Settings HTTP + secret lifecycle + provider test + readiness |
| 9 | `task/orchestrator.rs` | 1216 | Multi-step task orchestration |
| 10 | `voice/runtime.rs` | 1195 | Global voice session runtime |
| 11 | **`server.rs`** | **1160** | Composition + state + helpers + agent parsing |
| 12 | `db/task.rs` | 1107 | Task persistence |
| 13 | `task/task_supervisor.rs` | 1087 | Task supervision |
| 14 | `voice/provider.rs` | 1067 | STT/TTS provider integration |
| 15 | `agent/engine.rs` | 1042 | ReAct loop |

Bold rows are the hotspots named in the refactor mandate (§7).

### Frontend

| # | File | LOC | Why it is a hotspot |
|---|---|---:|---|
| 1 | `api/legacy.ts` | 1335 | Pre-facade client retained as compatibility surface |
| 2 | `pages/PluginsPage.tsx` | 973 | Page doing its own data + view + forms |
| 3 | `features/voice/GlobalVoiceHost.tsx` | 895 | Runtime + context + routing + rendering |
| 4 | `pages/SettingsPage.tsx` | 726 | Page doing all sections inline |
| 5 | `components/chat/ChatView.tsx` | 623 | Chat view + streaming + tool cards |
| 6 | `api/taskWorld.ts` | 601 | Task World client |
| 7 | `features/voice/useVoiceCapture.ts` | 557 | Capture lifecycle |
| 8 | `features/execution/reducer.ts` | 536 | Execution projection reducer |
| 9 | `api/voice.ts` | 536 | Voice client |
| 10 | `pages/TaskCenterPage.tsx` | 512 | Task centre page |
| 11 | `features/task-world/TaskWorldInspector.tsx` | 397 | Inspector (all sections inline) |
| 12 | `pages/WorkflowsPage.tsx` | 388 | Workflow page |
| 13 | `components/tasks/TaskDetailPanel.tsx` | 386 | Task detail |
| 14 | `features/task-world/TaskWorldPage.tsx` | 368 | Page + composition + wiring |
| 15 | `features/execution/ExecutionHistory.tsx` | 355 | Execution history view |

---

## 5. Current module dependency edges

Derived from `use crate::<module>` across each module directory.

```
agent          -> config db llm safety secret server task tools utils
api            -> agent capability config db execution llm mcp mcp_runtime safety
                  secret server shared skill_management task tools utils voice
                  workflow workspace
capability     -> db mcp_runtime server skill_management task tools
config         -> (leaf)
db             -> config execution secret shared task workflow workspace
execution      -> task
interaction    -> db safety shared task voice
isolation      -> utils
llm            -> config db secret
mcp_runtime    -> secret utils
plugin         -> (leaf)
resource_input -> (leaf)
safety         -> agent config db mcp_runtime tools
secret         -> config db safety server
shared         -> db
task           -> agent capability config db execution llm safety secret server
                  shared tools utils workflow workspace
tools          -> db isolation mcp safety server utils
utils          -> (leaf)
voice          -> config secret shared
workflow       -> config db execution llm safety secret tools
workspace      -> db
```

### 5.1 Dependency violations to remove

These are the concrete seams the refactor must cut. None of them are
behavioural; all are structural.

| Violation | Detail | Target |
|---|---|---|
| **Domain → composition root** | `task`, `capability`, `tools`, `agent`, `secret`, `safety`, `workflow` all import `crate::server` | move `AppServer` state to `app::state`; non-API modules depend on `app::state`, never on transport |
| **`db` → domain** | `db` imports `task`, `workflow`, `workspace`, `execution` | `db` becomes an infrastructure adapter behind module-owned repositories |
| **`api` → 20 modules** | `api/mod.rs` alone registers 178 routes and names every module | per-module route builders composed by `app::router` |
| **`safety` → `mcp_runtime`, `agent`** | kernel reaches into runtime layers | invert via ports/descriptors |
| **Monolith files** | `api/task_world.rs`, `api/settings.rs`, `mcp.rs`, `server.rs` | split per §7 hotspots |

`crate::server` importer counts by module (37 files total):
`api` 29, `agent` 2, and 1 each in `workflow`, `tools`, `task`, `secret`,
`safety`, `capability`.

Externally referenced `server::` symbols:
`AppServer` (36), `DiscoveredSubagent` (5), `mcp_transport_config` (4),
`LogBuffer` (3), `LogEntry`, `CHAT_MEMORY_TOP_K`.

---

## 6. State owners today

| State | Owner | Notes |
|---|---|---|
| `AppServer` (composition root state bag) | `server.rs` | 1160 LOC; also holds bootstrap + helpers |
| Cancellation tokens | `AppServer.active_tasks`, `active_workflow_runs`, `active_task_executions` | one per lifecycle family |
| Approvals | `AppServer.approval_store` (`safety::approval::ApprovalStore`) | |
| Audit | `AppServer.audit_recorder` | |
| Control-plane session | `AppServer.control_session` | `safety::control_session` |
| Capability registry | `AppServer.capability_registry` (lazy `RwLock<Option<..>>`) | |
| MCP runtime | `AppServer.mcp_runtime_manager` + `mcp_runtime/` | |
| Secrets | `AppServer.secret_store` / `secret_resolver` | single OS-backed instance |
| Events | `AppServer.event_hub` (`shared::event::EventHub`) | |
| Commands | `AppServer.command_router` (`shared::command::CommandRouter`) | |
| Resources | `AppServer.resource_service` (`shared::resource::ResourceService`) | |
| Voice | `AppServer.voice_runtime` (`GlobalVoiceSessionRuntime`) | |
| Task World | `AppServer.task_world` (`TaskWorldRuntime`) | authoritative v1 registry |
| Managed processes | `AppServer.managed_process_registry` | application-lifetime |
| Router | `api/mod.rs::build_router` | 546 LOC, 178 routes |
| Frontend server state | `stores/` (Zustand) | |
| Frontend route ownership | `surfaces/workspace/WorkspaceSurface.tsx` | |

---

## 7. Public API surface (must remain compatible)

178 route registrations, all composed in `api/mod.rs::build_router`.
Grouped by prefix:

```
task-world 20   voice 17   memories 13   tasks 11   mcp 9   security 7
llm 7   capabilities 7   workflows 6   workflow-graphs 6   plugins 6
workspaces 5   skills 5   skill-candidates 5   resources 5   artifacts 5
approvals 5   system 4   conversations 4   workflow-runs 3
managed-skill-versions 3   agents 3   agent-teams 3   task-decisions 2
settings 2   resource-bindings 2   logs 2   chat 2   tools 1   subagents 1
secrets 1   providers 1   projections 1   presence 1   health 1
```

Non-HTTP entrypoints:

| Symbol | Location | Consumers |
|---|---|---|
| `create_server()` | `lib.rs` | Tauri embedding, standalone binary |
| `serve(addr)` / `serve_in_background*()` | `lib.rs` | `main.rs`, `src-tauri` |
| `AppServer::new*()` | `server.rs` | `lib.rs`, tests |
| `api::build_router(Arc<AppServer>)` | `api/mod.rs` | `lib.rs` |
| `chat_handler` / `stop_handler` | `api/mod.rs` re-export | route composition |

---

## 8. Database ownership

SQLite via `rusqlite` (bundled). 7,925 LOC across 20 modules in `db/`,
~45 tables. Versioned migration ownership already exists in
`db/migrations.rs`:

| Namespace | Range | Owner enum |
|---|---|---|
| Shared/Core | `0–999` | `MigrationOwner::Shared` |
| v1 Task World | `1000–1999` | `MigrationOwner::V1TaskWorld` |

> **Discrepancy with the refactor mandate:** the mandate (§27) states a v2
> namespace `2000–2999` also exists at base. It does **not** exist on
> `v1/release-work@78f3755` — `MigrationOwner` has only `Shared` and
> `V1TaskWorld`. This is recorded, not acted upon: R1–R8 add **no**
> migrations and renumber **nothing**.

Table ownership by `db/` module:

| Module | LOC | Tables owned |
|---|---:|---|
| `db/mod.rs` | 627 | `Database` handle, connection, bootstrap SQL |
| `db/migrations.rs` | 602 | `schema_migrations` (+ namespace enforcement) |
| `db/task.rs` | 1107 | `tasks`, `task_plans`, `task_executions`, `task_node_executions`, `task_decisions`, `task_events` |
| `db/task_world.rs` | 638 | `task_world_state`, `task_world_checkpoints`, `task_world_execution_controls`, `task_world_supervisor_snapshots`, `task_world_seed` |
| `db/task_execution.rs` | 577 | `task_node_executions` (execution side) |
| `db/task_canvas.rs` | 446 | `task_canvas_views`, `task_graph_revision_history` |
| `db/memories.rs` | 775 | `memories` |
| `db/conversations.rs` | 620 | `conversations`, `messages`, `conversation_execution_records` |
| `db/workflow_runtime.rs` | 689 | `workflow_runs`, `workflow_graphs` |
| `db/workflows.rs` | 133 | `workflows` |
| `db/security_audit.rs` | 437 | `security_audit_events`, `security_approvals`, `security_grants`, `security_subjects`, `security_role_bindings` |
| `db/llm_models.rs` | 345 | `llm_models`, `llm_usage_events` |
| `db/resource_bindings.rs` | 245 | `resource_bindings` |
| `db/resources.rs` | 108 | `resources` |
| `db/mcp.rs` | 173 | `mcp_servers` |
| `db/artifact_provenance.rs` | 93 | `artifacts`, `artifact_provenance` |
| `db/skill_candidates.rs` | 43 | `skill_candidates`, `skill_candidate_revisions` |
| `db/workspace.rs` | 131 | `workspaces` |
| `db/settings.rs` | 49 | `settings` |
| `db/llm_models_tests.rs` | 87 | test-only |

Additionally owned outside `db/`: `agent_definitions`, `agent_executions`,
`agent_teams` (agent), `managed_skill_versions` (skill_management).
(`*_must_not_run*`, `digest_mismatch_*`, `pending_body_must_roll_back` are
migration-test negative fixtures, not real tables.)

---

## 9. Protected Kernel (semantics frozen for R1–R8)

Moving code is permitted. Changing semantics is not. If a contract change is
unavoidable, the stage must stop and emit
`ARCHITECTURE_CONTRACT_CHANGE_REQUIRED`.

| Concept | Location |
|---|---|
| `SecurityExecutionGateway` | `safety/execution_gateway.rs`, `safety/mod.rs:36` |
| Approval | `safety/approval.rs` |
| Event Envelope | `shared/event.rs` |
| Command Envelope | `shared/command.rs` |
| Resource Identity | `shared/resource.rs`, `docs/architecture/resource-model.md` |
| Presence | `shared/voice.rs`, `docs/architecture/voice-presence.md` |
| Capability security semantics | `safety/capability.rs`, `safety/policy_engine.rs` |
| Migration ownership | `db/migrations.rs` (`MigrationOwner` ranges) |

Existing safety semantics that must survive verbatim (from the mandate §9):

```
missing key        -> KEEP
empty key          -> KEEP
non-empty new key  -> REPLACE
explicit clear     -> DELETE
```

Loop guards that must not be weakened: `MAX_ITERATIONS`,
`MAX_CONSECUTIVE_SAME_TOOL`, `CancellationToken`.

---

## 10. Verification budget

Full gates are expensive under `jobs = 1`. Stage policy:

| Stage | Verification |
|---|---|
| R0 | none (read-only) |
| R1–R7 | focused: `cargo check -p yilian-backend --all-targets` + focused `cargo test` filters; frontend: focused `vitest` + `npm run build` |
| R8 | exactly one full gate: `cargo fmt --all -- --check`, `cargo check --workspace --locked -j1`, `cargo test --workspace --all-targets --locked -j1`, `cargo check -p yi-lian-qian-yan --locked -j1`, `npm test`, `npm run build`, real-backend E2E |

> Confirmed: the workspace members are `backend` (`yilian-backend`) and
> `src-tauri` (`yi-lian-qian-yan`), so the mandate's
> `cargo check -p yi-lian-qian-yan` is the Tauri shell crate and is valid.

---

## 11. Repository conventions that affect this refactor

| Convention | Detail |
|---|---|
| `docs/` is gitignored (`docs/*`) but tracked | new docs must be added with `git add -f`; 94 docs are already tracked this way |
| Protected branches | `main`, `develop`, `v1/release-work` — never merged into, never pushed |
| Cargo serialization | `.cargo/config.toml` → `jobs = 1` (low-memory guard; do not "fix") |
| Commit style | Conventional Commits, small and scoped |
| File moves | use `git mv` to preserve history |
| v2 | `LOCAL_FROZEN` — untouched by this refactor |

---

## 12. Current → Target map

| Current | Target (R-stage) |
|---|---|
| `server.rs` (composition + state + helpers) | `app/{state,bootstrap,router,lifecycle}.rs` (R1) |
| `agent` definition parsing inside `server.rs` | `agent/definition.rs` (R1) |
| `api/settings.rs` (1365) | `modules/settings/{domain,application,api,infrastructure}` (R2) |
| `api/task_world.rs` (1514) | `modules/task/{domain,application,api}` (R3) |
| `api/workflow_runtime.rs` (810) | `modules/workflow/{domain,application,api}` (R3) |
| `mcp.rs` (1384) + `mcp_runtime/` | `integrations/mcp/{protocol,transport,runtime,registry,result,security}` (R4) |
| `resource_input.rs` + `resource_input/` | `modules/resource/{parsers,…}` (R5) |
| `memory_skill.rs` + `skill_management.rs` | `modules/memory_skill/*` (R5) |
| `api/mod.rs` build_router (178 routes) | per-module route builders composed by `app/router.rs` (R1–R5) |
| `features/task-world/TaskWorldPage.tsx` | `features/task-world/{pages,canvas,inspector,execution,hooks,model}` (R6) |
| `features/voice/GlobalVoiceHost.tsx` | `features/voice/{runtime,hooks,context,ui,model}` (R6) |
| `pages/SettingsPage.tsx` | `features/settings/{pages,sections,hooks,model}` (R6) |
| `index.css` | `styles/{tokens,base,shell}.css` + `styles/features/*` (R6) |
| — (absent) | `docs/architecture/{module-boundaries,dependency-rules,ownership,current-to-target-map}.md` + boundary checks (R7) |

Target directory names are the mandate's frozen target (§4, §17). This map is
the working plan; the R-stage checkpoints record what was actually done.

---

## 13. Known risks

1. **Cold build cost.** No `target/` cache exists in the fresh worktree;
   `jobs = 1` makes the first full compile slow. Stage verification is
   therefore focused, with exactly one full gate at R8.
2. **`api/mod.rs` is the single composition point** for 178 routes; every
   R1–R5 stage touches it. It must be changed additively (route builders
   extracted first, then delegated to) to keep each commit green.
3. **`crate::server` has 37 importers** across 8 modules. R1 must leave a
   compatibility facade (`pub use app::state::AppServer;`) so all 37 keep
   compiling; call sites migrate incrementally, not in one commit.
4. **`db` → domain imports** cannot be cut in this round without touching
   repository boundaries broadly; R1–R5 scope it to "no *new* db→domain
   edges" and record the remainder as debt in the final report.
5. **`docs/*` ignore rule** means a forgotten `git add -f` silently drops a
   required deliverable. Every docs commit must be verified with
   `git show --stat`.
6. **Protected Kernel proximity.** `safety/execution_gateway.rs` (3049) is the
   largest file but is *not* a mandated hotspot; it is touched only where a
   move requires an import path update, never semantically.
