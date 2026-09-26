# R1 Architecture Refactor — Progress and Handoff

> **Branch:** `refactor/v1-architecture-foundation`
> **Base:** `origin/v1/release-work` @ `78f3755`
> **Status:** **PARTIAL — R0, R1, R2 complete and verified. R3–R8 not started.**
>
> This is *not* the `r1-final-report.md` the mandate §44 asks for. That document
> requires the R8 full gate to have passed. This is an honest handoff so the
> remaining stages can continue from a green, committed state.

---

## 1. Commits on this branch

| Commit | Stage | Message |
|---|---|---|
| `63836ce` | R0 | `docs(refactor): inventory current architecture` |
| `ffeaf84` | R1 | `refactor(app): split application composition` |
| `aa42219` | R2 | `refactor(settings): isolate settings domain` |

Working tree is clean; `main`, `develop`, `v1/release-work`, and `v2` are untouched.

---

## 2. Build environment — read this first

The machine cannot build this repository the obvious way.

| Constraint | Detail |
|---|---|
| **F: is 100% full** | 466 GB drive, ~790 MB free. `cargo check` works; `cargo test` dies with `os error 112 磁盘空间不足` because codegen needs several GB more. |
| **Use a target dir on E:** | Every cargo command must set `CARGO_TARGET_DIR`. This is what all verification below used: |

```bash
cd <worktree>/backend
CARGO_TARGET_DIR="E:/cargo-target/yilian-arch" cargo test -p yilian-backend --locked
```

| Other drives | C: 18.5 GB · D: 11.7 GB · E: 19.5 GB (all NTFS fixed) · G: 122 GB but **FAT32 removable — unusable for Cargo** |
|---|---|
| **RAM** | 15.2 GB total, ~1.7 GB free during a build. The repo's `.cargo/config.toml` `jobs = 1` is **required**, not incidental — parallelizing OOMs. Do not "fix" it. |
| **Cost** | ~60 min cold, ~3 min incremental. Budget accordingly; keep dev-stage verification focused. |

The worktree was created as:

```bash
git worktree add -b refactor/v1-architecture-foundation \
  "F:/项目开发/忆涟千言/YiLian-v1-architecture" 78f3755c7cd680d53990d006c4b60dbf3e2622a3
```

Note `.cargo/config.toml` must be present in the worktree (it is tracked, so a
fresh worktree already has it).

---

## 3. Verification performed

Both stages were verified with the same commands, against the E: target dir:

| Command | R1 | R2 |
|---|---|---|
| `cargo check -p yilian-backend --all-targets --locked` | pass | pass |
| `cargo fmt --all -- --check` | clean | clean |
| `cargo test -p yilian-backend --locked` | 892 + 20 targets, 0 failed | 892 + 20 targets, 0 failed |
| New warnings introduced | 0 (3 pre-existing remain) | 0 (3 pre-existing remain) |

Pre-existing warnings, unchanged by this work:
`apply_product_migration` never used, `validate_shared_version` never used,
`q_embed` unused variable.

Moved tests confirmed executed, not skipped:
- R1 — 15 tests: `app::state::tests::*` (4) + `agent::definition::tests::*` (10) +
  `app::state::tests::runtime_registry_includes_builtins_and_executable_subagent`
- R2 — 11 tests: `modules::settings::tests::*`, including the four that pin the
  rc.2 secret semantics.

---

## 4. R1 — what was done

`server.rs` (1160 lines) was state, assembly, routing and helpers at once.

| New file | Contents | Lines |
|---|---|---|
| `app/state.rs` | `AppServer` + 19 accessors, `LogBuffer`/`LogEntry`, `DiscoveredSubagent`, `mcp_transport_config` | 807 |
| `app/bootstrap.rs` | `AppServer::new*`, `create_server*`, `default_data_dir` | 248 |
| `app/lifecycle.rs` | `serve`, `serve_in_background*`, extracted `shutdown()` | 77 |
| `app/router.rs` | `build_router`, `require_control_session`, `control_plane_cors` | 517 |
| `agent/definition.rs` | `ParsedAgentDefinition` + AGENT.md parser + 10 tests | 212 |
| `server.rs` | compatibility facade (`pub use`) | 13 |
| `lib.rs` | 145 → 31 lines, re-exports preserved | 31 |

