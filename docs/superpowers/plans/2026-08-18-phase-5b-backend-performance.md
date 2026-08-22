# Phase 5B Backend Performance Implementation Plan

> **For agentic workers:** Use inline execution with `superpowers:executing-plans`; subagents are explicitly disabled for this task. Steps use checkbox syntax for tracking.

**Goal:** Reduce measured Rust backend runtime overhead and async worker blocking while preserving existing REST, SSE, database, Agent, Memory, MCP, approval, and security contracts.

**Architecture:** Keep the existing Axum/Tokio backend and SQLite handle. Move synchronous system collectors into Tokio's blocking pool, await independent MCP startup refreshes together, and retain the bounded log API while changing its internal queue to `VecDeque`.

**Tech Stack:** Rust 2021, Axum 0.7, Tokio, rusqlite, sysinfo, futures, reqwest, existing Rust unit/integration tests.

## Global Constraints

- Do not modify Agent execution logic, Memory algorithms, MCP protocol, Security Gateway, Approval semantics, RBAC, sandbox checks, secret handling, or audit behavior.
- Do not modify REST routes, public JSON response fields, database schema, SSE event semantics, or frontend source.
- Preserve `token`, `tool_start`, `tool_end`, `approval_required`, `done`, and `error` compatibility.
- Preserve system sampling behavior, GPU lookup, response fields, log levels, log messages, and MCP transport timeouts.
- Do not add dependencies; `futures` and Tokio are already present.
- Do not add duration-based tests or claim external LLM latency improvements from local measurements.
- Use an isolated temporary data directory and temporary control token for runtime checks.

---

### Task 1: Move synchronous system collection off Tokio worker threads

**Files:** Modify `backend/src/api/system.rs`; extend its existing `health_tests` module.

**Interfaces:** Preserve `health`, `system_info`, `cpu_info`, and `memory_info`. Add private synchronous helpers `collect_system_info()`, `collect_cpu_info()`, and `collect_memory_info()` returning the same `serde_json::Value` payloads.

- [ ] **Step 1: Add failing collector contract tests.** Before adding helpers, test that `collect_system_info()` contains `cpu`, `memory`, `disks`, and `gpu`, and that focused collectors contain the existing `cores`, `avg_usage_pct`, `total_gb`, and `usage_pct` fields. Run `cargo test -p yilian-backend api::system::health_tests -- --nocapture`; it must fail to compile because the helpers do not exist.

- [ ] **Step 2: Extract the existing synchronous bodies.** Move the current bodies into the three private helpers without changing expressions, field names, formats, sampling interval, or the PowerShell GPU command.

- [ ] **Step 3: Add the blocking-pool boundary.** Keep each handler signature and replace only its body with `Json(tokio::task::spawn_blocking(collect_system_info).await.expect("system metrics collector task failed"))`, using the corresponding collector for CPU and Memory. The `expect` preserves the prior panic behavior rather than inventing fallback JSON.

- [ ] **Step 4: Verify the checkpoint.** Run the focused tests and `cargo check -p yilian-backend`. Keep the source change uncommitted for the final Phase 5B delivery commit.

---

### Task 2: Make bounded log eviction constant-time

**Files:** Modify `backend/src/server.rs`; add tests adjacent to `LogBuffer` in its existing test module.

**Interfaces:** Preserve `LogBuffer::new`, `push`, `drain`, `recent`, `LogEntry` serialization, and `/api/logs` behavior.

- [ ] **Step 1: Add failing observable behavior tests.** Test a capacity-two buffer retains `second` and `third` in order after `first`, `second`, `third`; test `drain` returns `first`, `second` and leaves `recent` empty; test capacity zero drops an entry without panicking. Run `cargo test -p yilian-backend log_buffer -- --nocapture`; the zero-capacity case must expose the current invalid eviction behavior.

- [ ] **Step 2: Replace the internal vector.** Import `VecDeque`; change `entries` to `Arc<Mutex<VecDeque<LogEntry>>>`; construct with `VecDeque::with_capacity(max_entries)`. In `push`, return immediately for zero capacity, use `pop_front()` when full, then `push_back()`. In `drain`, use `std::mem::take(&mut *entries).into_iter().collect()`. In `recent`, use `saturating_sub(count)` and clone the retained iterator in chronological order.

