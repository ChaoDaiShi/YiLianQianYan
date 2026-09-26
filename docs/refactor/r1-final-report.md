# R1 Architecture Refactor — Final Report

> **Branch:** `refactor/v1-architecture-foundation`
> **Base:** `origin/v1/release-work` @ `78f3755` (v1.0.0-rc.2, frozen)
> **Status:** R0–R8 executed. **Full Gate PASS.** R6 is partially complete and
> is reported as such below — this is not a claim of total completion.
>
> Companion documents: `r1-inventory.md` (R0), `r1-progress-and-handoff.md`
> (per-stage detail), `../architecture/*.md` (the standing rules).

---

## 1. Compatibility conclusion (mandate §46)

| Check | Result | Evidence |
|---|---|---|
| Database compatible | **YES** | `backend/src/db/migrations.rs` is byte-identical to the base. No migration added, renumbered, edited or re-owned. No file under `backend/src/db/` changed at all. |
| REST compatible | **YES** | Route-path set extracted from both trees: **152 paths in the base, 152 at HEAD, zero differences**. No method, path or handler signature changed. |
| Frontend route compatible | **YES** | `surfaces/workspace/WorkspaceSurface.tsx` (the route table) is byte-identical to the base. |
| Secret compatible | **YES** | rc.2 KEEP / REPLACE / DELETE semantics moved verbatim; the four pinning tests (`empty_key_preserves_secret_across_settings_changes_and_restart`, `clear_delete_failure_preserves_each_persisted_secret_reference`, `settings_get_never_returns_secret_value`, `voice_settings_redact_stt_and_tts_secrets_independently`) pass in the R8 gate. |
| Existing user data compatible | **YES** | No persistence change: no schema, no migration, no storage-root or file-format change. |
| v2 touched | **NO** | No v2 namespace appears in v1 source, and a boundary check now enforces that (`v1_does_not_reach_into_v2`). |

**Refactor Gate: PASS.**

One more compatibility fact worth recording, because it is stronger than any
test: the **built stylesheet is byte-identical** across the whole refactor.
`dist/assets/index-*.css` has md5 `5889431edc65281a20589a3953c724e2` before and
after every stage. No CSS rule moved, reordered or changed.

---

## 2. Before → after architecture

### Backend, before

`server.rs` was 1160 lines of state, assembly, routing and helpers at once.
`api/` held 178 routes plus the settings state machine (1365 lines in one
file), the task-world and workflow HTTP surfaces, and MCP lived in two parallel
homes (`mcp.rs` 1384 + `mcp_runtime/` 3199). Domain directories sat at
`backend/src/` with no single convention.

### Backend, after

```
backend/src/
├── app/            state, bootstrap, router, lifecycle        (R1)
├── modules/        domain modules — the destination for product logic
│   ├── settings/   domain / application / api / tests         (R2)
│   ├── task/       model, runtime, planner, supervisor, api/  (R3)
│   ├── workflow/   graph, run, state machine, api/            (R3)
│   ├── resource/   limits, parsers, preview, ingest, binding  (R5)
│   └── memory_skill/ candidate, review, version, evidence, store (R5)
├── integrations/mcp/                                          (R4)
├── db/             schema, migrations                         (untouched)
├── safety/         SecurityExecutionGateway — Protected Kernel (untouched)
└── (voice, capability, llm, secret, agent, tools still at src/)   ← remaining
```

`server.rs` is a 13-line facade. `lib.rs` went 145 → 31 lines.

### Frontend, after

```
frontend/src/features/task-world/
├── TaskWorldPage.tsx            203 lines — composition only        (R6b)
├── canvas/{TaskWorldCanvas,TaskNode}.tsx                            (R6c)
├── inspector/                   shell + 9 section files             (R6a)
├── hooks/                       useTaskGraph, useCanvasView,
│                                useTaskEvents, useTaskReview,
│                                useTaskCommands                     (R6b)
└── (taskGraphProjection, canvasViewWriter, TaskExecutionTrail at root)
```

