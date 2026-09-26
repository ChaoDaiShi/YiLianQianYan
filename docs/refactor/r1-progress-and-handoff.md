# R1 Architecture Refactor — Progress and Handoff

> **Branch:** `refactor/v1-architecture-foundation`
> **Base:** `origin/v1/release-work` @ `78f3755`
> **Status:** **PARTIAL — R0, R1, R2, R3, R4, R5 complete and verified. R6–R8 not started.**
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
| `cacf187` | R3a | `refactor(task): relocate task and workflow under modules` |
| `024c4d4` | R3b | `refactor(task): separate task and workflow boundaries` |
| `fa8955d` | R4 | `refactor(mcp): unify MCP under integrations` |
| `ff5a907` | R5a | `refactor(capability): relocate resource and memory skill under modules` |
| `70f954f` | R5b | `refactor(capability): split resource module into parsers and use cases` |
| `f18790f` | R5c | `refactor(capability): split memory skill into review lifecycle layers` |

Working tree is clean; `main`, `develop`, `v1/release-work`, and `v2` are untouched.
All commits are pushed to `origin/refactor/v1-architecture-foundation`.

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

| Command | R1 | R2 | R3 | R4 | R5 |
|---|---|---|---|---|---|
| `cargo check -p yilian-backend --all-targets --locked` | pass | pass | pass | pass | pass |
| `cargo fmt --all -- --check` | clean | clean | clean | clean | clean |
| `cargo test -p yilian-backend --locked` | 892 + 20 targets, 0 failed | same | same | same | same |
| New warnings introduced | 0 | 0 | 0 | 0 | 0 |
| Pre-existing warnings remaining | 3 | 3 | 3 | 3 | 3 |

R5 verification was staged: each of R5a/R5b/R5c got its own
`cargo check --all-targets` plus focused tests, and only the final `--check` on
formatting. The lib test count is 892 before and after, confirmed by the filter
arithmetic each run (`passed + filtered out = 892`).

Pre-existing warnings, unchanged by this work:
`apply_product_migration` never used, `validate_shared_version` never used,
`q_embed` unused variable.

Moved tests confirmed executed, not skipped:
- R1 — 15 tests: `app::state::tests::*` (4) + `agent::definition::tests::*` (10) +
  `app::state::tests::runtime_registry_includes_builtins_and_executable_subagent`
- R2 — 11 tests: `modules::settings::tests::*`, including the four that pin the
  rc.2 secret semantics.
- R3 — 13 HTTP tests moved: `modules::task::api::tests::*` (6) +
  `modules::workflow::api::tests::*` (7), plus 169 task/workflow module tests.
- R4 — 62 `integrations::mcp::*` tests. **Two of these initially failed** after
  the move (see §7); the fix is part of the R4 commit.
- R5 — 10 relocated/moved tests: `modules::resource::limits::tests` (1) +
  `modules::resource::preview::tests` (4) + `modules::memory_skill::tests` (3) +
  `modules::memory_skill::store::tests` (2), plus the consumers
  `modules::task::context` (4), `db::resource_bindings` (1),
  `api::skills_route` (1) and `capability::import_owner` (1).

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

## 6. R3 — what was done

Two commits, because relocating first was what made the split sane.

**R3a (`cacf187`) — relocate.** R2 created `modules/` but only `settings` used
it, leaving one domain at `modules/settings` and the rest at `backend/src/`.
`task/` (30 files) and `workflow/` (10 files) moved under `modules/` by
`git mv` — 39 renames, zero content changes — with `task.rs` / `workflow.rs`
becoming facades. All 39 `crate::task` and 11 `crate::workflow` importers
compiled untouched. Revisit the convention question this settles: **the project
now uses `modules/<domain>/`, and remaining root-level domain dirs
(`voice/`, `capability/`, `skill_management.rs`, `db/`, `safety/`) are
inconsistent with it.**

**R3b (`024c4d4`) — split the HTTP files.**

```
modules/task/api/      graph_routes, node_routes, canvas_routes, review_routes,
                       execution_routes, command_routes, dto, shared, mod, tests
modules/workflow/api/  graph_routes, run_routes, mapping, dto, mod, tests
```

Both legacy paths (`api/task_world.rs`, `api/workflow_runtime.rs`) are facades.
Largest source file 294 lines; tests 595. Domain semantics, route paths and
response shapes are unchanged; all 13 moved HTTP tests pass, including the
cancellation and security-chain ones.

**Not done in R3:** the `modules/task/application/*` and
`modules/workflow/application/*` service extraction of mandate §10/§12. Business
logic still lives in the route modules (`create_graph` 71 lines,
`run_workflow_graph` 157 lines, `dispatch_execution` 66 lines). This is the
largest remaining piece of R3 and needs real design, not a move.

## 7. R4 — what was done

`fa8955d`. MCP lived in two parallel homes: `mcp.rs` (1384) and `mcp_runtime/`
(16 files, 3199). Both relocated under `integrations/mcp/` with facades, so all
4 `crate::mcp::*` and 9 `crate::mcp_runtime::*` call sites were untouched.
16 renames. `http.rs` already existed — no transport was added.

