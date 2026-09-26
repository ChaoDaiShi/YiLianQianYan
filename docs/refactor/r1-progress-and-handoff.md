# R1 Architecture Refactor — Progress and Handoff

> **Branch:** `refactor/v1-architecture-foundation`
> **Base:** `origin/v1/release-work` @ `78f3755`
> **Status:** **R0–R8 executed and verified. R6 is partially complete by
> decision: `index.css` is deferred on evidence and `GlobalVoiceHost`'s
> orchestrator is deliberately unsplit. Both are argued in the final report.**
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
| `958ffd6` | R5 | `docs(refactor): record R5 progress` |
| `0456c03` | R6a | `refactor(frontend): split task inspector into sections` |
| `c7cdde6` | R6b | `refactor(frontend): extract task world page behaviour into hooks` |
| `4ca7954` | R6c | `refactor(frontend): move canvas under canvas/ and extract TaskNode` |
| `85f25c2` | R7 | `chore(architecture): enforce module boundaries` |
| `736ea3e` | R7 | `docs(refactor): record R6 partial and R7 progress` |
| `41fb035` | R7 | `chore(architecture): enforce module boundaries` (baseline corrections) |
| `019c0e0` | R8 | `docs(refactor): add R1 final architecture report` |
| `82118ab` | R6d | `refactor(frontend): split settings page into sections` |
| `d94221e` | R6e | `refactor(frontend): extract voice model, routing and context from the host` |
| `7123cba` | R6f | `refactor(frontend): split index.css into cascade-ordered style files` |
| `f2044b1` | R6g | `refactor(voice): extract the barge-in flow into a testable module` |
| `e38b696` | R6h | `refactor(voice): extract the final-transcript/continuation turn flow` |
| `cff1c38` | R9a | `refactor(voice): relocate the voice module under modules/` |
| `ab61d80` | R9b | `refactor(capability): relocate the capability registry under modules/` |
| `2b3cf59` | R9c | `refactor(llm): relocate the LLM client under integrations` |
| `3378a5d` | R9d | `refactor(secret): relocate secret management under integrations` |

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
| **Commit limit** | 39.24 GB (RAM + a 24 GB page file), of which **36 GB is usually already committed** by ~490 resident processes. The root `Cargo.toml`'s `codegen-units = 1` then makes the lib-test binary one giant LLVM unit that cannot be allocated. If `cargo test` dies with `rustc-LLVM ERROR: out of memory`, see §10a — the fix is `CARGO_PROFILE_TEST_CODEGEN_UNITS=4`, not closing apps. |
| **Cost** | ~60 min cold, ~3 min incremental (plus ~1.5 min to relink the lib-test unit). Budget accordingly; keep dev-stage verification focused. |

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

## 9. R6 — what was done

Of the six hotspots named in mandate §38: four are done
(`TaskWorldInspector`, `TaskWorldPage`, `SettingsPage`, `index.css`), and two
are partial by decision (`TaskWorldCanvas`, `GlobalVoiceHost`).

**R6a (`0456c03`) — `TaskWorldInspector.tsx` (397) → `inspector/`.**
A shell (`TaskInspector.tsx`) plus one file per block: Basic, Executor,
Acceptance, State, Execution, Resource, Artifact, Dependency, Version. The
shell owns the shared semantic draft and the single form submit; no section
reaches for the API. Two documented deviations: `AcceptanceSection` is separate
because the executor fields sit *between* the description and the criteria in
the existing form, and merging would have reordered them; `StateSection` is
separate because the markup already carries two distinct headings.

**R6b (`c7cdde6`) — `TaskWorldPage.tsx` (368) → `hooks/`.**
`useTaskGraph`, `useCanvasView`, `useTaskEvents`, `useTaskReview`,
`useTaskCommands`. The page is now 203 lines of composition. Two faithfulness
notes worth keeping: focus repair is delegated as `onNodesLoaded(nodeIds)` and
run inline so it still happens before paint (a `useEffect` would flash the
previous selection), and that callback is memoised with `[]` deps so
`refreshDetail` keeps a stable identity — an inline arrow re-triggers the
reload effect into a fetch loop. Effect order and dependency arrays are
transcribed as written.

