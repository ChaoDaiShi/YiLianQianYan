# Baseline v1 — Modular Monolith

> The architecture this repository has converged on, frozen at the end of R2.
> Read this before making a structural change; it says what may move and what
> may not.

| | |
|---|---|
| **Branch** | `refactor/v1-architecture-semantic` |
| **HEAD at baseline** | `a528b4ea02189a54d04f030c6f311353a424c72c` |
| **Base line** | `refactor/v1-architecture-foundation` @ `669db21558b222fb1e5290cd1b27915938a1120a` (R1, frozen) |
| **Shape** | Modular monolith — one deployable, one process, one SQLite file |
| **R1 stage** | 33 commits, R0–R9. Structural: *where code lives* |
| **R2 stage** | 27 commits, S0–S7. Semantic: *what is responsible for what* |
| **Product version** | `1.0.0-rc.2` |

## The shape

```text
Transport / UI  (api/, frontend/)
      ↓
Application     (modules/*/application, app/*)
      ↓
Domain          (modules/*/domain)
      ↓
Ports           (traits the domain/application declare)
      ↑
Infrastructure  (db, integrations, secret, filesystem)
```

Direction is enforced, not merely documented:
`domain_layers_do_not_import_transport` (backend) and the four frontend
boundary checks. `cross_module_globs_are_allowlisted` additionally stops another
module's internals from being re-exported wholesale.

## Layout

**Backend** — `backend/src/`

| Area | Holds |
|---|---|
| `modules/` (7) | `capability`, `memory_skill`, `resource`, `settings`, `task`, `voice`, `workflow` |
| `integrations/` (3) | `llm`, `mcp` (subdivided into `protocol/ transport/ runtime/ registry/ result/ security/` + `config.rs`), `secret` |
| `app/` | `bootstrap`, `state` (`AppServer`), `router`, `lifecycle` — the wiring |
| `api/` | the HTTP transport layer |
| `shared/` | **Protected Kernel** — event envelope, command envelope, resource identity, presence, voice contracts |
| `safety/` | **Protected Kernel** — `SecurityExecutionGateway`, approval, grants, RBAC, audit, sandbox |
| `db/` | schema and migrations |
| `agent/`, `tools/` | the agent runtime and the tool registry/adapters |
| `config/ execution/ interaction/ isolation/ plugin/ utils/ workspace/` | supporting infrastructure |

`backend/src/` holds **only** `lib.rs` and `main.rs` besides those directories.
There is no crate-root compatibility shim, and
`no_removed_facade_paths` keeps it that way.

**Frontend** — `frontend/src/`

| Area | Holds |
|---|---|
| `core/api/http.ts` | `API_BASE`, `request`, `requestResult`, the one place the control-session header is attached |
| `api/` | one module per domain, plus `legacy.ts` — a **re-export surface with no declarations** |
| `features/` | `task-world`, `voice` (+ `runtime/`), `capabilities`, `execution`, `llm`, `mcp`, `memory`, `resources`, `security`, `skills`, `tasks` |
| `surfaces/` | `workspace` (route table), `desktop/` (the only place a static Tauri import is allowed) |

## Compatibility commitments

| Contract | State |
|---|---|
| REST | **Frozen.** 178 handlers, no method, path or handler-signature change across R1 and R2 |
| Frontend route table | `surfaces/workspace/WorkspaceSurface.tsx` byte-identical across both stages |
| Database schema / migrations | **Unchanged.** `db/migrations.rs` untouched; no migration added, renumbered, edited or re-owned. Fusion namespaces: Shared `0–999`, v1 `1000–1999`; v2 owns its own and this line must not borrow it |
| `db/` implementation files | 10 files changed by R2, **import paths only** (+25 / −25 lines). No query, table or row shape changed |
| Secrets | KEEP / REPLACE / DELETE semantics unchanged since rc.2, pinned by four tests |
| Existing user data | No storage-root, file-format or persistence change |
| v2 | Not touched; `v1_does_not_reach_into_v2` enforces it |

## Public API

`docs/architecture/public-api-policy.md` is the authority. In one line: **Rust
`pub` visibility is not a support commitment.**

| Layer | Contents | Promise |
|---|---|---|
| A — Product Public Contracts | REST, Command, Event, Projection, MCP, Skill, Capability, Resource, Artifact | Supported, release-versioned |
| B — Workspace Internal Host API | `safety::ControlSession`, `serve_in_background_with_control_token`, `create_server`, `serve`, `serve_in_background`, `AppServer` | Explicit, no third-party stability |
| C — Internal Implementation | everything else under `modules/`, `integrations/`, `app/`, `api/`, `db/`, … | No promise; may be renamed or deleted in any release |

`backend/Cargo.toml` sets `publish = false`, which makes layer C mechanical. A
future Rust SDK is a separate `yilian-sdk` crate, never a restoration of the
twelve removed facades.

## Protected Kernel — do not change semantics

- `shared/` — event envelope, command envelope, resource identity, presence,
  voice contracts.
- `safety/` — `SecurityExecutionGateway`, approval, capability security
  semantics. The chain
  `Workflow → SecurityExecutionGateway → Approval → Execution → Verification`
  must survive any refactor intact.

A change here is a `SHARED_CONTRACT_CHANGE_REQUIRED` stop, not a refactor.

## Verification

Two gates, and they answer different questions. Run both before declaring a
structural change done.

**Unit / Integration Architecture Gate** — behaviour and direction.

```bash
export CARGO_TARGET_DIR="E:/cargo-target/yilian/arch-r2"   # never share this dir
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo test  --workspace --all-targets --locked
cargo check -p yi-lian-qian-yan --locked
cd frontend && npm test && npm run build
```

`CARGO_PROFILE_TEST_CODEGEN_UNITS=4` may be passed **at the command level** on
this machine (the root `Cargo.toml` sets `codegen-units = 1` and the commit
limit is tight). Never edit `Cargo.toml` or `.cargo/config.toml` for it.

Baseline totals at this HEAD: **21 test binaries · 988 passed · 0 failed**;
frontend **93 files / 445 tests**; backend architecture boundaries **5**;
frontend architecture boundaries **5**.

**Real Application E2E** — the product actually runs.

```bash
cd frontend && npm run test:e2e
```

Drives a real `yilian-server.exe` against a real browser. See
`docs/refactor/r2-final-report.md` §7b for what it covers and its prerequisite.

## Related documents

- [module-catalog.md](module-catalog.md) — what lives where
- [ownership.md](ownership.md) — responsibilities and invariants
- [dependency-rules.md](dependency-rules.md) — direction of dependencies
- [module-boundaries.md](module-boundaries.md) — the boundaries
- [public-api-policy.md](public-api-policy.md) — the three layers
- [compatibility-facades.md](compatibility-facades.md) — what R2 removed
- [event-command-contract.md](event-command-contract.md) — the kernel contracts
- `docs/refactor/r1-final-report.md`, `docs/refactor/r2-final-report.md`
