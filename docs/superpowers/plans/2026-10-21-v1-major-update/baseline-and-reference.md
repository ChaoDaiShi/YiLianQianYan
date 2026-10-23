# v1 更新的基线与参照证据

返回 [主开发文档](../2026-10-21-v1-major-update.md)。

## 1. 证据范围

核对日期为 2026-10-21。本次读取本地源码、路由、类型与部分测试定义，读取参照仓库的 README、技术说明、目录及指定实现；没有启动两个产品，也没有执行它们的测试。因此“源码存在”与“运行通过”必须分别表述。

本地分支：`codex/connector-workbench`。HEAD：`4399a43718834e53c2d25d09ff7a88a52545397f`。该工作区有大量未提交修改，包含未跟踪的 connector、knowledge、model pool、settings canvas 等文件。实施前应重新核对工作区，不能覆盖或清理这些内容。

用户提供的 [原始 Cyrene-Agent 地址](https://github.com/Playa-0v0/Cyrene-Agent) 当前重定向至 [Playa-Cyrene/Cyrene-Agent](https://github.com/Playa-Cyrene/Cyrene-Agent)。读取时 `master` 对应提交 [a97db7ab8ca6ae820cc7bc042bb1a851aeaf8626](https://github.com/Playa-Cyrene/Cyrene-Agent/commit/a97db7ab8ca6ae820cc7bc042bb1a851aeaf8626)。以下使用该提交的固定链接。

## 2. 参照取舍

| 编号 | 已读取的来源 | 可借鉴内容 | 忆涟千言中的落实 |
|---|---|---|---|
| R01 | [README](https://github.com/Playa-Cyrene/Cyrene-Agent/blob/a97db7ab8ca6ae820cc7bc042bb1a851aeaf8626/README.md) | 会话场景、代码与学习、语音、扩展能力组合 | M03、M06、M07、M09、M11；保留自己的三个模式 |
| R02 | [CyreneHarness 技术说明](https://github.com/Playa-Cyrene/Cyrene-Agent/blob/a97db7ab8ca6ae820cc7bc042bb1a851aeaf8626/docs/references/cyrene-harness.md) | 连续运行、等待用户、上下文压缩、工具输出引用、轨迹投影 | M01、M02；增量扩展现有 ReAct 与数据库，不整体搬运 Harness |
| R03 | [timeout-clock.ts](https://github.com/Playa-Cyrene/Cyrene-Agent/blob/a97db7ab8ca6ae820cc7bc042bb1a851aeaf8626/src/main/orchestrator/harness/timeout-clock.ts) | 执行时间与用户等待时间分别累计 | M02 的执行预算与等待展示 |
| R04 | [memory-types.ts](https://github.com/Playa-Cyrene/Cyrene-Agent/blob/a97db7ab8ca6ae820cc7bc042bb1a851aeaf8626/src/main/memory/memory-types.ts) | 记忆生命周期、来源证据、替代关系 | M05 的用户可见管理；不复制自动裁决与衰减算法 |
| R05 | [scheduler/execution-spec.ts](https://github.com/Playa-Cyrene/Cyrene-Agent/blob/a97db7ab8ca6ae820cc7bc042bb1a851aeaf8626/src/main/scheduler/execution-spec.ts) | 定时规则与执行内容分开管理，启用对应具体规格 | M08 的明确配置与单次触发关联 |
| R06 | [插件开发指南](https://github.com/Playa-Cyrene/Cyrene-Agent/blob/a97db7ab8ca6ae820cc7bc042bb1a851aeaf8626/docs/plugins/plugin-dev-guide.md) | 工具、设置、页面、事件和宿主能力的连接体验 | M09 的声明式贡献闭环；不运行指南中的 Node/Electron 插件代码 |
| R07 | [learn 目录](https://github.com/Playa-Cyrene/Cyrene-Agent/tree/a97db7ab8ca6ae820cc7bc042bb1a851aeaf8626/src/main/learn) | 源码目录中存在试卷、Vault、进度模块 | 仅作为范围参照；M07 是我们拟定的本地资料学习方案，不声称已验证参照项目学习效果 |
| R08 | [tool-output 目录](https://github.com/Playa-Cyrene/Cyrene-Agent/tree/a97db7ab8ca6ae820cc7bc042bb1a851aeaf8626/src/main/orchestrator/harness/tool-output) | 长工具输出有存储与回读模块 | M02 复用自己的 Resource 存储 |

源代码、角色形象与素材使用不是同一个问题。本次只提炼功能与交互设计，不复制参考项目模型、美术、提示词或源码。后续若实际复用代码或素材，应分别记录来源及对应许可；不能把仓库代码许可视作所有素材许可。

## 3. 本地关键证据索引

以下均为读取时真实存在的路径。行号会随当前未提交工作变化，故用类型/函数名作为复查锚点。

| 编号 | 路径 | 锚点与核对结果 |
|---|---|---|
| L01 | `frontend/src/surfaces/workspace/WorkspaceSurface.tsx` | 实际 route 列表；没有单独的 code/learn/automation 路由 |
| L02 | `frontend/src/components/layout/navRegistry.ts` | 主页、画布、工作空间、能力为主要导航 |
| L03 | `frontend/src/pages/SettingsPage.tsx` | 直接导出 SettingsCanvasPage |
| L04 | `frontend/src/features/settings/canvas/SettingsCanvasPage.tsx` | 设置节点、section 解析、智能体资源绑定 |
| L05 | `frontend/src/features/capabilities/CapabilityNavigation.tsx` | 分类、查询参数和管理弹窗 |
| L06 | `backend/src/db/conversation_modes.rs` | WorkspaceMode 是 Chat/Work/Create；for_tool 调整显示模式 |
| L07 | `backend/src/api/chat_options.rs` | ChatRequestOptions 与审批续接设置持久化 |
| L08 | `backend/src/api/chat_lifecycle.rs` | ChatTurnEvents 排序、持久化与终态发布 |
| L09 | `backend/src/api/chat_runtime.rs` | 模型来源、绑定能力、readiness 的真实解析 |
| L10 | `backend/src/db/conversations.rs` | ConversationRunStatus、消息、推理文本和 execution_history |
| L11 | `backend/src/agent/engine.rs` | AgentEvent、ReAct loop、20 轮上限、连续相同工具 3 次上限、完成检查 |
| L12 | `backend/src/agent/state.rs` | estimate_tokens 为粗略估算；消息内容和工具调用列表 |
| L13 | `backend/src/config/types.rs` | CompactionConfig 及阈值；全后端搜索未找到运行消费 |
| L14 | `backend/src/api/connector_messages.rs` | 消息分支调用普通 chat runtime，不是第二套聊天执行器 |
| L15 | `backend/src/db/connector_messages.rs` | 分支复制完整祖先协议前缀，数据库迁移 1023 |
| L16 | `backend/src/db/connectors.rs` | agent 资源绑定与画布布局分离，迁移 1021 |
| L17 | `backend/src/api/knowledge.rs` | 资源导入、详情、搜索、删除 API |
| L18 | `backend/src/db/knowledge.rs` | canonical 文档正文、派生 memory chunk、hash 去重，迁移 1025 |
| L19 | `backend/src/db/knowledge/context.rs` | 全文遍历排名、重叠窗口与真实片段 |
| L20 | `backend/src/api/chat_grounding.rs` | 初始聊天与审批续接使用文档 grounding；restricted 路径不注入全库 |
| L21 | `backend/src/db/memories.rs` | Memory DTO、检索与 embedding；未见专用生命周期字段 |
| L22 | `backend/src/agent/memory/context.rs` | 已存在混合检索及 lexical fallback，本文不扩大这一策略 |
| L23 | `backend/src/agent/memory/policy.rs` | 已有候选长度、重复、敏感信息检查 |
| L24 | `backend/src/api/workspace_git.rs` | 只读 `/api/workspace-git` 与 `/diff` |
| L25 | `backend/src/modules/task/scheduler.rs` | DeterministicScheduler 计算 TaskGraph ready 集；非时间调度 |
| L26 | `backend/src/plugin/manifest.rs` | plugin.json 声明式贡献，不加载任意 JS/原生代码 |
| L27 | `backend/src/integrations/llm/model_pool.rs` | 已有模型池与路由事件，不在本文新增 fallback |
| L28 | `plugins/browser-adapter/README.md` | 可选独立浏览器、7 个 browser 工具、截图、未知提交状态 |
| L29 | `docs/architecture/v1-v2-boundary.md` | v1 Task World、v2 Desktop World；Workflow DAG 与 TaskGraph 分离 |
| L30 | `frontend/package.json` | test:e2e 当前默认仅带 `--workbench-revision6`，不能当作所有新增流程验收 |
| L31 | `backend/src/db/migrations.rs` | shared 0–999；v1_task_world 1000–1999；cloud client 3000–3999 |

## 4. 不能从本次核对推出的结论

- 存在测试代码不等于测试已运行。
- 已有 provider 配置不等于真实 API 连接正常。
- 已有语音/悬浮窗口文件不等于麦克风、播放、打断均可用。
- 当前工作区存在大量更新，不等于它们已提交、已集成或已发布。
- 参照仓库的功能说明不证明其所有场景都稳定，也不要求我们复制其架构。
- 本次未读取私有云仓库，不在文档中安排服务器、Admin 或云数据库变更。

## 5. R2 补充核对

用户要求同时补全功能方案与各页面预览后，再次读取了当前 WorkspaceSurface、WorkbenchHome、MessageBubble、AgentProfile、基础服务工具、文档提取与成果 renderer。以下是增量依据：

- `backend/src/api/agent_persona.rs` 已提供角色姓名、语气、语言、头像与用户提示词；M14 扩展其使用流程，不重复建立 Persona。
- `backend/src/modules/resource/document.rs` 明确返回图像/扫描件 OCR 暂不支持；M13 将 OCR 作为待开发能力，不能把现有文档导入描述成完整 OCR。
- `backend/src/modules/task/deliverable_renderers/mod.rs` 的格式枚举包含 Markdown、JSON、DOCX、XLSX、PNG、PDF；PPTX 是拟新增项。
- `frontend/src/components/chat/MessageBubble.tsx` 已有复制与朗读入口；编辑/重新生成/分支的完整产品流程仍需按 M12 接线和验收。
- 参照渠道范围补充读取 [飞书接入说明](https://github.com/Playa-Cyrene/Cyrene-Agent/blob/a97db7ab8ca6ae820cc7bc042bb1a851aeaf8626/docs/user-guide/feishu.md) 与 [QQ/OneBot 接入说明](https://github.com/Playa-Cyrene/Cyrene-Agent/blob/a97db7ab8ca6ae820cc7bc042bb1a851aeaf8626/docs/user-guide/napcat-onebot.md)。它们用于理解参考产品的接入场景，不作为我们已经适配这些平台的证据。平台协议实现前仍需核对供应商/协议官方资料。

R2 首次默认收起会话历史与详情，是本次提出的交互调整；并非声称当前版本已经如此。预览是 AI 生成的概念图，图内任务、记录和状态为示意，不是软件截图或验收产物。
