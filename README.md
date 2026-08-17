# 忆涟千言 — 桌面级 AI 智能体

基于 **Rust + React + Tauri** 的本地优先桌面 AI 智能体。Tauri 桌面客户端是主要交互入口，浏览器版本用于开发预览。

## 架构

```
frontend (:1420)          backend (:9420)           src-tauri (primary)
┌──────────────┐   HTTP   ┌──────────────┐   IPC   ┌──────────────┐
│ React + Vite │ ◄─SSE──► │ axum + Rust  │ ◄─────► │ Tauri shell  │
│ React UI     │          │ agent/tools  │         │ 主要桌面入口 │
└──────────────┘          └──────┬───────┘         └──────────────┘
                                 │ SQLite
                                 ▼
                           ~/.yilianqianyan/
```

## 快速开始

### 1. 启动后端

```powershell
cd backend
$env:OPENAI_API_KEY = "sk-your-key"   # 设置 API 密钥
$env:YILIAN_CONTROL_SESSION_TOKEN = "local-dev-control-session-token-change-me"
cargo run
# → http://127.0.0.1:9420
```

### 2. 启动前端

```powershell
cd frontend
npm install
$env:VITE_CONTROL_SESSION_TOKEN = "local-dev-control-session-token-change-me"
npm run dev
# → http://localhost:1420
```

浏览器打开 `http://localhost:1420` 可进行开发预览。前后端开发令牌必须完全一致，且至少 32 个字符；令牌只用于本地开发，不应提交到仓库。

### 3. 启动 Tauri 桌面客户端（主要入口）

```powershell
npm run tauri dev
```

默认窗口为 `1200 × 800`，最小可调整至 `800 × 600`。
Tauri 会在每次启动时生成新的控制会话令牌，并通过内部 command 交给 webview；无需手工配置，也不会把令牌写入日志或数据库。

## 桌面工作台

- `>= 1180px`：任务列表、对话区、执行轨迹三栏常驻。
- `960–1179px`：任务列表常驻，执行轨迹从右侧抽屉打开。
- `800–959px`：任务列表与执行轨迹使用互斥的左右抽屉。
- 默认主题为“暖色本地（warm-local）”，可切换“精密中性”“石墨专业”“高对比”；自定义背景、字号和面板透明度会本地持久化。
- 工具执行结果与结果验证分别显示；工具返回成功不等于验证通过。
- 高风险与严重风险操作仍会暂停并等待用户明确批准，拒绝后不会执行原工具调用。

## 项目结构

```
├── backend/                     # Rust HTTP 服务端
│   └── src/
│       ├── main.rs              # axum 服务器入口
│       ├── server.rs            # AppServer 共享状态
│       ├── api/                 # REST + SSE 路由
│       │   ├── chat.rs          # POST /api/chat (SSE流式)
│       │   ├── conversations.rs # CRUD /api/conversations
│       │   ├── settings.rs      # GET/PUT /api/settings
│       │   └── tools.rs         # GET /api/tools
│       ├── agent/               # ReAct agent 引擎
│       ├── llm/                 # LLM 客户端 (OpenAI兼容)
│       ├── tools/               # 内置工具系统
│       ├── safety/               # 风险、RBAC、脱敏、审计与控制会话
│       ├── db/                  # SQLite 持久化（含安全审计表）
│       └── config/              # 配置管理
├── frontend/                    # React 前端
│   └── src/
│       ├── api/client.ts        # fetch + SSE 客户端
│       ├── components/          # UI 组件
│       │   ├── chat/            # 聊天界面
│       │   ├── sidebar/         # 对话列表
│       │   └── settings/        # 设置面板
│       └── types/               # TypeScript 类型
├── src-tauri/                   # Tauri 桌面客户端（主要入口）
├── skills/                      # AI 技能目录
└── .agents/                     # 子智能体目录
```

## API 端点

