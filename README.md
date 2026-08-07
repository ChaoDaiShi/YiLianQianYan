# 忆涟千言 — 桌面级 AI 智能体

基于 **Rust + React** 前后端分离架构的桌面级 AI 智能体，可以帮你控制电脑。

## 架构

```
frontend (:1420)          backend (:9420)           src-tauri (optional)
┌──────────────┐   HTTP   ┌──────────────┐   IPC   ┌──────────────┐
│ React + Vite │ ◄─SSE──► │ axum + Rust  │ ◄─────► │ Tauri shell  │
│ 纯 web app   │          │ agent/tools  │         │ 桌面窗口     │
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
cargo run
# → http://127.0.0.1:9420
```

### 2. 启动前端

```powershell
cd frontend
npm install
npm run dev
# → http://localhost:1420
```

浏览器打开 `http://localhost:1420` 即可使用。

### 3. (可选) Tauri 桌面壳

```powershell
npm run tauri dev
```

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
│       ├── db/                  # SQLite 持久化
│       └── config/              # 配置管理
├── frontend/                    # React 前端
│   └── src/
│       ├── api/client.ts        # fetch + SSE 客户端
│       ├── components/          # UI 组件
│       │   ├── chat/            # 聊天界面
│       │   ├── sidebar/         # 对话列表
│       │   └── settings/        # 设置面板
│       └── types/               # TypeScript 类型
├── src-tauri/                   # Tauri 桌面壳 (可选)
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
| `GET` | `/api/health` | 健康检查 |

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

## 安全机制

YiLianQianYan 对 Tool 进行风险分级，在执行前进行权限评估。

| 风险等级 | 策略 |
|----------|------|
| `Low` | 自动执行 |
| `Medium` | 默认允许 |
| `High` | 需要用户批准 |
| `Critical` | 需要用户明确批准 |

安全链路：

```text
Tool Call
  ↓
SafetyPolicy.assess() → 最终风险等级
  ↓
PermissionManager.evaluate() → Allow / RequireApproval
  ↓
执行 或 暂停等待用户审批（approval_required）
```

### 用户审批

高风险与关键风险操作会暂停执行并请求用户确认。

只有用户明确选择"允许本次"后，系统才会执行最初请求的 Tool Call。

拒绝后，该 Tool 不会执行，Agent 可以基于拒绝结果重新规划。

审批 API：

```text
POST /api/approvals/:id/approve    允许 → 执行原始 Tool Call → 继续 Agent（SSE）
POST /api/approvals/:id/reject     拒绝 → 写回拒绝结果 → 继续 Agent（SSE）
POST /api/approvals/:id/cancel     取消 → 标记取消，不执行
GET  /api/approvals/:id            查询单个审批
GET  /api/approvals/pending        列出待审批
```

注意：当前 `PendingApproval` 为运行时状态（内存存储）；后端重启后，未处理审批会失效。

### 结果验证

YiLianQianYan 不会仅根据 Tool 返回 success 就认定任务完成。

对于支持确定性验证的操作，系统会在 Tool 执行后重新观察真实状态，例如：

- 文件是否真实存在
- 文件内容是否符合预期
- 目标进程是否真实运行

验证失败时，Agent 会收到验证结果并重新规划。

当前 Verifier 主要覆盖文件与进程等确定性场景。
Windows UI 与视觉结果验证将在后续版本扩展。