**R6c (`4ca7954`) — `TaskWorldCanvas.tsx` (198) → `canvas/`, partial.**
`canvas/TaskNode.tsx` takes the React Flow node renderer and its `statusLabel`
map — a real component boundary. The remaining 112-line surface stays intact:
at that size it is one coherent React Flow integration, and carving it into
`useCanvasProjectionSync` / `useCanvasViewport` / `useCanvasPersistence` would
be slicing by line count (§23) with no render test to catch a regression.

**R6d (`82118ab`) — `SettingsPage.tsx` (726) → 364 lines.**
Content moved to `features/settings/{model,sections}/`: `config.ts` (defaults,
section list, appearance modes), `mapping.ts` (the redacted comparison
projection and secret-source labels), `types.ts`, and nine section components.
The page keeps the config state, the save/clear flows that carry the rc.2
secret semantics, and the `?section=` deep link. It also **stays at
`pages/SettingsPage.tsx`**: §20 puts it under `features/settings/pages/`, but
route pages live in `pages/` throughout this repo, and moving one would both
create a second convention and churn the route table plus three `?raw` contract
tests for no behavioural gain.

**R6e (`d94221e`) — `GlobalVoiceHost.tsx` (895) → 703 lines, partial.**
The exported types, ~170 lines of pure projections and the route policy moved
to `features/voice/{model,context}/`, with `./GlobalVoiceHost` re-exporting
everything so all four importers compile untouched. The 640-line orchestrator
was **not** split into §21's hooks: it carries ~16 epoch/identity `useRef`
guards shared *across* the state machines those hooks would separate, a
barge-in path that re-checks the epoch after every `await`, and echo
suppression keyed to the context epoch. That is a concurrency redesign, not a
file move, and `GlobalVoiceHost.test.tsx` renders to static markup so it cannot
catch a mistake there. It remains on the over-600 baseline rather than being
quietly dropped from it. **This was resolved by R6g/R6h below**, which extract
the concurrency-critical flows into pure modules with an interaction test each —
the exact test the R6e decision said did not exist.

**Contract tests migrated, not weakened.** These are source-text (`?raw`)
assertions, so a split invalidates their import paths. `taskCorePaths.test.tsx`
now renders `inspector/TaskInspector`; `taskWorldCanvas.test.ts` reads the page
shell plus all five hooks and the inspector shell plus all sections;
`systemSettingsContract.test.ts` reads the settings page plus `model/` and all
nine section files. Every string each test pinned — including the negative
assertions — is still *required to exist on the same surface*.

**R6f (`7123cba`) — `index.css` (3389) → a 28-line manifest.**
The rules moved to ten files under `src/styles/`, split **by cascade layer**
rather than by feature: a feature grouping is not order-preserving for this
file, because the rules are interleaved by feature across layers. Splitting by
layer keeps every block contiguous and every `@layer` block inside the
Tailwind-processed file. Proved, not asserted: the built stylesheet is
byte-identical, md5 `5889431e` before and after.

