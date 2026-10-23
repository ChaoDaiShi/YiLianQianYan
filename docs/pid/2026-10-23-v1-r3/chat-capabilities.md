# 消息操作、协议修复与能力工作台

本轮继续在`codex/v1-r3-knowledge-memory`上修改。目标来自用户的实际HTTP400截图、弹窗留白反馈和能力页面重构请求。

## 设计与实现

Figma插件已创建[辅助设计文件](https://www.figma.com/design/NkNZec0gY6jUgyCQuMGs2u?node-id=2-129)，使用仓库现有IBM Plex Sans、昔涟粉/紫、背景和圆角标记；设计包含固定类别导航、列表/详情区域和消息操作示意。实现复用原React组件、Tauri与Rust接口。

- 构建弹窗取消760px固定高度，按内容收缩；保留模式切换、居中按钮和独立历史。
- 用户消息增加回退和单条删除，回答下方常驻复制、朗读和删除。回退删除明确选择的用户消息及后续内容，并恢复文本草稿与现有会话附件。
- 附件预览、来源分类和会话分支体验已接入。历史图片根据工具 `resource_id` 与上传轮次恢复原位置；新附件与对应消息关联。
- 失败重试先回退本次真正提交的消息，再重发，避免重复追加相同问题；请求未接收时不会误删旧成功消息。
- 修复工具循环中断时未配对的assistant工具调用。旧损坏历史不会在普通请求时自动修改；可通过明确回退恢复，保留前缀仍损坏时定位到受影响消息。
- 能力改为六类固定导航、原页面内管理，去掉分类外层大弹窗；保留单次新增/编辑/审批弹窗与原工作流编辑器。
- 补齐MCP搜索与连接筛选、工具列表/详情、Skill来源详情、插件清单及类别专属导入、发现/结构导入后的实际管理跳转。继续使用已实现接口。

参考了[Cyrene-Agent Skill管理](https://github.com/Playa-Cyrene/Cyrene-Agent/blob/1f1af913434ebf93cc1edcf9fcaa56c12c5ee0ed/src/renderer/react/features/settings/SkillSettingsPanel.tsx)的搜索/来源组织及MCP高级配置分区，不移植其Electron宿主接口。

## 故障证据

只读检查确认截图会话包含未获得tool结果的assistant工具调用，后面又追加多个用户消息。当前引擎在先加入assistant调用后，可能因连续同工具限制或取消而返回错误；未完成协议消息随后被持久化。后续请求违反工具调用配对，重试再追加用户文本，形成持续失败。

开发阶段不执行测试、构建、check或fmt，所有模块接入后统一验证。最终证据保存在`artifacts/v1-r3-chat-capabilities/`，不使用用户原会话进行破坏性验收。

## 主要代码位置

| 范围 | 修改或新增位置 |
|---|---|
| 消息回退/删除API与事务 | 新增`backend/src/api/conversation_messages.rs`、`backend/src/db/conversation_messages.rs`及各自测试；注册到原受保护router |
| 工具协议与错误终态 | 新增`backend/src/agent/state/tool_protocol.rs`，修改原Agent engine/state、chat admission与connected事件 |
| 聊天交互与草稿 | 修改MessageBubble、ChatView、聊天输入组件、conversationRuntime、聊天API及附件队列 |
| 画布连线状态 | 修改ConversationCanvas，新增`reconcileMessageNodes.ts`及测试，保留节点测量字段，连线ID与Bezier关系不变 |
| 能力工作台 | 修改CapabilityNavigation及样式、SkillsPage、PluginsPage、ToolManager、ManagedImports、StructuredImport、LocalServiceDiscovery、WorkflowsPage；新增CapabilityImportWorkspace |
| 弹窗空间 | 修改`frontend/src/features/knowledge/knowledgeLibrary.css`，移除固定高度与空白占位 |
| 回归 | 更新原前端契约/运行时测试及`workbench-revision7.mjs`、`v1-r3.mjs`，原断言按实际交互与完整协议数据保留 |

现有普通聊天、审批、必要输入、工作流与Resource身份保留；消息操作不执行外部工具或删除Resource文件，没有新增模型自动切换。代码没有提交、推送或打包发布。

## 最终统一验证结果

| 范围 | 最终结果 |
|---|---|
| `cargo fmt --all`及格式检查 | 退出码0 |
| `cargo test --workspace --no-fail-fast` | 43个集合，1425通过，0失败，退出码0 |
| `cargo check --workspace` / `cargo build --workspace` | 均退出码0 |
| 前端全量Vitest | 163个文件，776通过，0失败 |
| TypeScript与Vite生产构建 | 退出码0 |
| 原revision7与R3完整浏览器回归 | 31+14项，共45项通过，退出码0 |
| `git diff --check` | 退出码0 |

浏览器使用已有Playwright Core与Edge、隔离数据库和工作目录，覆盖1440×900、1280×720、800×600。实际核对了受控HTTP400后重试仅保留一条用户问题、常驻操作、删除回答保留用户消息、回退恢复输入草稿与不影响其他会话、紧凑弹窗、能力分类无外层Modal、结构化导入打开实际保存的工作流。最终控制台error为0，测试服务19420/11420端口已退出。真实模型账号未重放，原用户会话只读检查，未直接修改。

以下均为本机 `local evidence`，仅记录当时的验收运行，不保证其他机器存在：浏览器证据 `target/canvas-e2e/2026-10-23T04-27-10-625Z/`；截图副本 `F:/CodexTemp/v1-r3-chat-capabilities/previews/`；机器摘要 `artifacts/v1-r3-chat-capabilities/verification-summary.json`。

统一阶段修正了测试模块路径、MCP计数展示遗漏、聊天定位的外层滚动风险，以及消息画布刷新重建Node造成的测量状态竞争。旧聊天/审批测试夹具补齐真实assistant工具调用与tool结果配对；原历史、审计和执行次数断言均保留。最终完整复跑后才记录以上通过结果。

已有损坏历史需用户明确选择回退边界；如果较晚消息之前仍存在未配对调用，接口返回受影响消息ID，页面可定位原问题，不自动扩大删除范围。单条删除保留其他可见消息和独立分支；清理关联内部工具协议以保证后续模型请求有效。
