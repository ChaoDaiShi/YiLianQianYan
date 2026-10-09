# R2 Architecture Refactor — Final Report

> **Branch:** `refactor/v1-architecture-semantic`
> **Base:** `refactor/v1-architecture-foundation` @
> `669db21558b222fb1e5290cd1b27915938a1120a` (R1, frozen)
> **Status:** S0–S6 executed. **Full Gate PASS**, run once in the isolated target
> dir `E:/cargo-target/yilian/arch-r2`.
>
> R1 answered *where code lives*. R2 answers *what business logic is responsible
> for*, and closes the migration scaffolding R1 left behind. Same product: no
> route, schema, migration or secret-semantic change.
>
> Companion documents: `r2-progress-and-handoff.md` (per-stage detail),
> `../architecture/compatibility-facades.md`,
> `../architecture/public-api-policy.md`,
> `../architecture/module-catalog.md`,
> `v1.1-readiness.md`, `v1.2-readiness.md`.

---

## 1. Answer block

| Question | Answer |
|---|---|
| Legacy facade removed | **YES** — all twelve, no replacements |
| Supported Rust SDK | **NO** — and none is planned; see `public-api-policy.md` |
| Workspace host API | **EXPLICIT** — `safety::ControlSession`, `serve_in_background_with_control_token`, listed in the policy doc |
| Backend crate publishable | **NO** — `backend/Cargo.toml` sets `publish = false` |
| `legacy.ts` implementation owner | **NO** — 62 lines, 19 re-export statements, 0 declarations |
| Duplicate API implementations | **0** — 197 value exports across `src/api/`, each defined exactly once |
| Cross-worktree Cargo target isolation | **PASS** — `E:/cargo-target/yilian/arch-r2`, not shared |

No stop condition was hit: `PUBLIC_API_CHANGE_REQUIRED` was **not** raised,
because the removed paths are internal implementation by the policy the user
ratified (see §5). No DB, migration, security-semantics or shared-contract change
was made.

---

## 2. Compatibility conclusion

| Check | Result | Evidence |
|---|---|---|
| Database compatible | **YES** | `backend/src/db/migrations.rs` is untouched, and no migration was added, renumbered, edited or re-owned. Ten other `db/` files changed by **+25 / −25 lines** — import paths only (`use crate::task::*` → `use crate::modules::task::*`). No query, table or row shape changed. |
| REST compatible | **YES** | Route-path set unchanged: **178 handlers**, no method, path or handler-signature change in any R2 commit. |
| Frontend route compatible | **YES** | `surfaces/workspace/WorkspaceSurface.tsx` untouched. |
| Secret compatible | **YES** | KEEP / REPLACE / DELETE moved verbatim in R1 and untouched in R2; the four pinning tests pass. |
| Existing user data compatible | **YES** | No schema, migration, storage-root or file-format change. |
| v2 touched | **NO** | `v1_does_not_reach_into_v2` passes. |
| Behavior changed | **NO** — except one deliberate fix, §4. | |

An earlier revision of this table claimed no file under `db/` changed. That was
wrong: ten did. The correction above is the accurate statement, and it is the
kind of claim worth re-deriving rather than asserting — `git diff --stat` against
the R1 base is the check.

---

## 3. What each stage changed

| Stage | Change | Commits |
|---|---|---|
| S0 | R1 docs closed against final code facts | `a3bb59e` |
| S1 | `modules/task/application/` — graph, execution, review services. `execution_routes.rs` 294 → ~110 lines; the 160-line dispatch moved to `execution_service.rs` | `ceb03e7` |
| S2 | `modules/workflow/application/` — run, graph, approval services. `run_routes.rs` 290 → ~80; `resume.rs` → `application/approval_service.rs` by `git mv` (96%) | `5fc6478` |
| S3 | MCP grouped into `protocol/ transport/ runtime/ registry/ result/ security/` + `config.rs`; `mcp_transport_config` moved out of `app/state.rs`; `legacy_stdio.rs` (1,384 lines) → 6 modules + split tests | `e8df56d`, `171db97`, `f091c2c` |
| S4 | Twelve crate-root compatibility shims drained and deleted. `backend/src/` now holds only `lib.rs` and `main.rs` | `e3a19e1`, `8773b42`, `d2c8ad4` |
| S5 | `features/voice/runtime/useGlobalVoiceRuntime.ts` owns the epochs, echo evidence and continuation abort. Host 616 → 576 lines | `0e7e99b` |
| S6a | `core/api/http.ts` — the HTTP core, one `jsonInit` | `9ca9ad6` |
| S6b | Nine domains moved out of `legacy.ts` into their entrypoints | `a058dee` … `59bfc42` |
| S6c | `legacy.ts` 1,335 → 62 lines, re-exports only; `taskWorld.ts` 601 → `taskWorld/{types,transport,calls,index}.ts` | `e9646de`, `587f108`, `a0165be` |
| — | `publish = false`; `public-api-policy.md`, `module-catalog.md`, readiness docs | this commit |