Two attempts failed first, both caught by that md5 guard and neither by review:
placing `@import` before `@tailwind` (PostCSS's usual requirement) moved the
unlayered rules ahead of the hoisted output; then extracting by assumed line
ranges folded three *unlayered* `@media`/`@keyframes` regions into `@layer`
files and displaced 45 rules. Fixed by enumerating top-level blocks by brace
depth — which found five unlayered regions rather than the two I had assumed.

Two consequences worth carrying forward: `index.css` now holds no rules, so it
cannot sprawl again; and CSS is **not** covered by the automated boundary check
(it reads `.ts`/`.tsx` only), so the md5 diff is the only guard for a future
style change.

**R6g (`f2044b1`) + R6h (`e38b696`) — `GlobalVoiceHost`'s orchestrator concurrency, extracted.**

The R6e decision held that the 640-line orchestrator could not be split without
a concurrency redesign *and* an interaction test that static-markup rendering
cannot provide. Both now exist, so the decomposition the handoff §11 item #1
called for is done — by extraction, not by moving the component.

The two flows that carry the cross-`await` epoch/identity guards were pulled out
as pure functions of an injected runtime (`(result, runtime) => …`):

- **`bargeIn.ts`** — `runVoiceBargeIn`, the bump-epoch → abort → interrupt →
  commit sequence with its stale-response and sync-anchor fail-closed checks.
- **`turnFlow.ts`** — `runFinalTranscriptFlow`, the validate → echo → dispatch →
  re-check-epoch → continuation (abortable) → re-check-epoch → speak sequence.

Each runtime interface is a port: the host injects its `useRef` guards and
setters; the flow itself has no React and no globals, so the concurrency is
testable with controllable promises. Eighteen interaction tests drive the flows
across `await` boundaries: a stale generation or changed conversation is
dropped, echo suppresses the assistant's own playback, a dispatch that resolves
after a newer epoch is discarded, a continuation aborted by a newer barge-in is
silently dropped while a non-abort failure surfaces a notice, and `silent`
attention mode never speaks.

`GlobalVoiceHost.tsx` is now **616 lines** — 703 before, and the remaining body
is React glue (refs, effects, render). It is *still* over 600, so it **stays on
the boundary baseline** rather than being quietly dropped from it; the
concurrency logic, however, is out and covered. The host contract test that
pinned source strings now reads `bargeIn.ts` / `turnFlow.ts` where the strings
moved, without weakening any assertion.

### R6 — what was NOT done

One hotspot remains partial by decision: `TaskWorldCanvas`'s 112-line surface
(argued above). `GlobalVoiceHost`'s orchestrator is now decomposed (R6g/R6h).

## 10. R7 — what was done

Four documents under `docs/architecture/` (`docs/*` is gitignored — commit with
`git add -f`):

- `module-boundaries.md` — module shape, what each module owns, public
  entrypoints, and the list of compatibility facades still to drain.
- `dependency-rules.md` — the allowed direction on both sides, the numbered
  rules, which are machine-checked and which are review obligations.
- `ownership.md` — per module: purpose, owns, may depend on, must not depend
  on, entrypoints, persistence, invariants. Plus the Protected Kernel list.
- `current-to-target-map.md` — mandate §4/§17 target vs. reality, with the
  `index.css` analysis.

Two lightweight checks, no new dependency:

| Check | Runs under | Covers |
|---|---|---|
| `frontend/src/architecture/boundaries.test.ts` | `npm test` | primitives stay dumb; features stay independent; Tauri reached only dynamically outside `surfaces/desktop`; no new file >600 lines |
| `backend/tests/architecture_boundaries.rs` | `cargo test` | no transport in `modules/*/domain`; no v2 namespace in v1; no new file >600 lines |

All four frontend rules hold in the codebase today, so they are errors, not
warnings. The 600-line baselines (39 backend paths, 6 frontend paths) are
recorded debt: a file may leave the list by being split; nothing may join it.

## 10a. R9 — post-gate backend relocation (not a mandate stage)

R9 is the continuation of the remaining-work list in §11, not a stage the
mandate defines. It is labelled R9 so it cannot be confused with a mandate
stage; the mandate's own stage set ends at R8.

Four relocations, one commit each, all `git mv` + facade:

| Stage | Commit | Move | Files | Baseline |
|---|---|---|---|---|
| R9a | `cff1c38` | `voice/` → `modules/voice/` | 4 | `voice/provider.rs`, `voice/runtime.rs` re-pathed |
| R9b | `ab61d80` | `capability/` → `modules/capability/` | 12 | none over 600 |
| R9c | `2b3cf59` | `llm/` → `integrations/llm/` | 5 | `llm/client.rs` re-pathed |
| R9d | `3378a5d` | `secret/` → `integrations/secret/` | 8 | none over 600 |

Every one of the 29 moved files is recorded by git as a **100%-similarity
rename** — `0 insertions, 0 deletions` — so every moved region is byte-identical
and `git log -S` / `git blame` still attribute the original authorship. Each
destination module declares the new path in `modules/mod.rs` /
`integrations/mod.rs`, and each old path becomes a facade holding one glob
re-export, so **no call site changed**: all `crate::voice::*`,
`crate::capability::*`, `crate::llm::*` and `crate::secret::*` references compile
untouched, including submodule paths such as `crate::voice::provider::MAX_AUDIO_BYTES`
and the `pub fn record_secret_event` that lives in the secret module root.

### Why relocation was chosen over draining the facades

The remaining-work list had two backend items: relocate the domains (#4) and
drain the facades (#5). Draining was attempted first on paper and rejected on
evidence:

- The facades are also the crate's **public API**. `backend/tests/*.rs` import
  `yilian_backend::task::{...}` and `yilian_backend::server::AppServer`, so
  draining is not a `backend/src` refactor — it reaches the integration tests
  too.
- The cost is **243 references across 97 files** (`crate::task` 115 sites,
  `crate::server` 51, `crate::workflow` 33, `crate::mcp_runtime` 19, the rest
  smaller), all for zero behavioural or boundary change: a `pub use` facade is
  free at runtime and is not a second implementation.
- Relocation, by contrast, moves 29 files byte-identically for ~10 added lines.

So the facades are left standing, and R9 **adds four more** — the count goes
8 → 12. That is the honest trade: the strangler scaffolding is not free, and
draining it is a deliberately separate piece of work rather than a rounding
error on a relocation.

### R9 verification, and its one gap

| Check | Result |
|---|---|
| `cargo fmt --all -- --check` | clean after each of the four stages |
| `cargo check -p yilian-backend --all-targets --locked` | **exit 0** after each of the four stages; warnings unchanged at the same 3 pre-existing ones |
| `cargo test --lib` (the unit suite) | **892 passed / 0 failed** — after clearing the OOM below |
| `cargo test -p yilian-backend --locked` | **21 binaries, 983 passed, 0 failed, 0 ignored** |
| `cargo test -p yi-lian-qian-yan --locked` | **5 passed / 0 failed** |

### The OOM, and how it was worked around

`cargo test --lib` first failed reproducibly in the codegen/link step with
`rustc-LLVM ERROR: out of memory / Allocation failed` (exit `0xc0000409`).
Diagnosis, measured rather than guessed:

```
CommitLimit : 39.24 GB   (15.24 GB RAM + 24 GB page file)
CommitUsed  : 36.04 GB   → only ~3.2 GB of commit headroom
              spread over 490 processes + ~8.0 GB kernel commit
FreePhysical: 1.7–3.7 GB across five attempts
```

There is no single hog to close — the largest process commits 1.2 GB — so
freeing "a couple of apps" cannot recover the ~5 GB the unit needs.

The cause is the root `Cargo.toml`, which sets `codegen-units = 1` for
`[profile.dev]` **and** `[profile.dev.package."*"]`: the lib-test binary is one
giant LLVM codegen unit and cannot be allocated under that ceiling. The fix is a
per-invocation profile override that touches no repo file:

```bash
CARGO_PROFILE_TEST_CODEGEN_UNITS=4 \
CARGO_TARGET_DIR="E:/cargo-target/yilian-arch" \
cargo test -p yilian-backend --locked
```

`[profile.test]` applies only to the local crate's own test targets, not to
dependencies (which build under `dev`), so this recompiles just the lib-test
unit — 1m28s — instead of triggering a full dependency rebuild. No repo file
changed, no test behaviour changed, and the override is not needed on an
unloaded machine.

**The counts reconcile exactly.** Backend 983 + `src-tauri` 5 = **988**, which
is the number the R8 gate recorded for `--workspace --all-targets`. So R9 is
verified to the same depth the gate used, and the rc.2 secret pinning tests did
run:

- `modules::settings::tests::empty_key_preserves_secret_across_settings_changes_and_restart`
- `modules::settings::tests::clear_delete_failure_preserves_each_persisted_secret_reference`
- `modules::settings::tests::settings_get_never_returns_secret_value`
- `modules::settings::tests::voice_settings_redact_stt_and_tts_secrets_independently`

Their **module paths in the test output** (`integrations::secret::tests::*`,
`integrations::llm::client::tests::*`) are themselves evidence that the moved
tests relocated with their modules and still resolve.

## 11. Remaining work

Ordered by how much is already understood, not by size.

1. **`GlobalVoiceHost`'s orchestrator — done (R6g/R6h).** The barge-in and
   final-transcript/continuation flows are extracted into `bargeIn.ts` and
   `turnFlow.ts` as pure runtime-injected functions, with 18 interaction tests.
   The host itself is 616 lines of React glue and stays on the over-600
   baseline; the concurrency logic is out and covered.
2. **`TaskWorldCanvas`'s surface (112 lines).** Only worth splitting if the
   hooks fall out along real seams; at present they do not.
3. **R3/R4 gaps, inherited.** `modules/task/application` and
   `modules/workflow/application` are unextracted; MCP is relocated but not
   regrouped, and `mcp_transport_config` still lives in `app/state.rs`.
4. **Backend relocation — done for the product domains (R9a–R9d).** `voice`,
   `capability`, `llm` and `secret` now live under `modules/` and
   `integrations/`. `agent` and `tools` remain at the root: the target table
   assigns them no module home, so moving them would be a naming decision
   rather than a relocation.
5. **The compatibility facades — now twelve.** Four were added by R9
   (`capability.rs`, `llm.rs`, `secret.rs`, `voice.rs`). Each needs its call
   sites updated and then deleted. See §10a for why draining was not done
   first.
6. **`styles/workspace.css` (1536 lines).** One contiguous `@layer components`
   block for the workspace shell and its feature pages. Splitting it further is
   cascade-safe but would produce arbitrarily-named files, because its rules are
   interleaved by feature — the same reason `index.css` could not be split by
   feature. Leave it unless a real seam appears.

### If you touch the stylesheet

Read the header of `frontend/src/index.css` first. The import order there is
load-bearing, and the only way to check a change is to build and compare
`dist/assets/index-*.css` against md5 `5889431edc65281a20589a3953c724e2`. Also
add any new style file to `frontend/src/styles/stylesheet.ts`, or the CSS-facing
tests will silently stop reading it.

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

## 12. Recorded discrepancies with the mandate

| Mandate claim | Reality at base `78f3755` |
|---|---|
| §27 — migration namespaces are Shared `0–999`, v1 `1000–1999`, v2 `2000–2999` | Only `Shared` and `V1TaskWorld` exist in `db/migrations.rs`. There is **no** v2 namespace. Nothing was added or renumbered. |
| §7 — hotspots include `memory_skill.rs`, `resource_input.rs` | Present, but as single files (615 / 386 lines) with `resource_input/documents.rs` alongside — not directories as §16 implies. **Resolved by R5.** |
| §8 — `server.rs` < 200–300 lines | Achieved a 13-line facade, which is stricter than the target. |
| §16 — `parsers/{text,markdown,csv,image,pdf,docx,xlsx}.rs` | `archive.rs` added (shared OOXML container reader); `markdown.rs` and `csv.rs` deliberately omitted — see §8. |

---

## 13. Compatibility conclusion (R0–R8)

| Check | Result |
|---|---|
| Database compatible | **YES** — no schema change, no migration added or renumbered |
| REST compatible | **YES** — all 178 route paths, methods and handlers unchanged; verified by the passing API test targets |
| Frontend route compatible | **YES** — no route path or page contract changed in R1–R7 |
| Secret compatible | **YES** — rc.2 KEEP/REPLACE/DELETE semantics verbatim; pinning tests pass |
| Existing user data compatible | **YES** — no persistence change |
| v2 touched | **NO** |

Refactor Gate for R0–R8: **PASS**. R8 ran the full gate — fmt, check
`--workspace`, test `--workspace --all-targets` (21 binaries, 988 passed), check
`-p yi-lian-qian-yan`, `npm test` (91 files / 426 passed), `npm run build`, and
the real-backend E2E — all green. After R6d/R6e changed frontend files, the
frontend gates and the E2E were re-run against the final commit rather than
assumed; no Rust source changed in between. Full detail in the final report.

R6d–R6h are the frontend stages that moved files. None changed a route
path or a rendered contract: `npm test` stayed green across all of them (and
grew to **93 files / 444 tests** with R6g/R6h's 18 interaction tests), the E2E
passed against the rewritten Settings page at all three viewports, and the
built stylesheet md5 is still `5889431e` — which for R6f is not merely
consistent but is the proof that the split moved nothing. R6g/R6h changed no
route and no rendered markup either: they extract the voice host's async
concurrency into pure modules, and the host's static-markup contract test still
passes against the same surface.

R3, R4 and R5 changed no route path, no response shape, no schema and no
frontend file, so the R2 conclusions above still hold — they are
relocation-and-facade stages.

R6a/R6b are the first stages to move frontend files. They changed no route path
and no rendered contract: the inspector and page tests still pass, including
`taskCorePaths.test.tsx`, which *renders* the inspector rather than only
reading it. The frontend suite went 90 files / 422 tests → 91 / 426, the
increase being R7's four boundary tests; no test was deleted or skipped. The
built stylesheet md5 is unchanged at `5889431e` across all of R6, which is a
direct check that no styling moved.

R7 added documentation and two test files. It changed no production code on
either side.