| 方法 | 路径 | 说明 |
|------|------|------|
| `POST` | `/api/chat` | SSE 流式聊天 |
| `POST` | `/api/chat/stop` | 取消生成 |
| `GET` `/POST` `/DELETE` | `/api/conversations[/{id}]` | 对话 CRUD |
| `GET` `/PUT` | `/api/settings` | 配置管理 |
| `GET` | `/api/tools` | 工具列表 |
| `GET` | `/api/security/audit` | 按时间、关联 ID、工具、事件、决策和风险筛选安全审计 |
| `POST` | `/api/security/audit/export` | 导出版本化、已脱敏的 JSON 审计数据 |
| `GET` | `/api/security/health` | 查询审计子系统与策略版本状态 |
| `GET` | `/api/health` | 健康检查 |

除 `/api/health` 外，所有 `/api/*` 请求都必须携带：

```text
X-Yilian-Control-Session: <本次进程的控制会话令牌>
```

控制会话用于隔离 Agent 执行面、用户控制面和无关的本地回环请求，不宣称能够抵御已取得相同操作系统用户权限的恶意软件。默认 CORS 仅允许 Tauri Origin 与 `localhost:1420` 开发 Origin；额外 Origin 必须通过 `YILIAN_ALLOWED_ORIGINS` 显式配置。

## 可用工具

| 工具 | 说明 |
|------|------|
| `bash` | Shell/PowerShell 执行 |
| `read_file` / `write_file` / `edit_file` | 文件操作 |
| `grep` / `glob` | 文件搜索 |
| `http_request` | HTTP 请求 |
| `load_skill` | 加载 AI 技能 |
| `write_todos` | 任务规划 |
| `process` | 进程管理 |
| `mouse` | 鼠标模拟（移动/点击/拖拽/滚轮） |
| `keyboard` | 键盘模拟（输入/按键/组合键） |
| `screenshot` | 屏幕截图 |
| `upscale_image` | AI 图片放大 |

## v0.2 Trusted Execution

状态：**Completed**

已完成：

- Unified `SecurityExecutionGateway`
- `PolicyEngine` Runtime Integration
- SecuritySubject → Role → PolicyEngine 进入 Runtime（Agent 与 Approval 不再硬编码 `Owner`）
- Sandbox Path Enforcement（含 symlink / junction 逃逸防护）
- Approval Execution Unification（审批后执行使用原始 subject，重新评估当前 role）
- Deterministic Verification Pipeline
- Redacted Security Audit Chain

## v0.6 Workspace / Task / Multi-Agent Runtime

状态：**Completed（v0.6.0 Workspace / Task / Multi-Agent Runtime）**

v0.6 将忆涟千言从「能运行一次工作流的桌面 Agent」升级为「能长期管理项目、任务、多智能体协作、产物与执行历史的本地 Agent Runtime」：

- Workspace Runtime（项目工作空间，Active/Archived 生命周期，不绕过 Sandbox）
- Task Runtime（Draft → Running → WaitingApproval/WaitingUser → Completed/Failed/Cancelled/Blocked）
- Task Execution（每次执行独立持久化，Retry 生成新 attempt，历史保留）
- Task Planner（LLM 生成严格 TaskPlan，schema 校验 + executor 引用校验；Planner 不执行工具）
- Artifacts（路径 containment 校验，防 `..`/symlink 逃逸；metadata-only，不授予文件权限）
- Task Timeline（用户视角事件流，与安全审计分离，metadata 有界）
- Agent Definition / Agent Team（Database 来源，兼容现有 AGENT.md Subagent）
- Sequential Multi-Agent Runtime（TaskOrchestrator 顺序驱动 plan steps）
- Bounded Delegation（深度/执行数/迭代数硬上限，无 A→B→A 递归）
- Shared Task Context（有界拼接，char-safe 截断）
- Task Agent Approval（consume-once + execute_approved 重新评估 + 有界 resume）
- Task Decision（业务决策与安全审批分离）
- Retry / Recovery（启动时 running → interrupted，task → blocked）
- Workflow Integration（Task 绑定 Workflow，terminal 同步 Task 状态，Output → Text Artifact）
- Desktop Workspace Surface（工作空间列表/详情、任务列表/时间线/执行/产物、Agents/Teams 页面）