### S1 note — what was deliberately *not* built

The plan listed `node_service`, `checkpoint_service` and `rerun_service`. They
were not created: the node, checkpoint, command and rerun handlers were already
thin extract-validate-call-map adapters over `task_world`, and wrapping a single
call in a service would have produced a second name for the same thing. Creating
them would have been the speculative abstraction the mandate forbids.

## 4. The one behaviour change, and why it was correct

Draining the `server` shim made `domain_layers_do_not_import_transport` **fail**,
which it had not done before. `modules/settings/domain/readiness.rs` was
importing `crate::server::AppServer` and reaching into `server.db` and
`server.secret_resolver`. The boundary check greps for `crate::app`; the shim's
other name kept the violation invisible.

The fix: `build_provider_readiness` and `active_runtime_model` now take
`&Database` and `&Arc<SecretResolver>` — the two things they use — instead of the
composition root. The domain no longer depends on `app`. Observable behaviour is
identical.

This is worth recording as a finding, not just a fix: the boundary check has a
blind spot for **indirection through a re-export**. A future shim of any kind
reintroduces it.

## 5. Public API

Ratified policy, written up in
[public-api-policy.md](../architecture/public-api-policy.md):

- **Product Public Contracts** — REST, Command, Event, Projection, and the
  MCP / Skill / Capability / Resource / Artifact contracts. These are supported.
- **Workspace Internal Host API** — `safety::ControlSession` and
  `serve_in_background_with_control_token`, consumed by `src-tauri`. Explicit,
  no third-party stability promise.
- **Internal Implementation** — everything else. No semver-path compatibility.

The twelve removed paths were layer 3. `src-tauri`, the only consumer outside
`yilian-backend`, uses only layer-2 paths and is unaffected.

`publish = false` was added because no script, workflow or documented step in
this repository runs `cargo publish`, and no third party depends on the crate.
This makes layer 3 mechanical: an unpublished crate cannot attract a dependent
who mistakes a `pub` path for a contract. If a Rust SDK is ever wanted, the
policy says to build a separate `yilian-sdk` and **not** to restore the facades.

## 6. Validation — two gates

They answer different questions. The first proves behaviour and direction from
the inside; the second proves the product actually runs.

`CARGO_TARGET_DIR="E:/cargo-target/yilian/arch-r2"` throughout. That directory
was created for R2 and is **not shared** with the other worktree — see §10.

---

## 6a. Unit / Integration Architecture Gate

```bash
export CARGO_TARGET_DIR="E:/cargo-target/yilian/arch-r2"
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
CARGO_PROFILE_TEST_CODEGEN_UNITS=4 cargo test --workspace --all-targets --locked
cargo check -p yi-lian-qian-yan --locked
cd frontend && npm test && npm run build
```

`CARGO_PROFILE_TEST_CODEGEN_UNITS=4` is a **command-level** override, permitted
by the mandate for LLVM commit-memory pressure, which this machine demonstrably
has (documented in `r2-progress-and-handoff.md` §9). No repo file was changed to
work around the environment: `Cargo.toml`'s `codegen-units = 1` and
`.cargo/config.toml`'s `jobs = 1` are both intact.

