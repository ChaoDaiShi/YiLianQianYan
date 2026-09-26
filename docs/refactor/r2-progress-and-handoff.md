# R2 — Semantic Boundaries and Facade Convergence

> **Branch:** `refactor/v1-architecture-semantic`
> **Base:** `refactor/v1-architecture-foundation` @
> `669db21558b222fb1e5290cd1b27915938a1120a` (R1, frozen)
> **Status:** S0–S7 done and verified. Both closure gates PASS —
> Unit/Integration Architecture Gate and Real Application E2E, reported
> separately in §8b.
> Final report: `r2-final-report.md`. Companion documents:
> `r1-final-report.md`, `../architecture/baseline-v1.md` (the frozen
> architecture this line converged on), `../architecture/compatibility-facades.md`,
> `../architecture/public-api-policy.md`, `../architecture/module-catalog.md`.

R1 answered *where code lives*. R2 answers *what is responsible for what*, and
closes the migration scaffolding R1 left behind. Same product; no route, schema,
migration or secret semantic changes.

---

## 1. Stage status

| Stage | Scope | Status | Commits |
|---|---|---|---|
| S0 | Close the R1 docs against final code facts | done | `a3bb59e` |
| S1 | `modules/task/application/` services | done | `ceb03e7` |
| S2 | `modules/workflow/application/` services | done | `5fc6478` |
| S3 | MCP modularisation + `legacy_stdio` split | done | `e8df56d`, `171db97`, `f091c2c` |
| S4 | Compatibility facade convergence | done | `e3a19e1`, `8773b42`, `d2c8ad4` |
| S5 | Voice runtime isolation | done | `0e7e99b` |
| S6a | Frontend HTTP core | done | `9ca9ad6` |
| S6b | Drain `api/legacy.ts` into domain entrypoints | done | 9 domains, `a058dee` … `59bfc42` |
| S6c | `legacy.ts` → re-export surface; split `taskWorld` | done | `e9646de`, `587f108`, `a0165be` |
| S7 | Full gate + R2 report set | done | see `r2-final-report.md` |

---

## 2. S1 — Task application layer

`modules/task/application/` now holds the use cases the routes call:

- `graph_service` — create-from-nodes vs plan-from-goal, plus command
  reconciliation before a detail read.
- `execution_service` — executor resolution, the existing-workflow provider,
  dispatch folding, validation policy. This is the ~160 lines that used to sit
  inside `execution_routes.rs`.
- `review_service` — revision-guarded planner review.

`execution_routes.rs` went 294 → ~110 lines, `graph_routes.rs` 70 → 20,
`review_routes.rs` 44 → 22. Typed errors (`CreateGraphError`,
`ReviewGraphError`) keep the layer transport-free; `api/mapping.rs` owns the
status/error-code wire contract, which is unchanged.

**Not extracted, deliberately:** the node, checkpoint, command and rerun
handlers were already thin extract-validate-call-map adapters over
`task_world`. Wrapping a single call in a service would be a second name for the
same thing, so the planned `node_service` / `checkpoint_service` / `rerun_service`
were **not** created. `api/tests.rs` was later split into
`api/tests/{mod,execution,planning}.rs` after the S4 import rewrite pushed it
over the 600-line budget.

## 3. S2 — Workflow application layer

- `run_service` — start a run (persist, register cancel token, spawn the
  runner), execute-for-task-harness, cancel a run, and the shared gateway /
  agent-executor builders. `run_routes.rs` 290 → ~80 lines.
- `graph_service` — create and update share one validate-then-persist path.
- `approval_service` — `resume.rs` renamed with `git mv` (96% similarity), only
  its `super::` paths adjusted. Approval consumption, the TOCTOU-safe
  re-evaluation and one-shot replay protection are byte-identical.

The task harness now reaches
`modules::workflow::application::run_service` directly instead of through the
`crate::api::workflow_runtime` facade.

## 4. S3 — MCP modularisation

`integrations/mcp` was a flat directory of seventeen files. It is now grouped by
role with the flat public surface preserved: `protocol/`, `transport/`,
`runtime/`, `registry/`, `result/`, `security/`, plus `config.rs`.

- `McpTransportConfig` and `mcp_transport_config` moved out of `app/state.rs`
  into `config.rs`; the `McpTransport` trait stayed with its implementations.
- The old `protocol.rs` split into `protocol/version.rs` (era and `_meta`) and
  `security/header.rs` (CR/LF rejection, header encoding) — the two halves are a
  protocol concern and a security boundary.
- The catalogue parsers split by shape: `registry/` reads what a server
  *advertises* (`*/list` pages), `result/` reads what one *operation returned*
  (`tools/call`, `*/get`, `read`, including the MRTR `input_required` outcome).
- `legacy_stdio.rs` (1,384 lines, over half of it tests) became a directory of
  six modules plus split tests. `spawn_stdio_child` and `resolve_env` replace a
  block that was duplicated verbatim between the probe and call paths.

