# Changelog

## Unreleased — v0.8 Development

### Added

- Agent Memory Retrieval (`MemoryContextBuilder`): hybrid retrieval + bounded context injection
- Agent Memory Learning Loop: deterministic reflection + conservative write policy + embedding fallback
- Unified Capability Registry: Builtin/MCP/Subagent/Agent/Workflow/Skill discovery
- Runtime readiness status + atomic refresh + duplicate detection
- Planner capability reference validation (missing/disabled/unavailable → reject)
- `GET/POST /api/capabilities` API + desktop capability surface
- Plugin Manifest Foundation: declarative `plugin.json` model + validation + registry + path containment (no native execution, no auto-connect)
- MCP protocol primitives: 2026-07-28 modern metadata/header generation (base64 sentinel, CRLF rejection), transport config + URL validation

### Security

- Discovery separated from authorization/execution
- No provider may execute capabilities during discovery
- Registry metadata cannot grant permission; real authorization remains the Security Execution Gateway
- Shared secret-detection source of truth between Chat Memory Extraction and Agent Memory Learning
- Plugin discovery is side-effect-free; plugin paths are constrained to the plugin root; plugin permissions never grant RBAC
- MCP Streamable HTTP rejects remote plain HTTP, embedded credentials, and fragments

## 0.6.0 — Workspace / Task / Multi-Agent Runtime

### Added

- Workspace domain (project containers, Active/Archived lifecycle, additive persistence)
- Task runtime with TaskExecution lifecycle (start / retry / cancel / recovery)
- Task planner producing a strict, validated TaskPlan (LLM, mock-testable)
- Artifacts with path-containment validation (no `..`/symlink escape; metadata-only)
- Task timeline events (separate from the security audit chain)
- Agent definitions and agent teams (bounded delegation policy)
- Sequential multi-agent orchestrator (workflow / agent / subagent plan steps)
- Shared bounded task context for agents
- Task-agent approval classification with TOCTOU-safe bounded resume
- Task decisions (business decisions separate from security approvals)
- Recovery-on-startup (running executions → interrupted; tasks → blocked)
- Workflow integration (task binds a workflow; terminal state syncs; output → artifact)
- Desktop Workspace surface (workspace list/detail, task timeline/executions/artifacts, agents/teams)

### Security

- Trusted Execution preserved: tool / MCP / subagent still flow through the Security Execution Gateway
- Task agent subject inherited from the task execution context (never hard-coded local-user)
- Agent `allowed_tools` is a ceiling, not a grant — RBAC still applies
- Approval resume re-evaluates the current role; replay is consume-once
- Task decisions never authorize tool execution
- Planner never executes tools

## 0.4.1 — Workflow Runtime Completion

### Added

- Asynchronous workflow runs (start returns immediately; runner runs in background)
- Run history listing with graph/status filters and bounded limits
- Real cancellation via a CancellationToken-controlled active-run registry
- Bounded, UI-safe node results (char-safe truncation; binary payloads replaced with labels)
- Desktop workflow runtime UI: templates + runtime tabs, graph editor, run inspector, approval card, recent runs
- LLM-only Agent Node (single text generation, no tool calls, no approval)
- Output / Condition node results

### Fixed

- Workflow Approval HTTP bridge (approve/reject/cancel dispatch to the workflow runtime)
- Truthful approval SSE (resolution events only after the decision truly happens)
- Approval replay semantics (second approve fails fast with a conflict)

### Security

- Tool / MCP / Subagent nodes continue through the Security Execution Gateway
- Agent Node has no tool-use and never touches the Tool Registry
- Approval resume re-evaluates the current role binding
- Security subject is always resolved server-side

### Known Limitations

- Scheduler is sequential; DAG branches execute deterministically in sequence (no parallel)
- No Loop Node, no Workflow recursion, no Cron/background scheduling, no distributed worker
- Condition only supports Always / PreviousSucceeded (no arbitrary DSL)
- Agent Node is LLM-only — no ReAct loop, no nested Agent approval
- MCP stdio only; OS-level sandbox not implemented