**Known Limitations**：多智能体顺序执行（无并行）、无 Agent swarm、delegation 深度有界、无分布式 worker、无 Cron/定时任务、无 cloud Workspace、Workflow 调度仍顺序、Agent Team 不绕过 Security Gateway、MCP 仍 stdio、OS 级 Sandbox 未实现。

## v0.8 Development — Agent Memory + Unified Capability Registry

状态：**Phase 1–6 Completed（下一阶段：v0.8 Release Gate）**

- Phase 1 Agent Memory Retrieval：`MemoryContextBuilder` 将任务/步骤转为检索查询，走 hybrid（lexical + vector，lexical 回退），有界 char-safe 注入 Agent 上下文。
- Phase 2 Agent Memory Learning Loop：确定性 `DeterministicMemoryReflector`（无 LLM、无工具调用）+ 保守 `MemoryWritePolicy`（有界/置信度/secret 标记/近重复）+ `MemoryWriter`（validate → persist → best-effort embedding）。Completed → knowledge，Failed → note，Cancelled/Blocked/Waiting → 不学习。共享 `contains_sensitive_content` secret 检测单一真相源。
- Phase 3 Unified Capability Registry：`capability/` 模块统一 Builtin/MCP/Subagent/Agent/Workflow/Skill 的能力发现（`CapabilityDescriptor` + providers + 原子 refresh + 重复检测 + runtime readiness）。**Registry 只做发现，绝不执行能力**；真实授权仍在 Security Execution Gateway。`GET/POST /api/capabilities` API + Planner 引用校验（missing/disabled/unavailable → reject）+ 桌面「能力」页面。
- Phase 4 ✅ COMPLETED — MCP Runtime Expansion + Plugin Manifest Foundation：
  - Plugin Manifest（声明式 `plugin.json` 模型 + 校验 + registry + 路径 containment，绝不加载 native code / 自动连接 MCP）。
  - MCP Runtime：modern 2026-07-28 Streamable HTTP + legacy stdio；持久化 stdio 子进程；Tools/Resources/Prompts 目录；x-mcp-header 完整 wire；MRTR `resultType`；资源读缓存（`cacheScope` + TTL）；capability-gated 目录发现 + unknown-tool fail-closed。
  - **Production chain**：`DB MCP Config → McpRuntimeManager → Runtime Catalog → McpToolAdapter → ToolRegistry → SecurityExecutionGateway → Remote MCP`（approved/authorized remote call = 1，denied / pre-approval = 0）。
  - **Product surface**：Tools 元数据、Resources/read 只读预览（二进制不展开）、Prompts 预览（外部内容警告，不自动进对话/任务/System Prompt）、Plugin/MCP 生命周期（CRUD ↔ Runtime 同步 + shutdown_all + bounded startup refresh）。
  - 无 direct MCP Tool REST API，前端无 Tool 执行按钮；`call_tool` 保持 `pub(crate)`；Resource/Prompt 不自动注入 Agent 上下文。
- Phase 5 ✅ COMPLETED — SecretStore + Legacy Secret Migration：
  - OS-backed SecretStore（Windows Credential Manager / Keychain / Secret Service，`keyring` + `secrecy::SecretString` zeroize-on-drop）；测试用 `InMemorySecretStore`（绝不 plaintext fallback）。
  - `SecretRef`（version + key）持久化；`ModelConfig`/`McpServer` 只存 ref，`api_key`/`embedding_api_key`/stdio MCP env 值不再进 SQLite。
  - Legacy migration：Chat / Embedding / stdio MCP env → SecretStore（write → verify → clear plaintext）；失败保留明文并标 pending；幂等。
  - Runtime：`SecretResolver` 按需解析（SecretRef → env → legacy literal）；LLM/Embedding/MCP stdio 在 Authorization header / Command.env 边界才 `expose_secret`，不长缓存。
  - API/UX：Settings write-only 密钥 + 清除/轮换 + configured/source 状态；`GET /api/secrets/status`（无 value 导出）；`save_settings`/`create/update_mcp_server` 守卫拒绝明文。
  - **No plaintext new writes**：持久化 Chat/Embedding literal key = NO，stdio MCP env value = NO，API/log/audit/Debug 泄漏 = NO。