Frontend churn stayed contained: **41 files changed in total** across the
entire refactor — 33 added, 1 deleted, 6 modified, 1 renamed. No route path,
no page contract and no stylesheet rule changed.

---

## 3. Dependency graph (mandate §45)

Backend:

```
Transport / UI            api/, app/router.rs, Tauri command layer
      ↓
Application               modules/*/application/, app/*
      ↓
Domain                    modules/*/domain/
      ↓
Ports                     traits the domain/application declare
      ↑
Infrastructure            db/, integrations/, llm/, secret/
```

Frontend:

```
App
 ↓
Feature Pages
 ↓
Feature Hooks / Model
 ↓
Feature API
 ↓
Core HTTP / Event Client   (api/client.ts, api/events.ts)
```

Cross-domain interaction — `Conversation`, `Task`, `Workflow`, `Capability`,
`MemorySkill`, `Voice`, `Resource`, `Artifact` — goes through a command, an
event, a projection or an application service. Not through a sibling's
internals.

---

## 4. Module ownership

The authoritative per-module table is `docs/architecture/ownership.md`
(purpose, owns, may depend on, must not depend on, public entrypoints,
persistence ownership, events emitted, invariants). Summary of what moved:
settings, task, workflow, resource and memory_skill are now owned modules with
declared boundaries; `db/` and `safety/` are deliberately unmoved because they
own schema and the Protected Kernel respectively.

---

## 5. Current public facades

Every facade holds no implementation. Each exists so call sites could keep
compiling while the implementation moved.

| Facade | Re-exports | Stage |
|---|---|---|
| `crate::server` | `app::state` | R1 |
| `crate::task` | `modules::task` | R3a |
| `crate::workflow` | `modules::workflow` | R3a |
| `crate::mcp` | `integrations::mcp::legacy_stdio` | R4 |
| `crate::mcp_runtime` | `integrations::mcp::*` | R4 |
| `crate::resource_input` | `modules::resource` | R5a |
| `crate::memory_skill` | `modules::memory_skill` | R5a |
| `crate::skill_management` | `modules::memory_skill::store` | R5a |
| `crate::voice` | `modules::voice` | R9a |
| `crate::capability` | `modules::capability` | R9b |
| `crate::llm` | `integrations::llm` | R9c |
| `crate::secret` | `integrations::secret` | R9d |

Remaining work: update the call sites, then delete the facade. This was the
strangler strategy mandate §25 prescribes, and it is why no stage ever
produced hundreds of broken imports.

Draining is deferred, not overlooked — the cost was measured at **243
references across 97 files**, and the facades are also the crate's public API
(`backend/tests/*.rs` imports `yilian_backend::task` and
`yilian_backend::server`). R9 added four more facades rather than draining the
existing eight; the reasoning is in `r1-progress-and-handoff.md` §10a.

---

## 6. Removed legacy

- `server.rs` implementation (1160 → 13 lines) — moved to `app/`.
- `api/settings.rs` implementation (1365 lines) — moved to `modules/settings/`.
- `TaskWorldInspector.tsx` (397 lines) — deleted, replaced by `inspector/`.
- `resource_input/documents.rs` — deleted, replaced by `parsers/`.
- The 615-line `memory_skill.rs` and 220-line `skill_management.rs` — replaced
  by eleven focused files.

---

## 7. Files still over 600 lines

**Debt, recorded and now enforced.** Both boundary checks carry the list; a
file may leave it by being split, nothing may join it.

**Backend — 38 files.** The largest:

| File | Lines |
|---|---|
| `safety/execution_gateway.rs` | 3049 |
| `modules/task/runtime.rs` | 2910 |
| `api/approvals.rs` | 2093 |
| `modules/workflow/tests.rs` | 1737 |
| `api/memories.rs` | 1611 |
| `integrations/mcp/legacy_stdio.rs` | 1384 |
| `modules/task/orchestrator.rs` | 1216 |
| `modules/voice/runtime.rs` | 1195 |
| `db/task.rs` | 1107 |
| `modules/task/task_supervisor.rs` | 1087 |