## 0.4.0 — Workflow Runtime

- DAG definition and validation (typed graph model, cycle / reachability / endpoint checks)
- Workflow run state machine (deterministic transitions, ready-node calculation)
- Run persistence (graph definitions + run snapshots)
- Deterministic sequential scheduler
- Secure Tool / MCP / Subagent execution through the Security Execution Gateway
- Approval pause / resume with consume-once replay protection and re-evaluation
- Workflow runtime API (backend graph CRUD + run lifecycle)

### Known Limitations

- Sequential scheduler only (no parallel node execution)
- No workflow recursion or loop nodes
- No arbitrary expression language for conditions (Always / PreviousSucceeded only)
- No cron / background scheduling, no distributed workers
- MCP stdio only; Subagent delegation single-layer only
- OS-level sandbox still not implemented
- Agent node execution not wired (fails closed)
- Desktop runtime surface (frontend UI) not yet implemented

## Unreleased

## 0.3.2 — Product Surface Completion

- Completed the Memory embedding consistency surface: content edits invalidate stale embeddings and expose controlled reindexing.
- Added the Memory Reindex Surface with provider-aware status and bounded reindex controls.
- Added the Hybrid Retrieval Surface with lexical, semantic, hybrid, and lexical-fallback modes.
- Added the MCP Runtime Surface with stdio capability status, tool discovery, invocation feedback, and redacted configuration display.
- Added the Subagent Runtime Surface with safe discovery metadata, allowlisted tools, and private instructions.
- Added Secret Redaction / Security Foundation coverage for settings and MCP configuration responses without plaintext secret echo.

### Known Limitations

- Workflow remains a Template / Prompt Template and is not an executable DAG Runtime.
- MCP Runtime supports stdio only; Streamable HTTP and SSE Transport are not supported or advertised.
- Subagent delegation is single-layer only; nested approval, `workdir` override, AGENT.md hot reload, and recursive delegation are not supported.
- Sandbox enforcement is application-level path policy, not OS-level isolation.

## 0.3.1 — Runtime Stabilization

- Synchronized the documented v0.3 Runtime Intelligence status with the implemented Memory, MCP stdio, and Subagent runtimes.
- Expanded the public backend health response with service, release version, SQLite readiness, and policy version details.
- Added a lightweight desktop backend-health indicator with low-frequency polling and non-blocking failure handling.
- Added structured, redacted desktop startup diagnostics and health probing, including verified port-conflict classification.
- Stabilized Windows Rust development builds for low-memory machines through serialized compilation and reduced dev-profile memory pressure.
- Added local, no-network v0.3 Runtime Smoke Coverage for AppServer, migrations, tools, Memory routes, MCP configuration, and Subagent registration.

### Known Limitations

- Subagent delegation is single-layer only; nested approval, `workdir` override, AGENT.md hot reload, and recursive delegation are not supported.
- MCP Runtime supports stdio only; Streamable HTTP, SSE Transport, Resources, Prompts, and Tasks are not supported.
- Workflow remains a Template / Prompt Template and is not an executable DAG Runtime.
- Sandbox enforcement is application-level path policy, not OS-level isolation.

## 0.3 — Runtime Intelligence

- Added local Memory Runtime with validated extraction, sensitive-data filtering, lexical and hybrid retrieval, embedding fallback, reindexing, and chat-context injection.
- Added stdio MCP Runtime with tool discovery, namespaced adapters, production registry integration, and approval-aware execution.
- Added single-layer Subagent Runtime with strict `AGENT.md` discovery, private instructions, allowlisted child tools, isolated execution state, and bounded results.
- Kept Workflow at the Template / Prompt Template stage; executable DAG workflows remain outside v0.3.

## 0.2 — Trusted Execution

- Added the unified `SecurityExecutionGateway`, runtime `PolicyEngine`, RBAC subject resolution, sandbox path enforcement, approval execution unification, deterministic verification, and redacted security auditing.