## 5. S4 — Facade convergence

All twelve R1 glob shims are gone; `backend/src/` now holds only `lib.rs` and
`main.rs`. Classification, the old→owner table and the "how to move a module
without leaving a shim" note are in
[compatibility-facades.md](../architecture/compatibility-facades.md).

Two findings the drain exposed, both fixed:

1. **A real layering violation the facade was masking.**
   `modules/settings/domain/readiness.rs` imported `crate::server::AppServer` and
   reached into `server.db` / `server.secret_resolver`. The boundary check greps
   for `crate::app`, so the shim's other name kept it invisible; removing the
   shim made `domain_layers_do_not_import_transport` fail. The domain functions
   now name `&Database` and `&Arc<SecretResolver>`.
2. **`modules/task/api/tests.rs` crossed the line budget** once the drain
   expanded its imports. Split into a directory.

### Public API note

Removing the shims removes Rust-visible paths from the `yilian-backend` library
target (`yilian_backend::task::*`, `::server::*`, `::secret::*`, …). The only
consumer outside the crate is `src-tauri`, which uses
`yilian_backend::safety::ControlSession` and
`yilian_backend::serve_in_background_with_control_token` — neither is a shim, so
it is unaffected. Recorded here because it is the one judgement call in S4.

## 6. S5 — Voice runtime isolation

`features/voice/runtime/useGlobalVoiceRuntime.ts` owns the coordination state
that decides whether an async answer may still land: the dispatch,
capture-operation and context-identity epochs, the latest session and context,
the one-shot echo evidence, and the continuation `AbortController`. The host no
longer names an epoch at all and went 616 → 576 lines.

The echo-evidence guard had been written out twice (in `startCapture` and in the
hands-free `capture.start` wrapper) with identical conditions; it is one
`recordEchoEvidence` call now.

## 7. S6 — Frontend API (done)

**S6a.** `src/core/api/http.ts` holds `API_BASE`, `request`, `requestResult`,
`ApiResult`/`ApiFailure`, sharing one `jsonInit` helper so the control-session
header is attached in exactly one place. `api/transport.ts` points at it.

One boundary wart, recorded not hidden: `core/api/http.ts` imports
`controlSessionHeaders` from `api/controlSession.ts` (a core → api edge).
`controlSession.ts` stays where it is because ~15 modules and their
`vi.mock("./controlSession", ...)` calls depend on that path; re-exporting it
from `core/` would silently stop those mocks applying to the HTTP core.

**S6b.** Nine domains moved out of `legacy.ts` into their entrypoints:
`conversations`, `chat`, `system` (settings, models, tools, health, logs,
security grants, isolation), `plugins` (MCP, subagents, skills), `memory`,
`workflows`, `tasks` (+ agents/teams), `workspaces`, `capabilities`.

**S6c.** `api/legacy.ts` is **62 lines of re-exports and declares nothing** —
1,335 → 62. `api/taskWorld.ts` (601 lines) became
`api/taskWorld/{types,transport,calls,index}.ts`; `index.ts` re-exports, so
`from "../api/taskWorld"` resolves unchanged for all ten call sites.

Three things worth carrying forward:

- **Ownership comes from the entrypoint's name list, not from the section
  comments.** `legacy.ts`'s `// ── Health ──` marker covered the chat/SSE block,
  and the workflow-graph and workflow-run calls sat under it too. Cutting by
  section left `listWorkflowGraphs` behind; `domainBoundary.test.ts` caught it
  because it asserts the `client.ts` facade and the domain entrypoint are the
  *same function objects*.
- **Item boundaries need a parser, not brace counting.** A signature with a
  balanced-brace parameter object (`updateWorkflowGraph`) and a union type whose
  members end in `;` (`WorkflowApprovalEvent`) both make a text heuristic stop
  early and silently truncate. Two attempts corrupted `legacy.ts` that way; the
  drain helper was rewritten to use `ts.createSourceFile` spans.
- **A new boundary check keeps `legacy.ts` a surface**: it fails if the file
  declares a function, const, class, interface or alias again. That is the
  invariant that matters — a second implementation would compile, and the two
  would drift silently.

`features/voice/GlobalVoiceHost.tsx` is 576 lines and `pages/PluginsPage.tsx`
splitting was optional in S6 and was **not** done. Both are noted in
`r2-final-report.md` §8.

**How to verify a step:** `npx tsc --noEmit`, then `npx vitest run src/api`.

## 8. Verification used

### 8a. Per-stage — focused, not the full gate

```bash
CARGO_PROFILE_TEST_CODEGEN_UNITS=16 CARGO_TARGET_DIR="E:/cargo-target/yilian/arch-r2" \
  cargo check --workspace --all-targets --locked
# then, to run tests, build and invoke the reported executable directly:
CARGO_PROFILE_TEST_CODEGEN_UNITS=16 CARGO_TARGET_DIR="E:/cargo-target/yilian/arch-r2" \
  cargo test -p yilian-backend --locked --lib --no-run 2>&1 | grep Executable
```