**Frontend — 5 files:** `api/legacy.ts` (1335), `pages/PluginsPage.tsx` (973),
`features/voice/GlobalVoiceHost.tsx` (616), `components/chat/ChatView.tsx`
(623), `api/taskWorld.ts` (601).

`SettingsPage.tsx` left this list during R6 (726 → 364). `GlobalVoiceHost.tsx`
did not, and is still on it deliberately — see §8. Its orchestrator concurrency
was extracted (R6g/R6h), but the host remains 616 lines of React glue.

---

## 8. Deferred refactor

Honest list, each with its reason.

**R6 is incomplete.** Of the six hotspots in mandate §38:

| Hotspot | Status |
|---|---|
| `TaskWorldInspector.tsx` (397) | done — `inspector/` (R6a) |
| `TaskWorldPage.tsx` (368) | done — 203 lines + `hooks/` (R6b) |
| `TaskWorldCanvas.tsx` (198) | **partial** — `TaskNode` extracted, both under `canvas/`. The remaining 112-line surface was deliberately *not* split into `useCanvasProjectionSync` / `useCanvasViewport` / `useCanvasPersistence`: at that size it is one coherent React Flow integration and splitting it would be slicing by line count (mandate §23), with no render test to catch a regression. |
| `pages/SettingsPage.tsx` (726) | done — **364 lines**, content in `features/settings/{model,sections}/` (R6d) |
| `features/voice/GlobalVoiceHost.tsx` (895) | **done for its concurrency** — **616 lines**. The types, ~170 lines of pure projections and the route policy moved to `features/voice/{model,context}/` (R6e); the orchestrator's concurrency-critical flows (barge-in and final-transcript/continuation) moved to `bargeIn.ts` and `turnFlow.ts` as pure runtime-injected functions (R6g/R6h), covered by 18 interaction tests. The remaining host body is React glue and stays on the over-600 baseline. |
| `index.css` (3389) | done — `index.css` is a **28-line manifest holding no rules**; the rules live in ten files under `src/styles/`, split by cascade layer (R6f). Built CSS is byte-identical. |

**`index.css` was split by cascade layer, not by feature.** Feature grouping is
not available for this file: the rules are interleaved by feature across
cascade layers (`.task-world-*` at 146–210, `.conversation-*` at 222–370,
`.capability-*` at 1028 and 2035, `.system-*` at 2977+), so grouping by feature
means reordering rules within a layer. Splitting by layer keeps every block
contiguous and every `@layer` block inside the Tailwind-processed file, which is
what makes the order provable — and it is proved: the built stylesheet is
byte-identical, md5 `5889431e` before and after.

Two failed attempts preceded it, both caught by that md5 guard rather than by
review:

- Placing `@import` before `@tailwind` (PostCSS's usual requirement) moved the
  unlayered rules ahead of Tailwind's hoisted output — md5 `0153795f`. Reverted.
- Extracting by assumed line ranges folded three *unlayered* `@media` /
  `@keyframes` regions into `@layer` files, displacing 45 rules. Fixed by
  enumerating the file's top-level blocks by brace depth instead of guessing,
  which found five unlayered regions rather than two.

This is the concrete reason the split was refused until a guard existed, and it
is why `current-to-target-map.md` says the import order in `index.css` must not
be rearranged.

**Why `GlobalVoiceHost`'s orchestrator was not split as §21's hooks (R6e), and
how it was then decomposed (R6g/R6h).** §21 asks for `runtime/GlobalVoiceProvider`
plus four hooks. The body carries ~16 `useRef` epoch/identity guards (dispatch,
context identity, capture start) that are *shared across* the state machines
those hooks would separate, a barge-in path that aborts in-flight work and
re-checks the epoch after every `await`, continuation abort controllers, and
echo suppression keyed to the context epoch. Splitting those into hooks is a
redesign of the concurrency model, not a file move — and at the time the
available tests could not catch a mistake: `GlobalVoiceHost.test.tsx` renders to
static markup, exercising neither the async paths nor the guards.

That is why the decomposition was done by *extraction instead of hook-splitting*.
The two flows that carry the cross-`await` guards are now pure functions of an
injected runtime — `runVoiceBargeIn` (`bargeIn.ts`) and
`runFinalTranscriptFlow` (`turnFlow.ts`) — so the concurrency is testable with
controllable promises and the guards' behaviour is pinned by 18 interaction
tests. Two of §21's targets already exist as separate files (`useVoiceCapture.ts`,
`useSpeechPlayback.ts`) and the UI is already out in `GlobalVoiceLeaf.tsx`; the
orchestrator now delegates to the extracted flows. The consequence is stated
rather than hidden: `GlobalVoiceHost.tsx` is still over 600 lines (616) and
**stays on the boundary check's baseline** — but the concurrency logic that the
baseline entry was protecting is out and covered.