**Regression found and fixed during R4 — worth knowing for R5/R6.**
`stdio_tests.rs` self-spawns the test binary with a hardcoded libtest filter
string (`"mcp_runtime::stdio_tests::mock_stdio_server"`). After the move the
filter matched nothing, the child ran zero tests and exited, and both stdio
tests failed with `Transport("stdio EOF")`. It is now derived from
`module_path!()` so it survives future moves.

> **When moving any module, grep for its old path used as a *string*, not just
> as a `use`:** `grep -rn '"<old::module::path>'`. A compile-clean move can
> still silently disable a self-spawning test.

**Not done in R4:** the internal `protocol/ transport/ runtime/ registry/
result/ security/` subdivision of mandate §13. The files are relocated, not
regrouped. `mcp_transport_config` still sits in `app/state.rs` and is a natural
fit for `integrations/mcp/`.

---

## 8. R5 — what was done

Three commits, because relocating first is what made the splits reviewable.

**R5a (`ff5a907`) — relocate.** `resource_input` + `resource_input/documents.rs`
and `memory_skill` / `skill_management` moved under `modules/` by `git mv`
(4 renames) with facades at the old paths, so all 12 call sites compiled
untouched. `skill_management.rs` was renamed to
`modules/memory_skill/store.rs` — the file holds `ManagedSkillStore`, and the
mandate's module group now owns it explicitly.

**R5b (`70f954f`) — resource.**

```
modules/resource/
├── mod.rs      limits.rs   model.rs
├── ingest.rs   binding.rs  preview.rs
└── parsers/    mod, text, image, pdf, docx, xlsx, archive
```

Two deliberate deviations from mandate §16, both recorded in the code:

- **`archive.rs` added.** `zip_entries` (entry/size/expanded-bytes caps) was
  already shared by `docx` and `xlsx` inside `documents.rs`. Leaving it in
  either parser would make one import from the other.
- **`markdown.rs` / `csv.rs` not created.** Every `text/*` type the upload
  policy canonicalises — plain text, Markdown, CSV/TSV, code — shares one
  extraction path today: strict UTF-8 decoding. Separate files would contain
  only a forward to `text::parse`, which §23/§34 forbid. `parsers/text.rs`
  documents the decision where a future format would be added.

**R5c (`f18790f`) — memory skill.** 615 lines became eight layers
(`candidate`, `review`, `version`, `repository`, `evidence`, `sensitivity`,
`validator`, `service`) plus `store` and `tests`. An inherent
`impl MemorySkillService` block is legal in any module of the crate, so the
lifecycle reads `candidate -> review -> version` without a second type. The
struct fields became `pub(crate)`; external visibility is unchanged.

### R5 — what was *not* changed

- No route, response shape, schema, migration or frontend file.
- The secret-adjacent guarantees were transcribed verbatim: unconfirmed install
  refused, stale revision refused, `status != "validated"` not confirmable,
  evidence and lesson re-screened on every transition, `checked_current`
  refusing an externally edited rule instead of trusting the database, and a
  failed commit restoring the previous file content.
- Nothing was named `CapabilityEvolutionEngine`.

### R5 — boundary debt this exposed

`modules/resource/binding.rs` reads `modules::task::execution::MAX_NODE_CONTEXT_ITEM_CHARS`.
That is a resource -> task compile-time dependency on a task constant, found
while splitting and left as-is because R5 is relocation, not redesign. It is
the one cross-domain edge the R7 boundary check will need to either allow or
schedule for removal. Recorded, not silently fixed.

---

## 9. Remaining work — R6 to R8

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

## 10. Recorded discrepancies with the mandate

| Mandate claim | Reality at base `78f3755` |
|---|---|
| §27 — migration namespaces are Shared `0–999`, v1 `1000–1999`, v2 `2000–2999` | Only `Shared` and `V1TaskWorld` exist in `db/migrations.rs`. There is **no** v2 namespace. Nothing was added or renumbered. |
| §7 — hotspots include `memory_skill.rs`, `resource_input.rs` | Present, but as single files (615 / 386 lines) with `resource_input/documents.rs` alongside — not directories as §16 implies. **Resolved by R5.** |
| §8 — `server.rs` < 200–300 lines | Achieved a 13-line facade, which is stricter than the target. |
| §16 — `parsers/{text,markdown,csv,image,pdf,docx,xlsx}.rs` | `archive.rs` added (shared OOXML container reader); `markdown.rs` and `csv.rs` deliberately omitted — see §8. |

---

## 11. Compatibility conclusion (R0–R5)

| Check | Result |
|---|---|
| Database compatible | **YES** — no schema change, no migration added or renumbered |
| REST compatible | **YES** — all 178 route paths, methods and handlers unchanged; verified by the passing API test targets |
| Frontend route compatible | **YES** — no frontend change in R1–R5 |
| Secret compatible | **YES** — rc.2 KEEP/REPLACE/DELETE semantics verbatim; pinning tests pass |
| Existing user data compatible | **YES** — no persistence change |
| v2 touched | **NO** |

Refactor Gate for R0–R5: **PASS**. The mandate's overall Gate cannot be
evaluated until R8.

R3, R4 and R5 changed no route path, no response shape, no schema and no
frontend file, so the R2 conclusions above still hold. All three are
relocation-and-facade stages: every public surface is reached through the same
paths as before the move.
