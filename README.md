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

在审批流程（Sprint 01C）完成前，高风险操作采用 **fail-closed** 策略：

> 不会未经用户确认直接执行。遇到 `High` / `Critical` 工具时，Agent 发送 `approval_required` 事件并停止该工具，当前版本尚未执行。

安全链路：

```text
Tool Call
  ↓
SafetyPolicy.assess() → 最终风险等级
  ↓
PermissionManager.evaluate() → Allow / RequireApproval
  ↓
执行 或 阻断（发送 approval_required）
```
