# Changelog

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