| Suite | Result |
|---|---|
| `cargo fmt --check` | PASS |
| `cargo check --workspace --all-targets` | PASS |
| `cargo test --workspace --all-targets` | 21 binaries · 988 passed · 0 failed (§7a) |
| `cargo check -p yi-lian-qian-yan` | PASS |
| `npm test` | 93 files / 445 tests PASS |
| `npm run build` | PASS |
| Backend architecture boundaries | 5 PASS |
| Frontend architecture boundaries | 5 PASS |

## 6b. Real Application E2E

```bash
cd frontend && npm run test:e2e     # node e2e/core-paths.mjs
```

**It does not need a Tauri bundle.** It spawns the real backend binary at
`<worktree>/target/debug/yilian-server.exe` — a path hardcoded in the script,
not derived from `CARGO_TARGET_DIR` — plus a Vite dev server, then drives a real
system Chromium (Edge or Chrome) with `playwright-core`. The one prerequisite is
the binary in that location:

```bash
cp "E:/cargo-target/yilian/arch-r2/debug/yilian-server.exe" target/debug/
```

It uses an isolated temp data dir and workspace, free ports, and a fixed control
token, so it does not touch real user data.

Result at this HEAD:

```json
{"status":"passed","graphs_created":2,
 "settings_viewports":["1280x720","1366x768","1920x1080"],
 "real_backend":true,
 "browser":"C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe"}
```

What that pass actually covers, in order — this is the list to check when
deciding whether an E2E is meaningful for a change:

1. Backend `/api/health` and the frontend `/tasks` route both come up.
2. The first-run setup dialog is dismissed if present.
3. Two task canvases are created and get **distinct graph identities** — the
   script fails if the second URL equals the first.
4. A task node is added, `.task-world-node` renders, auto-layout runs, and the
   task centre shows the expected graph count after each creation.
5. `/capabilities` renders the managed-skill heading and the protected managed
   import route is reachable.
6. `/system` renders the monitor heading.
7. `/chat` opens the voice surface and the voice pill renders.
8. `/settings?section=model` is checked at **1280×720, 1366×768 and 1920×1080**:
   no horizontal overflow, the save button is visible and clickable, and the
   voice pill does not overlap it.
9. **Zero browser console errors or page errors** across the whole run.

Steps 3, 8 and 9 are the ones with teeth: canvas identity, cross-viewport
layout regression, and a silent runtime error that a unit test cannot see.

---

## 7a. Unit / Integration Architecture Gate results

Run once, end to end, in `E:/cargo-target/yilian/arch-r2` (cold — this target
dir was created for R2, so the numbers are from a clean build, not a warm
incremental one).

**Rust — 21 test binaries, 988 passed, 0 failed.**

| Binary | Tests |
|---|---|
| `yilian-backend` lib | 892 |
| `yi-lian-qian-yan` main | 5 |
| `architecture_boundaries` | 3 → **5** after the closure checks in §7c |
| `canvas_view` | 2 |
| `cycle6_contracts` | 5 |
| `cycle6_conversation_anchor` | 5 |
| `cycle6_interaction_router` | 6 |
| `cycle6_interaction_safety` | 8 |
| `cycle6_minimax_stt` | 10 |
| `cycle6_task_commands` | 9 |
| `cycle6_task_narration` | 2 |
| `cycle6_voice_api` | 7 |
| `cycle6_voice_provider` | 7 |
| `cycle6_voice_runtime` | 8 |
| `runtime_smoke` | 1 |
| `security_execution` | 10 |
| `shared_foundation` | 1 |
| `task_canvas_api` | 1 |
| `task_canvas_persistence` | 5 |
| `task_harness_api` | 1 |
| `yilian-server` main | 0 (binary target, no tests) |

**988 is the same total R1's gate produced** (983 backend + 5 Tauri). No test was
lost, added, weakened or filtered out across R2 — the count is the check that the
S1 test-file split and the S6 drain did not drop coverage.

**Frontend — 93 files / 445 tests passed, build succeeded.** 445 is one more than
the R1 baseline of 444: the new `legacy.ts` boundary check.

## 7b. Real Application E2E result

```json
{"status":"passed","graphs_created":2,
 "settings_viewports":["1280x720","1366x768","1920x1080"],
 "real_backend":true,
 "browser":"C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe"}
```