Call-site compatibility held: `crate::server::*` and `crate::api::build_router`
both still resolve, so all 37 `crate::server` importers and every existing
`build_router` call site (5 files: `api/tests.rs`, `api/chat.rs`,
`api/llm_models_tests.rs`, `api/artifact_download.rs`, `app/bootstrap.rs`)
compiled untouched.

### Deviations to be aware of

1. **`api/mod.rs` route ownership moved to `app/router.rs` in R1, not R3.**
   The mandate lists `app/router.rs` ("HTTP router composition") under R1, so
   the 178-route table moved with it. This required making the 30 `api`
   submodules `pub(crate)` — handlers were already `pub async fn`, so no
   handler signatures changed. If the intent was to keep the route table in
   `api/` until R3, this is the commit to revisit.

2. **`api/system.rs` — `HealthResponse` widened `pub(super)` → `pub(crate)`.**
   Required because the response type now crosses the `api` → `app` boundary.
   No wire-format change. This is the only visibility edit outside the pure-move
   set. Worth a second look if an API-compatibility audit is run.

3. **Git cannot pair renames.** `server.rs` and `api/mod.rs` both survive (as
   facade / transport module), so git records the moved code as new files
   rather than renames. This is inherent to the strangler-facade strategy the
   mandate §25 prescribes. Moved regions are byte-identical, so `git log -S`
   and `git blame` still attribute the original authorship.

---

## 5. R2 — what was done

`api/settings.rs` (1365 lines) split into the first `modules/` domain module.

```
backend/src/modules/
├── mod.rs
└── settings/
    ├── mod.rs
    ├── domain/{mod,policy,readiness}.rs
    ├── application/{mod,secret_lifecycle,update,provider_test,service}.rs
    ├── api/{mod,routes}.rs
    └── tests.rs
```

Largest resulting file: `tests.rs` 655; largest source file: `application/update.rs` 269.

### File-placement decision

Per mandate §4 the module root is `backend/src/modules/`. Every *existing*
domain dir (`task/`, `workflow/`, `voice/`, `safety/`, `db/`) still sits at
`backend/src/`. The tree is therefore mixed mid-migration by design — a later
stage must relocate those dirs under `modules/`, or the convention needs
revisiting.

### Files deliberately NOT created

The mandate §9 prescribes `api/dto.rs`, `api/mapping.rs`, `domain/model.rs`,
`infrastructure/config_store.rs`. None was created because there is no existing
content to move into them — mandate §34 forbids speculative abstraction, and §23
forbids splitting logic into meaningless files. Settings persistence remains
`db::settings` plus the `AppConfig` snapshot on `AppServer`. If these files are
wanted, that is new design work, not a move.

### rc.2 secret semantics — preserved

No semantic change. The state machine was moved verbatim into
`application/update.rs`; `application/secret_lifecycle.rs` carries the
KEEP/REPLACE/DELETE helpers. The four pinning tests still pass:

- `empty_key_preserves_secret_across_settings_changes_and_restart`
- `clear_delete_failure_preserves_each_persisted_secret_reference`
- `settings_get_never_returns_secret_value`
- `voice_settings_redact_stt_and_tts_secrets_independently`

---

## 6. Remaining work — R3 to R8

Unchanged from the mandate; recorded here with the concrete entry points found
during R0/R1/R2 so the next session does not have to re-derive them.

### R3 — Task + Workflow HTTP (largest remaining backend stage)

| File | Lines | Notes |
|---|---|---|
| `api/task_world.rs` | 1514 | → `modules/task/{domain,application,api}` |
| `api/workflow_runtime.rs` | 810 | → `modules/workflow/{domain,application,api}` |

