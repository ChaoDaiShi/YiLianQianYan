# Module Catalog

> What lives where, and who owns it. Companion to
> [ownership.md](ownership.md) (responsibilities and invariants) and
> [module-boundaries.md](module-boundaries.md) (the rules).
> Last updated at R2 / S6.

## Backend (`backend/src/`)

### Product domains — `modules/`

One shape: `mod.rs` (public surface, re-exports only) + `domain/`, `application/`
and `api/` where the module has them.

| Module | Holds | Layering |
|---|---|---|
| `capability` | the unified registry: builtin / MCP / subagent / agent / workflow / skill providers, imports, system preferences | `provider`, `registry`, `builtin`, `runtime_providers`, `imports`, `import_owner`, `import_store`, `system_preferences` |
| `memory_skill` | user-reviewed evidence candidates and the managed skill store | `candidate`, `evidence`, `sensitivity`, `validator`, `review`, `version`, `repository`, `service`, `store` |
| `resource` | bounded file extraction, preview and node binding | `limits` → `parsers/` (archive, docx, image, pdf, text, xlsx) → `preview`; `ingest`, `binding` are the use cases |
| `settings` | model and voice provider config, the secret lifecycle, readiness | `domain/` (policy, readiness) / `application/` (service, update, secret_lifecycle) / `api/` |
| `task` | Task World: the product layer above a workflow run | `application/` (graph, execution, review services) / `api/` (canvas, command, execution, graph, node, review routes + `mapping`) + graph, execution, planner, orchestrator, supervisor, projection, timeline, artifact, approval |
| `voice` | the global voice session runtime and provider adapters | `runtime`, `provider`, `dispatch` |
| `workflow` | workflow graphs and their runs | `application/` (run, graph, approval services) / `api/` + definition, validation, state_machine, executor, runner, run |

### Infrastructure — `integrations/`

| Module | Holds |
|---|---|
| `llm` | the OpenAI-compatible client, request/response types, usage accounting |
| `mcp` | MCP. Grouped by role in R2 / S3: `config.rs` (transport config + the legacy plugin-row conversion), `protocol/` (jsonrpc, model, version), `transport/` (the `McpTransport` trait, stdio, Streamable HTTP), `runtime/` (manager, cache), `registry/` (what a server advertises — `*/list`), `result/` (what one operation returned — `tools/call`, `*/get`, `read`), `security/` (header injection defense, `x-mcp-header` scanning), `legacy_stdio/` (the 2025-11-25 client) |
| `secret` | secret refs, the resolver, and the platform store |

### Composition and kernel

| Path | Holds |
|---|---|
| `app/` | `bootstrap`, `state` (`AppServer`), `router`, `lifecycle` — the wiring |
| `api/` | the HTTP transport layer; handlers parse, delegate, map |
| `shared/` | the **Protected Kernel**: event envelope, command envelope, resource identity, presence, voice contracts |
| `safety/` | the **Protected Kernel**: `SecurityExecutionGateway`, approval, grants, RBAC, audit, sandbox |
| `db/` | schema and migrations. Shared `0–999`, v1 `1000–1999`; no v2 namespace in this branch |
| `agent/`, `tools/` | the agent runtime and the tool registry/adapters |
| `config/`, `execution/`, `interaction/`, `isolation/`, `plugin/`, `utils/`, `workspace/` | supporting infrastructure the target map leaves in place |

### Retired in R2

The twelve crate-root compatibility shims. See
[compatibility-facades.md](compatibility-facades.md). `backend/src/` now holds
only `lib.rs` and `main.rs`.

## Frontend (`frontend/src/`)

| Path | Holds |
|---|---|
| `core/api/http.ts` | the HTTP core: `API_BASE`, `request`, `requestResult`, `ApiResult` / `ApiFailure`, and the one place the control-session header is attached |
| `api/` | one module per domain: `conversations`, `chat`, `system`, `plugins`, `memory`, `workflows`, `tasks`, `workspaces`, `capabilities`, plus `transport` (points at the core), `commands`, `events`, `resources`, `voice`, `projections`, `mcpRuntime`, `approvals`, `evidence`, `productManagement`, `providerConnection`, `controlSession` |
| `api/taskWorld/` | `types` (the contract), `transport` (`requestTaskWorld` + the error envelope), `calls` (22 wrappers), `index` (re-export surface) |
| `api/legacy.ts` | **compatibility surface only** — re-exports, no declarations. `api/client.ts` re-exports it |
| `features/` | `task-world` (canvas, inspector, hooks, model), `voice` (+ `runtime/`), `capabilities`, `execution`, `llm`, `mcp`, `memory`, `resources`, `security`, `skills`, `tasks` |
| `surfaces/` | `workspace` (the route table) and `desktop/` (the only place a static Tauri import is allowed) |
| `styles/` | `index.css` is a manifest with no rules; the rules live in ten files split by cascade layer |
| `architecture/boundaries.test.ts` | source-text checks: primitive/feature independence, feature/feature independence, Tauri import shape, the line budget, and "`legacy.ts` declares nothing" |

## What is not here

`modules/artifact`, `modules/approval` and `integrations/filesystem` are
`not started` in [current-to-target-map.md](current-to-target-map.md). Each is a
new module rather than a move, so it is product work.