- Phase 6 ✅ COMPLETED — OS-assisted Isolation + Resource Grants + Permission Surface：
  - Typed `SecurityGrant`（Allow/Deny + `GrantResource`：Filesystem/Network/Process/Shell；`NetworkZone`；`ProcessGrantScope`）+ `security_grants` 表 + GrantStore + GrantEvaluator（Explicit Deny 优先、过期忽略、Missing → RequireApproval）。
  - SecurityExecutionGateway 接入 GrantEvaluator（决策顺序 RBAC → Grants → Sandbox → Risk）；Approval = 一次性资源授权，`execute_approved` 仍 re-evaluate 当前 live grants（Explicit Deny 不可被 approval 绕过）。
  - Filesystem Read/Write grant 强制 + canonical path + symlink/junction containment；Network target grant（scheme/host/port/method + zone）；http_request SSRF/DNS rebinding 硬化（redirect=none + private/loopback 拒绝）。
  - 进程隔离：Bash 走 ManagedProcessRunner（sanitized env，secret 剥离 + 真实 async timeout + 超时 kill 整个 process tree）；ManagedProcessRegistry 区分 managed child 与宿主 PID。
  - Grant CRUD API（protected）+ `/api/security/isolation/status`（诚实：process containment Active；OS filesystem/network namespace **未实现**，非容器级隔离）。
  - Windows hardening：受限令牌必须验证为严格降权；管道句柄继承失败即阻断；Job Object 真实回归覆盖孙进程树；优雅退出会清理 ManagedProcessRegistry。
  - ProcessTool 只能终止 Gateway 注册的 managed child，拒绝后端自身 PID 与未知宿主 PID；Bash/HTTP 直达 Tool Registry 的执行路径 fail closed。
  - HTTP 只允许经 Gateway 的固定 DNS 地址请求；解析结果必须全部属于目标 NetworkZone，重定向关闭，敏感请求/响应头与 URL query 不进入可见结果。
  - 每次资源授权评估写入 `grant_evaluated` 审计事件，审计上下文只保留类型化、脱敏证据；审计持久化失败时在副作用前 fail closed。
  - Settings「权限」页面提供 Filesystem/Network/Process/Shell 授权创建、删除、风险提示与真实隔离状态；不宣传 Full Sandbox。

**Key principle**：Discovery ≠ Authorization ≠ Execution。Capability Registry 不在 execution authorization 链中；Workflow/Registry 永不构成绕过 Trusted Execution 的第二条执行通道。SecretStore 只负责存取，绝不授予权限或绕过 Gateway；Grant 是 authority，不是 execution。

## v0.4 Workflow Runtime

状态：**Completed（v0.4.1 Workflow Runtime Completion）**

v0.4 将旧 Workflow 从 Prompt Template 升级为可执行 DAG Runtime：

- Typed DAG Definition + Validation（schema version、节点/边上限、唯一性、可达性、环检测）
- Workflow Run State（确定性状态机与 ready 节点计算）
- Persistence（graph 定义与 run 快照）
- Async Run Lifecycle（启动立即返回 run_id，后台调度）
- Run History（`GET /api/workflow-runs` 分页/过滤查询）
- Real Cancellation（CancellationToken 控制活动 Runner）
- Deterministic Sequential Scheduler
- Bounded Node Results（节点结果安全摘要，UTF-8 char-safe 截断）
- Tool / MCP / Subagent Nodes（一律经过 SecurityExecutionGateway，不绕过 RBAC/Approval/Sandbox/Verifier/Audit）
- Agent Node（LLM-only，不调用工具、不产生审批）
- Limited Condition Node（Always / PreviousSucceeded）
- Output Node
- Approval Pause / Resume（consume-once 防重放 + 重新评估 + HTTP Bridge）
- Desktop Runtime Surface（工作流中心：模板工作流 / 运行工作流两个 Tab，含图编辑器、Run Inspector、审批卡片、最近运行）
- Legacy Prompt Template Compatibility（旧模板保持可用）

