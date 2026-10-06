# YiLianQianYan v1.1 User Cloud and Experience Implementation Plan

> 本计划在单一实施分支连续执行。仓库指令禁止未授权子智能体，因此所有工作由当前智能体完成。每个阶段形成小提交，但不在阶段之间等待人工确认。

**Goal:** 在保持完整 local-first 能力的前提下，交付可选身份、增量同步、账号与隐私体验、首次配置、AI Search、隔离的私有云服务与 Admin、导出删除生命周期及 v1.1 RC 证据。

**Public baseline:** `v1.1/base @ 012dc010ebb730148a3583200f8275dcf9a0fa03`

**Public implementation:** `feat/v1-1-user-cloud-experience`

**Private repository:** `F:\项目开发\忆涟千言\YiLianCloud-Private`, local Git only, no remote.

## P0 Foundation Freeze

- [ ] Freeze ADR-001 through ADR-007.
- [ ] Add public Cloud Contract, sync contract, data classification and stable error taxonomy.
- [ ] Add public/private boundary scanner and CI gate.
- [ ] Initialize the physically separate private repository with no remote.
- [ ] Commit the public and private P0 baselines before Identity implementation.

## P1 Optional Identity

- [ ] Add persistent LocalIdentity, Guest, Authenticated, Offline Authenticated and Closing states.
- [ ] Store cloud session credentials only through SecretStore references.
- [ ] Add register, login, refresh, logout, device and session client flows.
- [ ] Preserve complete local use when the user skips login or the cloud is unavailable.

## P2 Private Cloud Data

- [ ] Implement private cloud API identity, device, session and user metadata services.
- [ ] Add PostgreSQL migrations for users, devices, sessions, sync changes, user data metadata, jobs and audits.
- [ ] Add object metadata boundary without storing binary blobs in PostgreSQL.
- [ ] Keep all server implementation inside the private repository.

## P3 Incremental Sync

- [ ] Add local outbox, cursor, entity revision, tombstone and conflict persistence.
- [ ] Implement idempotent push and cursor pull.
- [ ] Define per-entity conflict policies and prevent deleted data resurrection.
- [ ] Expose retryable failures and manual conflict resolution in the local API.

## P4 Account Sync and Privacy UI

- [ ] Build Account Center, Sync Center and Data and Privacy surfaces.
- [ ] Separate logout from permanent account deletion.
- [ ] Make sync state, backlog, conflicts, last success and errors visible.
- [ ] Keep navigation compact and avoid duplicate actions.

## P5 Setup Wizard

- [ ] Build a resumable first-run wizard for identity, sync, LLM, Search and optional Voice.
- [ ] Reuse existing Provider Readiness and Settings save paths.
- [ ] Keep advanced provider fields in Settings.

## P6 AI Search

- [ ] Implement a provider-neutral SearchProvider contract.
- [ ] Add AnySearch `POST /v1/search` adapter with anonymous and SecretStore-backed BYOK modes.
- [ ] Build AI Search results with source opening and actions into Chat, Task and Knowledge.
- [ ] Preserve request IDs and classify auth, quota, rate-limit and provider failures without logging credentials.

## P7 Private Admin Console

- [ ] Implement private Admin API with role checks, metadata-only responses and audit records.
- [ ] Build Admin Web for users, sync, storage, lifecycle, audit and health.
- [ ] Keep `/internal/admin/*` outside the public contract and public source tree.

## P8 Export and Delete Lifecycle

- [ ] Implement export requests and downloadable result metadata.
- [ ] Implement session revocation, DeletionJob, storage cleanup and terminal tombstone behavior.
- [ ] Make deletion status visible and prevent deleted sessions from writing data.

## P9 RC Hardening

- [ ] Run Rust formatting, checks and focused tests for changed public and private crates.
- [ ] Run frontend tests and production builds for desktop and Admin Web.
- [ ] Run boundary, identity, sync conflict, privacy, search and Admin RBAC E2E gates.
- [ ] Record Windows/Tauri and real external-provider checks separately as PASS, PARTIAL or HUMAN_PENDING.
- [ ] Produce hashes, final report, defect list, rollback notes and release candidate metadata.

## Stop Condition

Stop after the v1.1 public client, private services, documentation and validation evidence are complete. Do not begin v1.2, Cloud Harness, remote Computer Use, team collaboration, subscription billing, full Web Surface or v2 DesktopSpace work.