**SettingsPage's route entry also stayed in `pages/`.** §20 places it at
`features/settings/pages/`. Route-level pages live in `pages/` throughout this
repository (the surface lazily imports all thirteen from there); moving one
would create a second convention and churn the route table plus three `?raw`
contract tests for no behavioural gain. §20's actual requirement — "SettingsPage
变为 composition page" — is met, and `features/settings/` now exists with
`model/` and `sections/`.

**R3 and R4 remain incomplete in the ways their handoff recorded.**
`modules/task/application` and `modules/workflow/application` service
extraction is not started — business logic still lives in the route modules.
MCP is relocated but not regrouped into the mandated
`protocol/ transport/ runtime/ registry/ result/ security/`, and
`mcp_transport_config` still lives in `app/state.rs`.

**Backend modules not yet under `modules/`:** `agent` and `tools`. `voice`
(incl. a 1195-line runtime), `capability`, `llm` and `secret` were relocated by
R9 — see §14. The target table assigns `agent` and `tools` no module home, so
moving them is a naming decision rather than a relocation.

**One cross-domain edge is recorded, not fixed:**
`modules/resource/binding.rs` reads
`modules::task::execution::MAX_NODE_CONTEXT_ITEM_CHARS`.

**Mandate §4 asks for `core/`; the kernel lives in `shared/`.** Renaming it
would be churn without a boundary change, so `shared/` is kept and the
deviation is recorded.

---

## 9. v1.1 readiness

The v1.1 constraint is that business logic must not be bound to Tauri, so a
Web Surface and a Cloud Model Gateway can be added without rewriting domains.
What this refactor establishes:

- Application logic sits behind `app/` and `modules/*/application`, not in the
  Tauri command layer.
- A boundary check now **enforces** that `@tauri-apps/*` is reached only by
  dynamic import outside `surfaces/desktop/` — the two current call sites
  (`api/controlSession.ts`, `components/chat/externalLink.ts`) already do this.
- `surfaces/workspace` is the surface host; `surfaces/desktop` holds the
  Tauri-specific code. A `surfaces/web` has a declared home.

Not done, and deliberately so (mandate §35): no Web surface, no Cloud Gateway,
no Identity.

## 10. v1.2 readiness

The v1.2 features (Harness Router, Repository Skill Forge, Capability
Evolution) need seams, not implementations. What exists:

- `modules/memory_skill` holds the stable concepts — **candidate, evidence,
  review, version** — separated into their own files, with a documented
  lifecycle. A capability-evolution engine can be built on them.
- Nothing is named `CapabilityEvolutionEngine`, and no such engine is
  anticipated in code (mandate §34/§15).
- Task execution, capability and MCP are out of the giant API files:
  `modules/task` owns execution, `integrations/mcp` owns the MCP runtime.

---

## 11. Verification — the R8 gate

Run on the final commit, sequentially, with cargo's real exit codes captured
(an earlier run piped through `grep` and masked a non-zero exit — see §12).

