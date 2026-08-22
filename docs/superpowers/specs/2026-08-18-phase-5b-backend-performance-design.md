# Phase 5B Backend Performance Design

## Goal

Reduce measured Rust backend runtime overhead and improve event-loop responsiveness without changing REST responses, SSE event semantics, database schema, Agent behavior, Memory retrieval rules, MCP protocol behavior, approval behavior, security enforcement, or frontend architecture.

## Baseline Evidence

- Branch: `develop`
- Baseline HEAD: `a123b7db2fb02e1b8eb41e6a4c72c1f4d74046cd`
- Backend workspace tests: 685 passed, 0 failed, 0 ignored, approximately 17.2 seconds.
- `cargo check --workspace`: passed.
- Isolated temporary-data runtime: `GET /api/health` returned HTTP 200 with `database: healthy` and policy version `security-rbac-v3`.
- With 250 temporary memories, local endpoint medians were approximately 2.25 ms for `GET /api/memories?limit=100`, 2.16 ms for lexical retrieval, and 0.53 ms for `GET /api/tasks?limit=20`.
- Monitor medians were approximately 1.39 s for `/api/system`, 142 ms for `/api/system/cpu`, and 40 ms for `/api/system/memory`. The full system endpoint includes synchronous system collection and a Windows PowerShell GPU query.
- `cargo build --workspace --release`: passed in approximately 5 minutes 50 seconds on the baseline workspace.

These measurements are local development observations, not claims about external LLM latency or production hardware.

## Selected Approach

### 1. Move system collection off Tokio worker threads

Keep the current system collection code, sampling behavior, sleep interval, PowerShell GPU lookup, output fields, and error behavior. Extract synchronous collection bodies into private functions and have the async handlers call them through `tokio::task::spawn_blocking`.

The affected handlers are:

- `GET /api/system`
- `GET /api/system/cpu`
- `GET /api/system/memory`

This changes scheduling only. It does not cache metrics, fake samples, reduce refresh frequency, omit GPU data, or change response JSON.

### 2. Refresh independent MCP servers concurrently during startup

Keep the existing registered-server snapshot, per-server refresh implementation, transport timeout, protocol negotiation, catalog parsing, failure logging, and unavailable-server degradation. Replace the startup loop that awaits each server serially with independent futures awaited together.

No MCP session pool, retry policy, protocol change, permission change, new concurrency setting, or direct tool execution path will be introduced.

### 3. Make bounded log eviction constant-time

Replace the internal `Vec<LogEntry>` storage with `VecDeque<LogEntry>`. When the fixed capacity is reached, remove the oldest item with `pop_front`; append new entries with `push_back`. Preserve:

- the 2000-entry production bound;
- chronological order from `recent`;
- chronological order from `drain`;
- clone behavior and the existing logs API;
- all log levels and messages.

Add behavior tests for bounded retention, order, `recent`, and `drain`. No logs are disabled, coalesced, redacted differently, or moved to an unbounded buffer.

## Explicitly Deferred

- Shared LLM `reqwest::Client` reuse. The current code creates clients through multiple chat, embedding, approval, workflow, and subagent paths. A safe implementation must preserve per-request timeout, proxy, TLS, headers, authentication, and live configuration semantics. The current local measurements do not isolate external LLM transport setup from provider latency.
- SQLite connection pools or broad `spawn_blocking` conversion of every database call. The current local Memory and Task paths are low-millisecond, and a broad change would touch lock and transaction boundaries.
- Memory retrieval algorithm, score weights, candidate limits, embedding JSON format, or global caches.
- Security Execution Gateway, sandbox, approval, RBAC, secret resolution, MCP permissions, or audit behavior.

## Testing and Verification

Before implementation, add the LogBuffer behavior tests and confirm they fail for the new expectations where applicable. Implement the smallest changes, then run:

```text
cargo test --workspace --all-targets
cargo check --workspace
cargo build --workspace --release
npm.cmd test -- --run
npm.cmd run build
git diff --check
```

Runtime verification uses an isolated temporary data directory and a temporary control-session token. It checks `/api/health`, Monitor endpoints, Memory list/retrieval, Task list, and a safe local backend interaction. No external LLM success, Agent end-to-end completion, or MCP remote success will be claimed unless actually observed.

## Non-Goals

This phase does not modify frontend source, REST routes, response schemas, SSE events, database schema, Agent prompts, Agent loop guards, tool behavior, approval semantics, security policy, MCP protocol, voice, Live2D, or Phase 5C work.