**Known Limitations**：顺序调度（无并行）、无 Loop Node、无 Workflow 递归、无 Cron/后台调度、无分布式 worker、Condition 仅 Always/PreviousSucceeded、Agent Node 仅 LLM（无 ReAct/工具）、无嵌套 Agent 审批、MCP 仅 stdio、OS 级 Sandbox 未实现。

## v0.3.2 Product Surface Completion

状态：**Completed**

v0.3.2 将已完成的 Memory、MCP Runtime 与 Subagent Runtime 能力接入桌面端产品表面：

- Memory 页面提供记忆管理、词法/语义/混合检索模式、Embedding 状态与受控重建索引入口；内容变更会使旧 Embedding 失效。
- MCP 页面提供 stdio Runtime 能力状态、工具发现/调用反馈与脱敏配置展示；不支持的 HTTP/SSE Transport 不会被宣传为可用。
- Subagent 页面展示安全运行时元数据和允许工具白名单；私有 instructions、路径细节与敏感配置不会通过公共页面/API 回显。
- Settings 与 MCP 配置响应对 API Key、Embedding Key 和环境变量执行脱敏；未配置外部 Provider 或 MCP 服务时显示可理解的状态，而不是伪造成功。

本版本不新增 Workflow DAG、MCP HTTP Transport、嵌套 Subagent 或 OS 级 Sandbox。

## v0.3 Runtime Intelligence

状态：**Completed**

### Memory Runtime

已完成：

- 从对话中提取长期记忆，并按 `fact`、`preference`、`knowledge`、`note` 分类；
- 写入前执行格式校验、去重与敏感信息过滤；
- 提供词法检索，以及可配置的 Embedding Provider、向量持久化和混合检索；
- Embedding 不可用时自动回退到词法检索，并支持历史记忆重新索引；
- 将检索结果注入聊天上下文，同时不通过公共 API 暴露原始 Embedding。

Memory Runtime 是面向 Agent 上下文的本地长期记忆层，不替代 SQLite 数据库，也不等同于完整知识库或 RAG 平台。

### MCP Runtime

已完成：

- 通过 stdio 完成 `initialize` / `initialized`、带分页的 `tools/list` 与 `tools/call`；
- 将发现的 MCP Tool 转换为带命名空间的运行时 Tool，并提供 MCP 专用安全描述符；
- MCP 调用进入生产 Tool Registry 和统一审批恢复链；
- `Owner`、`Standard` 默认需要审批，`Restricted` 默认拒绝，审批后仍由当前策略重新评估。

当前 MCP Runtime 仅支持 stdio Transport；尚不支持 Streamable HTTP、SSE Transport、Resources、Prompts 或 Tasks。

### Subagent Runtime

已完成：

- 严格解析本地 `AGENT.md`，并保持子智能体 instructions 私有；
- 通过 `subagent_*` Tool Adapter 和 `agent.delegate` 安全描述符接入生产 Tool Registry；
- `Owner`、`Standard` 默认需要审批，`Restricted` 默认拒绝；
- `LocalSubagentExecutor` 为每次委派创建隔离的 Child `AgentState`、严格的 `allowed_tools` 白名单和独立安全 Gateway；
- 支持 Parent → Child 单层委派，并把 Child 输出封装为有长度上限的 `ToolResult`。

当前限制：仅支持单层委派；不支持嵌套审批、`workdir` 覆盖、热重载或递归委派。

### Workflow 状态