| Gate | Command | Result |
|---|---|---|
| 1 | `cargo fmt --all -- --check` | **PASS** (exit 0) |
| 2 | `cargo check --workspace --locked -j1` | **PASS** (exit 0) — includes the `yi-lian-qian-yan` Tauri shell |
| 3 | `cargo test --workspace --all-targets --locked -j1` | **PASS** (exit 0) — **21 binaries, 988 passed, 0 failed, 0 ignored** |
| 4 | `cargo check -p yi-lian-qian-yan --locked -j1` | **PASS** (exit 0) |
| 5 | `npm test` | **PASS** — 91 files, **426 passed, 0 failed** |
| 6 | `npm run build` | **PASS** — `tsc` + vite build |
| 7 | `node e2e/core-paths.mjs` (real backend) | **PASS** — `{"status":"passed","graphs_created":2,"settings_viewports":["1280x720","1366x768","1920x1080"],"real_backend":true}` |

Warnings: **3, all pre-existing and unchanged** — `apply_product_migration`
never used, `validate_shared_version` never used, `q_embed` unused variable. No
stage introduced a new warning.

Test counts moved only by addition: backend lib 892 → 892 lib tests plus the
new `architecture_boundaries` target; frontend 90 files / 422 tests → 91 / 426
(the four new boundary tests). **No test was deleted, skipped or weakened.**

Environment note: the machine OOMs under parallel load, so the gate was run
sequentially with nothing else executing, using
`CARGO_TARGET_DIR=E:/cargo-target/yilian-arch` and the repo's required
`jobs = 1`.

### Re-verification after R6d–R6h

R6d through R6h changed frontend files *after* the gate above, so the gate was
re-checked against the final state rather than assumed:

- **No Rust source changed** between the gate commit and the final commit
  (`git diff --stat 41fb035..HEAD -- backend/ src-tauri/` is empty), so gates
  1–4 still describe the same Rust source.
- **`npm test`**: **93 files / 444 passed**, re-run on the final commit. The
  growth over the gate's 91 / 426 is R6g/R6h's 18 interaction tests (6 barge-in,
  12 turn-flow); no test was deleted or weakened.
- **`npm run build`**: passes; built CSS md5 still `5889431e`.
- **Real-backend E2E**: re-run and **passed** — and this one mattered, because
  the E2E drives the real Settings page at three viewports
  (`1280x720`, `1366x768`, `1920x1080`), which is exactly the surface R6d
  rewrote. R6g/R6h changed no route or rendered markup, so the E2E conclusion is
  unchanged by them.

---

## 12. Process notes, recorded honestly

- **One transient gate failure.** An early Gate 3 run reported
  `could not compile yilian-backend (lib test)` while `npm test` and
  `npm run build` were running concurrently — consistent with the memory
  pressure the handoff documents. The clean sequential re-run passed. The gate
  result above is from the clean run.
- **A gate command was wrong and was fixed.** The first Gate 3 invocation
  piped cargo into `grep`, so the pipeline's exit status was `grep`'s and `&&`
  continued past a failure. The corrected run captures `$?` from cargo
  directly. This is recorded because a gate you cannot read the exit code of is
  not a gate.
- **Two commits were created out of band.** An interruption during R7 left
  `85f25c2` (`chore(architecture): enforce module boundaries`, the docs and
  checks) and `736ea3e` (`docs(refactor): record R6 partial and R7 progress`)
  already committed; `41fb035` then added two corrections on top (a stale
  600-line baseline entry and a count in the docs). History was **not**
  rewritten to tidy this, per mandate §32. `git log` therefore shows R7's
  content across three commits rather than one.
- **A baseline was corrected by checking it against reality.** The backend
  600-line list initially contained `modules/task/tests.rs` (554 lines), which
  is not over budget. It was removed, and the backend count corrected from 39
  to 38 in the docs.

---

## 13. What "done" means here

Mandate §49 defines success as a new developer or agent being able to answer
where things are without re-deriving the architecture. That is now answerable
from `docs/architecture/ownership.md` rather than from reading the source:

| Question | Answer |
|---|---|
| Where is model configuration? | `modules/settings` |
| Where is the Task HTTP entry? | `modules/task/api/` |
| Where is Task business logic? | `modules/task` (application layer **not yet extracted**) |
| Where is MCP transport? | `integrations/mcp/` (`legacy_stdio`) |
| Where is MCP runtime? | `integrations/mcp/manager.rs` |
| Where is the voice session owner? | `modules/voice/runtime.rs` (moved there by R9a) |
| Where is Memory-to-Skill? | `modules/memory_skill/` |
| Where is the resource parser? | `modules/resource/parsers/` |
| Who owns the database? | `db/` — unchanged and unmoved |
| What may a module depend on? | `docs/architecture/ownership.md` per module, enforced in part by two checks |

