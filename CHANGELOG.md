# Changelog

## Unreleased

## 0.9.0 — 2026-08-22

### Added

- “昔涟 · 涟漪 / Cyrene Ripple”完整桌面主题，提供白天与夜间两种外观，并以透明月光 Surface 保留全局环境背景。
- 智能工作台首页、对话反馈、Agent Progress、Tool 详情、审批、错误提示与可折叠执行轨迹的统一交互体验。
- Task Center、Workspace、Workflow Editor、Memory、Knowledge Base、Skill、Plugin、Agent、Capability、Monitor、Logs 与 Settings 的 v0.9 产品界面。
- 后台对话任务生命周期与任务筛选，历史记录可持久化并恢复真实执行状态。
- Skill 安全 CRUD、Capability Source 管理与 MCP 服务配置管理。
- 多服务商 OpenAI 兼容模型档案：支持 OpenAI、DeepSeek、千问、GLM、自定义地址，包含连接验证、激活切换和系统凭据库保护。
- 本地持久化 LLM Token usage：按模型/日期聚合输入、输出、总 Token，并在设置页提供模型树与柱状图。

### Changed

- 前端功能路由采用懒加载，减少初始 bundle 与不必要的聊天消息渲染；后端降低空闲轮询和运行时资源开销。
- 执行历史优先展示用户可理解的动作和错误说明，原始 Tool 字段、stdout、stderr 与技术错误进入折叠详情或日志。
- 网站与桌面应用启动只有在目标窗口真实可见并获得前台焦点后才报告成功。
- 桌面文字输入绑定明确目标应用，并在目标窗口保持前台且输入完成得到验证后才允许 Agent 宣告任务完成。

### Fixed

- 修复后台命令窗口短暂闪烁、路由切换闪烁、长执行详情越界、历史审批状态丢失和聊天执行历史恢复问题。
- 修复打开网站或应用仅检查进程却误报“已打开”的问题。
- 修复只打开记事本但未完成文字输入时仍回答成功的问题。

### Security

- GUI 启动、键盘输入与 Agent 完成状态继续通过真实可观察证据验证；无法确认时 Fail Closed，不伪造成功。
- Skill、Capability 与 MCP 管理操作保持现有 Security Gateway、审批、SecretStore 与审计边界。

### Packaging

- Windows x64 正式产物统一为 NSIS 安装包；默认 Tauri 构建不再混合 macOS bundle 目标。
- 版本一致性、正式构建、SHA-256、隔离静默安装与卸载均提供可重复执行的发布脚本。
- macOS `.app` / `.dmg` 保留独立构建入口；签名与公证不在本版本自动化范围内。

## 0.8.0 — 2026-08-17

### Added

- Agent Memory Retrieval (`MemoryContextBuilder`): hybrid retrieval + bounded context injection
- Agent Memory Learning Loop: deterministic reflection + conservative write policy + embedding fallback
- Unified Capability Registry: Builtin/MCP/Subagent/Agent/Workflow/Skill discovery
- Runtime readiness status + atomic refresh + duplicate detection
- Planner capability reference validation (missing/disabled/unavailable → reject)
- `GET/POST /api/capabilities` API + desktop capability surface
- Plugin Manifest Foundation: declarative `plugin.json` model + validation + registry + path containment (no native execution, no auto-connect)
- MCP protocol primitives: 2026-07-28 modern metadata/header generation (base64 sentinel, CRLF rejection), transport config + URL validation
- MCP Runtime Manager: persistent stdio + Streamable HTTP transports, protocol negotiation, capability-gated Tools/Resources/Prompts discovery, resource-read cache (cacheScope + TTL), x-mcp-header wire, shutdown_all lifecycle
- Managed MCP trusted execution: McpToolAdapter → McpRuntimeManager, Tools capability gate + unknown-tool fail-closed
- SecurityExecutionGateway → Managed MCP real E2E (approved = 1 remote call, denied / pre-approval = 0)
- Protected runtime APIs: `GET /api/mcp/servers` + tools/resources/prompts/read (no direct tool execution)
- Frontend MCP surface: per-server runtime status / protocol / counts, Tools metadata, Resources/read preview, Prompts preview with external-content warning (no auto-submit, no tool execute button)
- OS-backed SecretStore (`keyring`): Windows Credential Manager / Keychain / Secret Service; `secrecy::SecretString` zeroize-on-drop; `InMemorySecretStore` for tests
- `SecretRef` persistence for Chat / Embedding API keys + stdio MCP env; `SecretResolver` on-demand resolution (SecretRef → env → legacy literal)
- Legacy secret migration (write → verify → clear plaintext; idempotent; failure preserves plaintext)
- Settings write-only Secret UX (configured/source status, clear/rotate) + `GET /api/secrets/status` (value-free)
- MCP stdio env → SecretStore + delete cleanup; Streamable HTTP header env-var references unchanged
- Typed `SecurityGrant` resource grants (Filesystem/Network/Process/Shell) + `security_grants` table + GrantEvaluator (explicit Deny wins, missing → approval)
- SecurityExecutionGateway grant enforcement (RBAC → Grants → Sandbox → Risk); approval = one-shot, revalidates live grants
- Filesystem read/write grant + canonical-path/symlink containment; Network target grants + SSRF/DNS-rebinding hardening (redirect=none + private/loopback reject)
- Managed process runner for Bash: sanitized env (no secret inheritance) + real async timeout + whole-tree kill
- Process grants and UI are limited to `ManagedChildren`; legacy explicit-PID/all-host rows remain readable but fail closed
- Network execution consumes gateway-created zone evidence after one DNS resolution; ordinary host approvals default to Public

### Security

- Discovery separated from authorization/execution
- No provider may execute capabilities during discovery
- Registry metadata cannot grant permission; real authorization remains the Security Execution Gateway
- Shared secret-detection source of truth between Chat Memory Extraction and Agent Memory Learning
- Plugin discovery is side-effect-free; plugin paths are constrained to the plugin root; plugin permissions never grant RBAC
- MCP Streamable HTTP rejects remote plain HTTP, embedded credentials, and fragments
- No direct MCP tool execution REST API; `call_tool` remains `pub(crate)`
- Frontend can never execute an MCP tool; prompt preview never auto-triggers chat/task/agent/system-prompt/memory
- Gateway E2E: unauthorized remote call = 0, approval-before-execution remote call = 0, approved = 1
- Production discovery/execution never use legacy `probe_stdio_server` / `call_stdio_tool`
- New secret writes never fall back to plaintext: `save_settings` / `create` / `update_mcp_server` reject plaintext API keys / stdio MCP env
- Secret value never appears in API / logs / audit / Debug; only exposed at the Authorization header / Command.env boundary
- SecretStore only stores/resolves secrets — it never grants permission or bypasses the Security Execution Gateway

### Known Boundaries

- OS-backed filesystem and network isolation remain capability-level boundaries; they are not presented as a complete host sandbox.
- v0.8.0 packaged Windows acceptance completed; the release checklist records the user-confirmed launch, health, lifecycle, and persistence evidence.

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