当前 Workflow 能力仍是 Workflow Template / Prompt Template，不是可执行 DAG 或完整 Workflow Engine。Workflow Engine 计划在 v0.4 继续演进。

## 安全机制

YiLianQianYan 对 Tool Call 进行统一安全评估。Agent 主循环和审批恢复路径中已接入运行时的 Tool Call 均通过 `SecurityExecutionGateway`，不会由 Agent 或 Approval API 直接执行 Tool。

| 风险等级 | 策略 |
|----------|------|
| `Low` | 通常自动执行 |
| `Medium` | 默认允许，但仍受角色、权限与 Sandbox 约束 |
| `High` | 需要用户批准 |
| `Critical` | 需要用户明确批准 |

### 统一安全执行链

```text
Tool Call
  ↓
ToolSecurityDescriptor
  ↓
ResourceScope Resolution
  ↓
Sandbox Policy
  ↓
SafetyPolicy Risk Evaluation
  ↓
PolicyEngine Decision
  ↓
Allow / RequireApproval / Deny
  ↓
Tool Execution
  ↓
Deterministic Verification
  ↓
Redacted Audit Recording
```

未知 Tool、无效 Descriptor、身份不匹配或资源无法解析时，执行链默认关闭。执行前的必要审计若无法持久化，也不会继续启动 Tool；Tool 已实际执行后的审计异常则保留执行结果，不会把已发生的执行伪装成未执行。

### SecurityExecutionGateway

`SecurityExecutionGateway` 是当前统一的 Tool 安全执行入口，负责连接：

- `ToolSecurityDescriptor` 与实际 `ResourceScope` 解析；
- Sandbox 写路径约束；
- `SafetyPolicy` 动态风险评估；
- `PolicyEngine` 权限决策；
- `ToolRegistry` 单次执行；
- `Verifier` 确定性结果验证；
- `AuditRecorder` 脱敏审计。

普通 Agent Tool Call 与审批通过后的原始 Tool Call 都通过 Gateway 执行。未来接入 Tool 执行能力的扩展模块也应复用该入口，不能绕过安全检查直接调用 `ToolRegistry.execute(...)`。

### PolicyEngine 与最小权限

`PolicyEngine` 负责统一处理：

- RBAC 角色；
- Capability；
- Permission；
- ResourceScope；
- Tool 最终风险；
- `Allow` / `RequireApproval` / `Deny`。

安全模块提供 `owner`、`standard`、`restricted` 三种内置角色。角色的解析链路为 `SecuritySubject → security_role_bindings → BuiltInRole → PolicyEngine`，Agent 和 Approval 运行时不再硬编码角色。`Owner` 不会绕过 High/Critical 风险的单次审批要求。role binding 不存在、无效或安全数据库不可用时，Gateway Fail Closed 并拒绝 Tool 执行（返回明确错误，不 fallback 到任何内置角色）。Gateway 不复制 RBAC 规则，只向 `PolicyEngine` 提交真实 Descriptor、资源范围、角色与最终风险。

### Sandbox 路径约束

当前 Sandbox 是**应用层路径约束**，用于判断文件写入目标是否满足配置策略。

支持的 profile：

- `read-only`：拒绝文件写入；
- `workspace-write`：仅允许工作区内写入；
- `custom`：仅允许 `writable_paths` 范围内写入；
- `open`：默认允许写入，但仍受显式拒绝路径限制。

支持：

- `writable_paths`
- `denied_write_paths`

`denied_write_paths` 的优先级始终高于 workspace 或 writable allow 规则。路径判断会规范化相对路径、绝对路径和 `..`，并按路径组件判断目录边界，避免把相似字符串前缀误判为同一目录。

路径判断会规范化相对路径、绝对路径和 `..`，并按路径组件判断目录边界，避免把相似字符串前缀误判为同一目录。写入目标路径会通过文件系统 `canonicalize` 解析 symlink / junction / reparse point 后再做边界检查，symlink 指向 workspace 外部时拒绝写入。

当前 Sandbox **不等价于**：