The refactor is **not** complete. Within R6, `TaskWorldCanvas`'s 112-line
surface remains deliberately unsplit; `GlobalVoiceHost`'s orchestrator
concurrency is now decomposed (R6g/R6h) while the host itself stays 616 lines
of React glue; R3 and R4 retain their recorded gaps. What is complete is R0–R5,
all of R6 except that one partial, R7 and R8 — every stage verified green, and
the final state re-verified after the last change.

---

## 14. R9 — post-gate backend relocation

R9 is **not a mandate stage**. It continues the remaining-work list in
`r1-progress-and-handoff.md` §11 after the R8 gate had already passed, so the
gate table in §11 describes the state *before* R9 and was not re-run for it.

Four relocations, one commit each, all `git mv` + facade:

| Stage | Commit | Move | Files |
|---|---|---|---|
| R9a | `cff1c38` | `voice/` → `modules/voice/` | 4 |
| R9b | `ab61d80` | `capability/` → `modules/capability/` | 12 |
| R9c | `2b3cf59` | `llm/` → `integrations/llm/` | 5 |
| R9d | `3378a5d` | `secret/` → `integrations/secret/` | 8 |

All 29 moved files are 100%-similarity renames (`0 insertions, 0 deletions`),
so the moved regions are byte-identical. Each old path keeps a one-glob
compatibility facade, so **no call site changed** and no route, response shape,
schema or migration was touched. The compatibility facade count goes 8 → 12.

### R9 verification

| Check | Result |
|---|---|
| `cargo fmt --all -- --check` | clean after each stage |
| `cargo check -p yilian-backend --all-targets --locked` | **exit 0** after each stage; the same 3 pre-existing warnings, no new ones |
| `cargo test --test architecture_boundaries` | **3 passed / 0 failed** after each stage |
| Moved regions | byte-identical (29 × 100%-similarity renames) |
| `cargo test -p yilian-backend --locked` | **21 binaries, 983 passed, 0 failed, 0 ignored** |
| `cargo test -p yi-lian-qian-yan --locked` | **5 passed / 0 failed** |

Backend 983 + `src-tauri` 5 = **988**, which is exactly the number the R8 gate
recorded for `--workspace --all-targets`. R9 is therefore verified to the same
depth the gate used. The rc.2 secret pinning tests ran and pass, and their
module paths in the output (`integrations::secret::tests::*`,
`integrations::llm::client::tests::*`) are themselves evidence that the moved
tests relocated with their modules.

### The one obstacle, and its workaround

The unit suite did **not** run on the first five attempts. It failed
reproducibly in codegen/link with `rustc-LLVM ERROR: out of memory`, exit
`0xc0000409`. Measured cause: the commit limit is 39.24 GB (15.24 GB RAM + a
24 GB page file) and **36.04 GB was already committed** — spread over 490
processes plus ~8.0 GB of kernel commit, with no single process above 1.2 GB.
There was no hog to close and no way to free the ~5 GB needed.

The real cause is the root `Cargo.toml`, which sets `codegen-units = 1` for
`[profile.dev]` **and** `[profile.dev.package."*"]`, making the lib-test binary
a single giant LLVM codegen unit. The workaround is a per-invocation override
that changes no repo file:

```bash
CARGO_PROFILE_TEST_CODEGEN_UNITS=4 \
CARGO_TARGET_DIR="E:/cargo-target/yilian-arch" \
cargo test -p yilian-backend --locked
```

`[profile.test]` applies only to the local crate's own test targets, not to
dependencies, so this recompiles just the lib-test unit (1m28s) rather than
rebuilding every dependency. No repo file and no test behaviour changed, and the
override is unnecessary on an unloaded machine — it is a workaround, not a
recommended permanent setting.