- [ ] **Step 3: Verify the checkpoint.** Run `cargo test -p yilian-backend log_buffer -- --nocapture`, `cargo test -p yilian-backend api::logs -- --nocapture`, and `cargo check -p yilian-backend`. Keep the source change uncommitted for the final Phase 5B delivery commit.

---

### Task 3: Refresh registered MCP servers concurrently at startup

**Files:** Modify `backend/src/server.rs`; use existing MCP tests and the full backend suite for regression coverage.

**Interfaces:** Preserve `AppServer::refresh_mcp_runtime`, `McpRuntimeManager`, transport timeouts, protocol negotiation, catalog parsing, failure logging, and unavailable-server degradation.

- [ ] **Step 1: Capture the current MCP baseline.** Run `cargo test -p yilian-backend mcp_runtime -- --nocapture`; all transport, cache, protocol, stdio, timeout, and security-adjacent MCP tests must pass before editing.

- [ ] **Step 2: Replace only the startup refresh loop.** Keep the registered-server snapshot and per-server warning behavior. Clone the existing manager, map each runtime to an async future calling `refresh_server(&id)`, and await the collection with `futures::future::join_all`. Do not change `refresh_server`, transport construction, timeout values, protocol fields, cache behavior, or tool execution paths.

- [ ] **Step 3: Verify the checkpoint.** Run the MCP suite, `cargo test --workspace --all-targets`, and `cargo check --workspace`. Keep the source change uncommitted for the final Phase 5B delivery commit.

---

### Task 4: Run Phase 5B regression and runtime verification

**Files:** No source changes expected. Update a directly affected test only if a real contract assertion requires it, and record why.

- [ ] **Step 1: Run backend verification.** Run `cargo test --workspace --all-targets`, `cargo check --workspace`, and `cargo build --workspace --release`. Record exact test counts, ignored count, pass/fail, and durations. Security execution tests must remain green.

- [ ] **Step 2: Run frozen frontend regression.** Run `npm.cmd test -- --run` and `npm.cmd run build`. Do not modify frontend source or perform unrelated bundle splitting.

- [ ] **Step 3: Check scope and whitespace.** Run `git diff --check develop...HEAD`, `git status --short`, and `git diff --stat develop...HEAD`. Confirm only the design/plan documents and targeted backend files changed; no frontend, API, schema, SSE, or security source changed.

- [ ] **Step 4: Run isolated runtime checks.** Start the backend with a new temporary `YILIAN_DATA_DIR`, the repository as `YILIAN_WORKSPACE`, and a temporary `YILIAN_CONTROL_SESSION_TOKEN` of at least 32 non-whitespace characters. With the control header, verify HTTP 200 healthy `/api/health`, existing fields from `/api/system`, `/api/system/cpu`, and `/api/system/memory`, existing shapes from `/api/memories`, `/api/memories/retrieve?q=baseline`, `/api/tasks?limit=20`, and `/api/tools`. Stop only the temporary backend. Do not claim external LLM, Agent completion, approval, or remote MCP success without actual observation.

- [ ] **Step 5: Repeat targeted measurements.** Using a reused local HTTP client and 5–15 samples per endpoint, repeat `/api/health`, `/api/system`, `/api/system/cpu`, `/api/system/memory`, `/api/memories?limit=100`, `/api/memories/retrieve?q=performance&top_k=8`, and `/api/tasks?limit=20`. Report medians and ranges, not isolated best cases. Do not add duration assertions to automated tests.

- [ ] **Step 6: Create the delivery commit and push.** Inspect `git diff -- backend/src/api/system.rs backend/src/server.rs` and run `git diff --check`. After all checks pass, run `git add backend/src/api/system.rs backend/src/server.rs; git commit -m "perf(backend): reduce runtime overhead and resource usage"; git push origin develop`. Record final HEAD, commit, push, working-tree state, tests, runtime observations, deferred candidates, and known limitations. Stop after Phase 5B; do not begin Phase 5C.
