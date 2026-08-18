# LLM Model Management Implementation Plan

> **For agentic workers:** Inline execution in this session; no subagents. Steps use checkbox syntax for tracking.

**Goal:** Add persistent multi-provider OpenAI-compatible model profiles, connection verification, active-model routing, and token usage analytics to settings.

**Architecture:** Add a focused `llm_models`/`llm_usage_events` persistence layer and `/api/llm/*` handlers. Reuse `ModelConfig`, `LlmClient`, `SecretResolver`, the control-session middleware, and the existing settings page; the active profile is converted into the existing chat client configuration, while the old configuration remains a fallback.

**Tech Stack:** Rust 2021, Axum 0.7, rusqlite, reqwest 0.12, Tokio, React 18, TypeScript, Vite, Vitest, Tailwind utility classes, lucide-react.

## Global Constraints

- API keys must never be persisted in SQLite or returned by API responses; only SecretRef/configured metadata is allowed.
- Keep existing `AppConfig.model`, `/api/settings`, Agent loop guards, SSE event compatibility, React/Tauri/Rust architecture, and SQLite persistence.
- Model profile count is capped at 32; usage aggregation is bounded to 366 days and 5000 rows.
- Provider usage is recorded only when the API supplies token usage; no fabricated exact token counts.
- No subagents, no unrelated refactors, no new runtime dependency.

### Task 1: Model and usage persistence

**Files:**
- Create: `backend/src/db/llm_models.rs`
- Modify: `backend/src/db/mod.rs`
- Modify: `backend/src/db/settings.rs` only if active-model fallback metadata is needed
- Test: Rust unit tests in `backend/src/db/llm_models.rs`

- [ ] Write failing CRUD, activation, key-ref redaction, and daily aggregation tests.
- [ ] Add additive SQLite tables and indexes in the existing migration batch.
- [ ] Implement `LlmModelRow`, `LlmUsageRow`, `LlmModelInput`, CRUD, `activate_llm_model`, `get_active_llm_model`, `record_llm_usage`, and bounded aggregation methods.
- [ ] Run focused `cargo test db::llm_models` and then the full backend test suite.

### Task 2: LLM profile API and connection verification

**Files:**
- Create: `backend/src/api/llm_models.rs`
- Modify: `backend/src/api/mod.rs`
- Modify: `backend/src/secret/model.rs` and `backend/src/secret/mod.rs` for hashed per-profile SecretRefs and audit kind
- Modify: `backend/src/config/types.rs` only for serializable compatibility fields if required
- Test: API tests in `backend/src/api/llm_models.rs`

- [ ] Write failing tests for redacted list responses, create/update/delete, active switching, missing-key failure, and mock OpenAI verification.
- [ ] Implement provider presets as API-safe metadata, validated URL/model input, dynamic SecretRef generation, and value-free error responses.
- [ ] Implement verify using `LlmClient::invoke`, persist `verified_at`, and record returned usage.
- [ ] Mount protected `/api/llm/models` CRUD/verify/activate and `/api/llm/usage` routes.
- [ ] Run focused API tests and `cargo fmt --check`.

### Task 3: Active model routing and streaming usage persistence

**Files:**
- Modify: `backend/src/llm/types.rs`
- Modify: `backend/src/llm/client.rs`
- Modify: `backend/src/agent/engine.rs`
- Modify: `backend/src/api/chat.rs`
- Test: existing LLM/client/chat tests plus new usage assertions

- [ ] Write failing tests proving stream usage is parsed and active profile values reach the mock chat endpoint.
- [ ] Add optional `stream_options` and usage to SSE chunk parsing; expose a synchronous usage sink on `LlmClient`.
- [ ] Build a persistent usage sink from `Database` and active profile id, attach it to chat and verification calls.
- [ ] Resolve the active profile at chat start, fall back to legacy `AppConfig.model` when none exists, and preserve embeddings from the legacy config.
- [ ] Run mock chat integration tests and the complete backend suite.

### Task 4: Frontend API/types and model management UI

**Files:**
- Modify: `frontend/src/api/client.ts`
- Modify: `frontend/src/types/index.ts`
- Create: `frontend/src/features/llm/ModelTree.tsx`
- Create: `frontend/src/features/llm/TokenUsageChart.tsx`
- Create: `frontend/src/features/llm/llmModelUtils.ts`
- Modify: `frontend/src/pages/SettingsPage.tsx`
- Test: `frontend/src/features/llm/llmModelUtils.test.ts`, `frontend/src/pages/systemSettingsContract.test.ts`

- [ ] Write failing tests for preset defaults, tree layout cap, date query construction, and model section semantics.
- [ ] Add typed API functions for list/create/update/delete/verify/activate/usage.
- [ ] Implement bounded SVG tree layout with up to 32 branches and truncated labels.
- [ ] Implement responsive daily token bars with model/date filters and empty/loading/error states.
- [ ] Replace the single-model settings block with profile cards and an add/edit form while keeping legacy embedding and config controls available.
- [ ] Run focused Vitest tests and `npm.cmd run build`.

### Task 5: Documentation and final verification

**Files:**
- Modify: `README.md`
- Modify: `CHANGELOG.md`

- [ ] Document profile setup, supported presets, secret behavior, activation, and usage limitations.
- [ ] Run `cargo fmt`, `cargo fmt --check`, `cargo check`, `cargo test`, `npm.cmd test`, and `npm.cmd run build` from the worktree.
- [ ] Inspect `git diff`, ensure no SQLite/database or secret artifacts are tracked, and report the branch, files, validation evidence, and known limitations.
