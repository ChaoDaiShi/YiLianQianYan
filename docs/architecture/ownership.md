# Module Ownership

> One owner per domain. Written so a new developer or an AI agent does not have
> to re-derive the architecture from the code before making a safe change.

## How to use this

Before editing, find your domain. If your change crosses into another domain's
`Owns` column, or touches anything under `shared/` or `safety/`, it is a
**SHARED_CHANGE**: say so explicitly and expect review.

An agent working the Task domain may edit `modules/task` and
`frontend/src/features/task-world`. It may not edit the voice runtime or the
settings secret lifecycle.

---

## `modules/settings`

- **Purpose** — model and voice provider configuration, the secret lifecycle,
  and provider readiness.
- **Owns** — `AppConfig` semantics; the KEEP / REPLACE / DELETE secret state
  machine; the readiness projection.
- **May depend on** — `db::settings`, `secret`, `shared::contracts`.
- **Must not depend on** — any other module's internals; a transport.
- **Public entrypoints** — `modules::settings::{api, application, domain}`.
- **Persistence** — `db::settings` rows plus the `AppConfig` snapshot on
  `AppServer`. The module owns no tables of its own.
- **Invariants** — a missing key KEEPs, an empty key KEEPs, a non-empty new key
  REPLACEs, an explicit clear DELETEs. Four tests pin this
  (`empty_key_preserves_secret_across_settings_changes_and_restart`,
  `clear_delete_failure_preserves_each_persisted_secret_reference`,
  `settings_get_never_returns_secret_value`,
  `voice_settings_redact_stt_and_tts_secrets_independently`). A refactor may not
  reintroduce a key-overwrite bug.

## `modules/task`

- **Purpose** — Task World: the product layer above a workflow run.
- **Owns** — `TaskGraph` semantics, `TaskSupervisor`, executions and their
  history, validation, planning, checkpoints, the canvas projection.
- **May depend on** — `workflow`, `shared`, `safety`.
- **Must not depend on** — a surface's visual state. `CanvasView` is a visual
  document and may not modify `TaskGraph` semantic state; the frontend may not
  rebuild `TaskSupervisor` from events.
- **Public entrypoints** — `modules::task::mod.rs` re-exports
  (`TaskWorldRuntime`, `TaskSupervisor`, `projection`, `canvas_view`, …).
- **Persistence** — `db::task`, `db::task_world`, artifact tables.
- **Events emitted** — `task.node.running`, `task.canvas.updated` (observed by
  the frontend through `isTaskWorldEvent`). Running is emitted as an
  invalidation: consumers re-read the projection rather than applying state.

## `modules/workflow`

- **Purpose** — workflow graphs and their runs.
- **Owns** — `WorkflowRun`, graph revisioning, approval resume.
- **May depend on** — `shared`, `safety`.
- **Must not depend on** — Task World semantics.
- **Persistence** — `db::workflow_runtime`.
- **Security chain** — a workflow run reaches execution only through
  `Workflow → SecurityExecutionGateway → Approval → Execution → Verification`.
  This chain is a Protected Kernel and must survive any refactor intact.

## `modules/resource`

- **Purpose** — bounded extraction of user-supplied files, independent of the
  shared Resource DTO.
- **Owns** — the upload policy, the parsers, the preview projection, and node
  resource binding.
- **May depend on** — `shared::resource`, `db::resource_bindings`.
- **Must not depend on** — a Task's state. Parsers take bounded bytes and return
  normalized content; they never read or write a Task.
- **Public entrypoints** — `modules::resource::{ingest_resource,
  resource_preview, bound_node_resources, validate_upload, extract_preview}`.
- **Layout** — `limits` (policy) → `parsers` (bytes in, content out) →
  `preview` (projection); `ingest` and `binding` are the two use cases.
- **Known edge** — `binding.rs` reads
  `modules::task::execution::MAX_NODE_CONTEXT_ITEM_CHARS`. Recorded, not yet
  removed.

## `modules/memory_skill`

- **Purpose** — user-reviewed evidence candidates; never an automatic long-term
  memory write.
- **Owns** — the candidate lifecycle (`candidate → review → version`), evidence
  and sensitivity screening, the managed skill file store.
- **May depend on** — `db`, `safety`, `agent::memory` (for the write policy),
  `tools::skill` (discovery).
- **Must not depend on** — an automatic trigger. Creation requires an explicit
  authorization flag; install requires an explicit confirmation flag.
- **Public entrypoints** — `modules::memory_skill::MemorySkillService`.
- **Persistence** — `skill_candidates`, `skill_candidate_revisions`,
  `managed_skill_versions`, plus the managed `SKILL.md` files under the store
  root.
- **Invariants** — the database is not treated as the truth for the file: a
  rule edited outside the service is refused, not overwritten. These are the
  stable concepts a future capability-evolution feature would build on; no such
  engine exists here, and none is anticipated in code.

## Protected Kernel (no owner may change semantics unilaterally)

`shared/` — event envelope, command envelope, resource identity, presence,
voice contracts.
`safety/` — `SecurityExecutionGateway`, approval, capability security semantics.

## Infrastructure

- `integrations/mcp` — MCP protocol, transport, runtime, registry, result
  handling. Subdivided in R2 / S3 into `protocol/ transport/ runtime/ registry/
  result/ security/` plus `config.rs`, which is where `mcp_transport_config`
  now lives (it was in `app/state.rs`).
- `db/` — schema and migrations. Migration namespaces: Shared `0–999`,
  v1 `1000–1999`. There is **no v2 namespace in this branch**; v2 owns its own.
  Architecture work adds no migration.
- `integrations/llm`, `secret`, `isolation` — provider, secret and sandbox
  adapters.

## Retired compatibility facades

R1 left twelve glob shims at `backend/src/` so moved call sites kept compiling.
They are gone as of R2 / S4. A reference to `crate::server::*`, `crate::task::*`,
`crate::secret::*` or any of the other nine is stale — the owning module is
listed in [compatibility-facades.md](compatibility-facades.md).

## Not yet under `modules/`

`safety/`, `db/`, `agent/` and `tools/` still sit at `backend/src/`. They are
unrelocated, not unowned. `voice/`, `capability/` and the `integrations/*`
adapters were relocated by R9; moving the remaining four is the largest
structural work still open and is deliberately not attempted by R2.