- Route registration for both already lives in `app/router.rs` — only the
  handler modules move.
- Do **not** touch `TaskGraph` semantics, `TaskSupervisor`, or workflow
  behaviour. Reuse the existing `TaskSupervisor` / `TaskGraph` / `Execution` /
  `Validation` / `Planner`.
- The security chain `Workflow → SecurityExecutionGateway → Approval →
  Execution → Verification` must survive intact.
- Expect `modules/task/` and `modules/workflow/` to be new, while the existing
  `task/` and `workflow/` dirs stay put — decide the relocation policy first
  (see §5 above), because R3 is where the mixed-tree problem becomes acute.

### R4 — MCP

`mcp.rs` (1384) + `mcp_runtime/` (16 files) → `integrations/mcp/`.
Notably `mcp_transport_config` currently lives in `app/state.rs` and is a
natural fit for `integrations/mcp/` — move it there.
Only stdio exists today; do not add a transport. Keep `McpToolDescriptor`
generic (no GIS types).

### R5 — Resource + Memory Skill

`resource_input.rs` (386) + `resource_input/documents.rs` (226) →
`modules/resource/parsers/`; `memory_skill.rs` (615) + `skill_management.rs`
(220) → `modules/memory_skill/`. Do not name anything
`CapabilityEvolutionEngine` — that is v1.2.

### R6 — Frontend

`features/voice/GlobalVoiceHost.tsx` (895), `pages/SettingsPage.tsx` (726),
`features/task-world/TaskWorldPage.tsx` (368) + `TaskWorldInspector.tsx` (397),
`index.css`. No visual redesign. `npm run build` must pass.

### R7 — Architecture enforcement

`docs/architecture/{module-boundaries,dependency-rules,ownership,current-to-target-map}.md`
plus a lightweight boundary check. Remember `docs/*` is gitignored — use
`git add -f`.

### R8 — Full gate

`cargo fmt --all -- --check`; `cargo check --workspace --locked -j1`;
`cargo test --workspace --all-targets --locked -j1`;
`cargo check -p yi-lian-qian-yan --locked -j1` (confirmed: the Tauri crate **is**
named `yi-lian-qian-yan`); frontend `npm test` + `npm run build`; real-backend E2E.

**`src-tauri` has never been compiled in this worktree.** The workspace gate
will trigger a cold build of the Tauri shell, which is the heaviest target in
the repo and the one the `jobs = 1` policy exists for. Budget for it, and
consider warming it before R8 rather than during it.

---

## 7. Recorded discrepancies with the mandate

| Mandate claim | Reality at base `78f3755` |
|---|---|
| §27 — migration namespaces are Shared `0–999`, v1 `1000–1999`, v2 `2000–2999` | Only `Shared` and `V1TaskWorld` exist in `db/migrations.rs`. There is **no** v2 namespace. Nothing was added or renumbered. |
| §7 — hotspots include `memory_skill.rs`, `resource_input.rs` | Present, but as single files (615 / 386 lines) with `resource_input/documents.rs` alongside — not directories as §16 implies. |
| §8 — `server.rs` < 200–300 lines | Achieved a 13-line facade, which is stricter than the target. |

---

## 8. Compatibility conclusion (R0–R2 only)

| Check | Result |
|---|---|
| Database compatible | **YES** — no schema change, no migration added or renumbered |
| REST compatible | **YES** — all 178 route paths, methods and handlers unchanged; verified by the passing API test targets |
| Frontend route compatible | **YES** — no frontend change in R1/R2 |
| Secret compatible | **YES** — rc.2 KEEP/REPLACE/DELETE semantics verbatim; pinning tests pass |
| Existing user data compatible | **YES** — no persistence change |
| v2 touched | **NO** |

Refactor Gate for R0–R2: **PASS**. The mandate's overall Gate cannot be
evaluated until R8.