Run against the tree at `93b332b`, which is the same tree the §7a gate passed on,
and again on the closure tree after `a528b4e`. A real `yilian-server.exe` built
from that tree, a real Vite server, and a real Edge: two canvases created with
distinct identities, the node and auto-layout path exercised, capabilities /
system / chat surfaces rendered, settings checked at three viewports, and zero
browser console errors.

`a528b4e` touches only `backend/tests/architecture_boundaries.rs`, an
integration-test target outside the lib and bin, so it cannot change the server
binary's behaviour. That is why one E2E run covers both — a claim about *what
changed*, not a licence to skip the E2E for the next real change.

The E2E closes the gap the earlier revision of this report flagged: the unit gate
cannot see a runtime console error, a cross-viewport layout regression, or a
graph identity collision.

**One unreproduced failure, recorded rather than dropped.** An invocation of
`npm test && npm run build && npm run test:e2e` chained into a single command
failed after `npm run test:e2e` had already passed standalone. The diagnostic
output was not captured, so the cause is **not known**. Subsequent runs passed:
the standalone re-run, and a re-run of the same chained form. No process was left
listening on 9420 or 1420, and no stray `yilian-server` / `node` existed
afterwards.

So the observed record is **four passes and one failure**, with the failure
unexplained. The E2E is not proven flaky and not proven stable. If it fails
again, capture the `[backend tail]` / `[frontend tail]` output the script prints
before rethrowing — that is what would identify it. Treating this as a known
flake would be wrong; treating it as green would be worse.

## 7c. Closure boundary checks

Added after the two gates, so they are not in the §7a run above. Both were
verified to **fail** before being trusted — see the commit message.

| Check | Guards | Result |
|---|---|---|
| `no_removed_facade_paths` | the 12 removed shims have 0 references in `src/`, and `lib.rs` declares none | PASS |
| `cross_module_globs_are_allowlisted` | every `pub use crate::…::*` is on an explicit allowlist; `lib.rs` never globs | PASS |

`architecture_boundaries` is therefore **5 tests**, not the 3 in the §7a table.
The two new checks are source-text and dependency-free; `syn` was rejected as a
large dependency for checks that only read text.

Current state: **5 backend boundary tests + 5 frontend boundary tests, all
passing.**

## 8. Known limitations

1. **`db/`, `safety/`, `agent/`, `tools/` are not under `modules/`.** They are
   unrelocated, not unowned. Moving them is the largest structural work still
   open; R2 deliberately did not attempt it.
2. **`core/api/http.ts` imports `controlSessionHeaders` from `api/controlSession.ts`** —
   a core → api edge. Left alone because ~15 modules and their
   `vi.mock("./controlSession", ...)` calls depend on that path; re-exporting it
   from `core/` would silently stop those mocks applying to the HTTP core. A
   15-test-file change for no boundary gain.
3. **Two same-named types, `ApiResult`.** `core/api/http.ts` exports
   `ApiResult`/`ApiFailure`; `api/taskWorld/types.ts` exports
   `ApiResult`/`ApiError` (the task-world envelope carries `code`). Documented in
   the task-world types file. They are not interchangeable.
4. **The boundary checks are source-text, not type-level.** They catch direction
   and shape, not truth. §4 is the proof that indirection can hide a violation
   from them.
5. **`components/chat/ChatView.tsx` and `pages/PluginsPage.tsx` remain above the
   600-line budget.** Recorded debt; splitting `PluginsPage` was optional in S6
   and was not done, since it is product work with no architecture benefit.
6. **No E2E run in this gate.** The frontend `test:e2e` script drives a built
   desktop app; the mandate's S7 list named architecture checks, tests and build,
   and the E2E path requires the Tauri bundle. Flagged rather than silently
   skipped.

## 9. Recommended next step

Close R2 and merge `refactor/v1-architecture-semantic` into
`refactor/v1-architecture-foundation` — the R1 line is the stable base and R2 is
purely additive on top of it. Then open the next line from the readiness
documents, not from this branch:

- v1.1 — `v1.1-readiness.md` records a clean seam and no head start.
- v1.2 — `v1.2-readiness.md` records the concepts and the seams, and no engine.

The two structural items worth scheduling independently are the
`agent/`, `tools/`, `safety/`, `db/` relocation (§8.1) and the `controlSession`
edge (§8.2).