**Read §9's first hazard before trusting a `cargo test` result** — it describes
the shared target dir that made a `cargo test` run another worktree's binary.
The isolated dir in the commands above is the fix; the direct-executable pattern
is the second line of defence.

Totals reached at S5, re-confirmed after every later stage:

| Suite | Result |
|---|---|
| `yilian-backend` lib | 892 passed |
| 18 integration binaries | 91 passed |
| `architecture_boundaries` | 3 passed |
| `yi-lian-qian-yan` | 5 passed |
| frontend | 93 files / 444 tests passed |
| `npm run build` | succeeds |

### 8b. Closure — two gates, run once each

They answer different questions and are reported separately in
`r2-final-report.md` §6a/§6b.

**Unit / Integration Architecture Gate** — behaviour and dependency direction
from the inside. Full commands in `r2-final-report.md` §6a. Result: fmt PASS,
check PASS, **21 Rust test binaries · 988 passed · 0 failed**, Tauri check PASS,
frontend **93 files / 445 tests** PASS, build PASS, backend boundaries **5**
PASS, frontend boundaries **5** PASS.

**Real Application E2E** — the product actually running. `npm run test:e2e`,
which spawns the real `yilian-server.exe` plus Vite and drives a real Chromium.
Result: `{"status":"passed","graphs_created":2,"settings_viewports":["1280x720","1366x768","1920x1080"],"real_backend":true}`.

**It does not need a Tauri bundle.** The only prerequisite is the server binary
at `<worktree>/target/debug/yilian-server.exe` — a hardcoded path, not derived
from `CARGO_TARGET_DIR`:

```bash
cp "E:/cargo-target/yilian/arch-r2/debug/yilian-server.exe" target/debug/
```

Route-path set unchanged throughout (178 handlers; no method, path or handler
signature touched). No migration, schema or secret-semantics change. Ten `db/`
files changed by **import path only** (+25 / −25); `migrations.rs` untouched.

## 9. Environment hazards

Three, each of which cost real time to rediscover.

**1. Never share a Cargo target dir with the other worktree.**
`E:/cargo-target/yilian-arch` is shared with
`F:/项目开发/忆涟千言/YiLianQianYan` (branch `prep/shared-foundation-v1-v2`, the
flat pre-R1 layout). With a shared dir, `cargo test` can execute *that*
worktree's test binary. Symptom: `0 passed; N filtered out` for a filter that
should match, or pre-R1 test names (`mcp::tests::…`, `mcp_runtime::tests::…`,
`workflow::tests::…`) instead of `integrations::mcp::legacy_stdio::tests::…`,
`modules::workflow::tests::…`. It bit the S3c verification once.

The fix, applied in R2's closure, is a target dir this line owns:

```bash
export CARGO_TARGET_DIR="E:/cargo-target/yilian/arch-r2"
```

Second line of defence, if a result ever looks wrong — build, then run the
reported executable directly, which bypasses cargo's binary selection:

```bash
cargo test -p yilian-backend --locked --lib --no-run 2>&1 | grep Executable
"E:/cargo-target/yilian/arch-r2/debug/deps/yilian_backend-<hash>.exe" modules::task
```

The isolated dir is cold, so the first build is long. Do not "save time" by
pointing back at the shared one.

**2. `codegen-units = 1` plus a nearly-full commit limit.**
`rustc-LLVM ERROR: out of memory` / exit `0xc0000409` on the lib-test unit. Pass
`CARGO_PROFILE_TEST_CODEGEN_UNITS=4` per invocation; it applies only to
`[profile.test]`, recompiles just the local test unit, and changes no repo file.
Do not "fix" `.cargo/config.toml` (`jobs = 1` is deliberate) or the root
`Cargo.toml` profile.

**3. The E2E wants the server binary at a path `CARGO_TARGET_DIR` does not
control.** `frontend/e2e/core-paths.mjs` spawns
`<worktree>/target/debug/yilian-server.exe`. With an isolated target dir, that
file has to be copied into place once:

```bash
cp "E:/cargo-target/yilian/arch-r2/debug/yilian-server.exe" target/debug/
```

`target/` is gitignored, so this stages nothing.

## 10. What R2 did not do

- No UI redesign, no font-size change, no Canvas viewport or Task node layout
  change, no new MCP Canvas Node.
- No GIS / GeoServer / uDig, no v1.1 (Identity, Cloud Gateway, Web Surface), no
  v1.2 (Harness Router, Repository Skill Forge, Capability Evolution), no v2.
- No new Port trait per SQLite function — the application services take
  `&AppServer` or their two real dependencies, matching the house pattern.
- `modules/artifact`, `modules/approval` and `integrations/filesystem` remain
  `not started` in the target map. Each is a *new* module rather than a move, so
  it is product work, not refactoring.