- Windows AppContainer；
- Windows 受限令牌或 Job Object；
- Linux namespace；
- 独立安全 Worker；
- 操作系统级强隔离。

Tool 仍在现有后端进程内执行。当前能力是应用层路径安全边界，不应宣传为 OS 级 Sandbox。

### 用户审批

高风险与关键风险 Tool Call 会暂停并进入以下流程：

```text
Tool Request
  ↓
Policy Decision
  ↓
RequireApproval
  ↓
用户批准 / 拒绝
  ↓
SecurityExecutionGateway Resume
  ↓
Execute Once
  ↓
Verify
  ↓
Audit
```

用户批准后执行的是审批时保存的**原始 Tool Call**，不会要求 LLM 重新生成另一个 Tool Call。审批采用原子消费，同一个 `approval_id` 最多执行一次；重复 approve 不会再次执行 Tool。

拒绝或取消不会执行 Tool。Agent 可以在拒绝结果写回上下文后继续重新规划。

审批 API：

```text
POST /api/approvals/:id/approve    允许 → Gateway 执行原始 Tool Call → 验证 → 继续 Agent（SSE）
POST /api/approvals/:id/reject     拒绝 → 写回拒绝结果 → 继续 Agent（SSE）
POST /api/approvals/:id/cancel     取消 → 标记取消，不执行
GET  /api/approvals/:id            查询单个审批
GET  /api/approvals/pending        列出待审批
```

注意：当前 `PendingApproval` 是内存中的运行时状态；后端重启后，未处理审批会失效。

### 持久化安全审计

当前 Tool 安全执行链会持久化以下关键事件：

```text
policy_decided
approval_requested
approval_resolved
execution_started
execution_finished
verification_finished
```

`Allow`、`RequireApproval` 和 `Deny` 都会记录策略决策；只有真正执行 Tool 时才记录 execution 事件，只有运行 Verifier 后才记录 `verification_finished`。

`AuditRecorder` 会在持久化前递归脱敏对象和数组：

- API Key、Token、Password、Cookie 等敏感值不会按原文保存；
- data URI 不保存正文；
- 长文本只保存受限预览、原始长度与 SHA-256 摘要；
- Gateway 不把完整原始 arguments 或 `ToolResult` 直接写入审计数据库。

SQLite 当前包含：

```text
security_subjects
security_role_bindings
security_approvals
security_audit_events
```

安全审计支持查询和版本化 JSON 导出，不提供删除 API。`previous_hash`、`event_hash` 和 `signature` 仍是未来升级预留字段，当前不宣称审计日志具备防篡改签名能力。

### 结果验证

YiLianQianYan 不会仅根据 `ToolResult.ok == true` 就认定任务完成。

Tool 执行后统一进入现有 `Verifier`。对于支持确定性验证的操作，Verifier 会重新观察真实状态，例如：

- 文件是否真实存在；
- 文件内容是否符合预期；
- 目标进程状态是否符合预期。

`ToolResult` 与 `VerificationResult` 分开保留。验证失败不等于 Tool 未执行；Agent 会收到验证失败和 `should_replan` 信息，并沿用现有 ReAct 流程重新规划。

当前 Verifier 主要覆盖文件与进程等确定性场景。Windows UI 与视觉结果验证仍属于后续扩展。

### 当前能力边界

当前能力仍有以下边界：

- MCP Runtime 仅支持 stdio，不支持 Streamable HTTP、SSE Transport、Resources、Prompts 或 Tasks；
- Workflow 仍是 Template / Prompt Template，不是可执行 DAG 或完整 Workflow Engine；
- Subagent Runtime 仅支持单层 Parent → Child 委派，不支持嵌套审批、`workdir` 覆盖、热重载或递归委派；
- OS 级 Sandbox 隔离；
- 跨后端重启持久化的 PendingApproval。

README 只把已经接入当前运行时并通过回归测试的能力标记为已完成；上述边界不作超出当前实现的承诺。
