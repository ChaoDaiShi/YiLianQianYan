# 两个项目的源码研究

返回 [R3 总文档](README.md)。本页区分：源码事实、静态接线判断、设计选择和待验证行为。下列缺口没有被当作本轮已复现的运行 bug。

## 1. 两个产品的职责重心

忆涟千言以本地工作台、任务执行、真实工具与结果验证为中心。既有 Task、TaskGraph、Workflow、ProofLoop 与 SQLite/Resource 都已有独立职责；前端是多页面 React Router 工作区，Tauri 承载桌面。

Cyrene 的外层围绕聊天与角色体验组织：`AppRouter` 保持 ChatPage，在设置打开时隐藏它并挂载设置页；聊天内部再切换定时任务、动态和右侧内容。其 Code/Learn 是带特定目录与工具语境的会话消费者，不等于独立的全功能 IDE 或 LMS。[Cyrene AppRouter](https://github.com/Playa-Cyrene/Cyrene-Agent/blob/dc55c5f66768abf2fe52b3631b7df71b5449cd69/src/renderer/react/app/routing/AppRouter.tsx#L7)、[面板宿主](https://github.com/Playa-Cyrene/Cyrene-Agent/blob/dc55c5f66768abf2fe52b3631b7df71b5449cd69/src/renderer/react/features/chat/components/ChatPagePanelHost.tsx)。

**设计选择：** 保留我们的 NavRail 与已有路由，用已有工作空间承载代码、学习和成果，借鉴按需打开面板的行为。

## 2. 普通聊天与附件：最需要补齐的输入链

### 忆涟千言实际路径

```text
ChatInput.addFiles
  → ingestResource
  → getResourcePreview
  → handleSend(text, queue.readyIds(), options)
ChatView.handleSend
  → 必要时 createConversation
  → bindResources({kind:conversation}, resourceIds)
  → chatRuntime.send
  → POST /api/chat（文本 + 现有 options）
后端 chat_handler
  → acquire_turn
  → prepare_with_model
  → 角色/工作流/记忆/knowledge grounding
  → load_chat_messages
  → AgentState → engine
```

来源：[ChatInput](../../../../frontend/src/components/chat/ChatInput.tsx)、[ChatView](../../../../frontend/src/components/chat/ChatView.tsx) 的 220–245 行、[chat_handler](../../../../backend/src/api/chat.rs) 的 678–953 行。

`ResourceTarget::Conversation` 与绑定 API 已存在。全后端对 resource_bindings 的调用搜索中，运行期内容读取落点是 `bound_node_resources`，其读取 Graph/Node；普通 chat 的已读路径没有消费 Conversation 绑定。`ChatMessage.content` 目前为 `Option<String>`，不能据此宣称原生多模态聊天输入已经完成。

**静态判断 Y01：** 上传/绑定与普通模型请求之间存在待接通点。后续应用测试应上传唯一标记文件，检查实际 provider 请求是否包含它，不能只检查附件缩略图出现。

**Cyrene 对照：** Composer 明确携带附件并按模式提供文件/Vault 选择；`file-ingest.ts` 将 pending 附件按图片、文本或其他文件区分，这个 helper 自身不能解析 PDF/DOCX，不代表其整个知识库都不支持这些格式。不能把单个 helper 的限制扩大成全项目结论。[Composer](https://github.com/Playa-Cyrene/Cyrene-Agent/blob/dc55c5f66768abf2fe52b3631b7df71b5449cd69/src/renderer/react/features/chat/components/ChatComposer.tsx#L351)、[附件描述](https://github.com/Playa-Cyrene/Cyrene-Agent/blob/dc55c5f66768abf2fe52b3631b7df71b5449cd69/src/main/rag/file-ingest.ts)。

## 3. 会话运行：已有所有者，缺少稳定的轮次关联

我们已有 `conversationRuntime`，生命周期不随 ChatView 卸载消失；`ChatTurnEvents` 将执行事件、推理快照与终态发布排序。`acquire_turn` 是每会话 async mutex，当前语义是等待取得锁，不是 R2 里写的“已有运行立即返回 409”。[前端所有者](../../../../frontend/src/features/execution/chatRuntime.ts)、[后端生命周期](../../../../backend/src/api/chat_lifecycle.rs)。

`ConversationRunStatus` 持久化在会话上，包含 idle/running/waiting_approval/completed/failed/cancelled/interrupted。工具记录有 tool_call_id、审批状态、执行状态与验证状态，尚无显式 turn_id。普通 user message 写入、标题更新和初始 running 更新处存在 `let _ =`；后续 assistant 持久化有错误处理。

**静态判断 Y02：** 本轮归属与部分早期写入错误仍需闭合；已有生命周期不是空白。需要在现有 owner 上追加轮次元数据，并明确一次接收事务，不重写整个 SSE/runtime。

Cyrene 当前 HarnessRunStore 已以 metadata 为主，Transcript projection 区分 UI、模型上下文和恢复信息；这不是“保存所有消息到另一个 run JSON”那么简单。普通运行状态与未结算工具状态也分开。[run-store](https://github.com/Playa-Cyrene/Cyrene-Agent/blob/dc55c5f66768abf2fe52b3631b7df71b5449cd69/src/main/orchestrator/harness/run-store.ts)、[transcript projection](https://github.com/Playa-Cyrene/Cyrene-Agent/blob/dc55c5f66768abf2fe52b3631b7df71b5449cd69/src/main/orchestrator/conversation-transcript-projection.ts#L240)。

**设计选择：** 复用 messages 与 execution_history，只新增可引用的 turn 记录、上下文摘要和输出关联；不移植另一套 canonical journal。

## 4. 上下文压缩：配置与执行必须相连

本地 `CompactionSettings` 提供开关、窗口、阈值与保留 token；`CompactionConfig` 保存它们；`AgentState::estimate_tokens` 为粗估。全后端搜索 compaction/keep_recent_tokens/trigger_threshold，没有找到 engine 调用压缩实现。[配置](../../../../backend/src/config/types.rs)、[state](../../../../backend/src/agent/state.rs)、[设置](../../../../frontend/src/features/settings/sections/CompactionSettings.tsx)。

**源码事实 Y03：** 这是配置已有、运行消费尚缺的明确开发落点。

Cyrene 在每轮请求前计算 system、tool schemas、messages 与输出预留，用 safe cut 避免拆开工具调用和结果；压缩检查点进入可恢复上下文。[预算与边界](https://github.com/Playa-Cyrene/Cyrene-Agent/blob/dc55c5f66768abf2fe52b3631b7df71b5449cd69/src/main/orchestrator/harness/compaction.ts#L51)、[Harness 调用](https://github.com/Playa-Cyrene/Cyrene-Agent/blob/dc55c5f66768abf2fe52b3631b7df71b5449cd69/src/main/orchestrator/harness/cyrene-harness.ts#L537)。

借鉴预算和协议边界；不复制其自动重试、降级或失败兜底策略。完整历史继续保留，压缩只改变下一次模型输入。

## 5. 必要询问与审批不是同一流程

我们有 TaskDecision/TaskDecisionOption，要求 task_id 和 task_execution_id；resolve_decision 校验选项并更新 resolved，但已读实现没有把选择结果持久化成答案再续接通用 chat 的步骤。[模型](../../../../backend/src/modules/task/model.rs) 532 行、[接口](../../../../backend/src/api/tasks.rs) 500 行。

**静态判断 Y04：** “已有选项数据”不等于“普通对话可暂停等答案”。应复用 UI 与选项校验，保持任务决策 owner；普通 chat 的 pending input 关联真实 turn，不伪造 task_id。

Cyrene 的 tool-round 把 ask_user/submit_plan 作为排他调用，未执行的同轮工具写 not_executed，避免工具协议悬空，用户等待单独计时。[实现](https://github.com/Playa-Cyrene/Cyrene-Agent/blob/dc55c5f66768abf2fe52b3631b7df71b5449cd69/src/main/orchestrator/harness/tool-round.ts#L224)。借鉴这类完整行为，不把普通澄清做成权限批准。

## 6. 模式：保留我们已有的三种语义

我们是 chat/work/create，且实际工具事件会影响显示模式。Cyrene 是 chat/work/code/learn，并由 mode-prompt-profile 与 run-capabilities 共同决定 prompt、tool 和 skill。

参照仓库简介中的“Chat 不暴露工具”不足以描述当前实现：源码含 chatBuiltin、hasFileAttachments 和 chatToolsEnabled 的 opt-in 分支，Chat skill 列表仍为空。[精确代码](https://github.com/Playa-Cyrene/Cyrene-Agent/blob/dc55c5f66768abf2fe52b3631b7df71b5449cd69/src/main/orchestrator/run-capabilities.ts#L27)。

**设计选择：** 我们仍保留聊天/工作/创造；代码、学习是有明确输入范围的预设，放在模式菜单二级选择，不增添第五种互相矛盾的权限模式。权限仍以原网关和用户授权决定。

## 7. 知识与个人记忆：两条检索链必须说清

我们已有文档正文、Resource、hash 去重、派生 memory chunks；knowledge_context 遍历 canonical 全文，用关键词排名和重叠窗口选片段。memory 检索另有 lexical/vector/hybrid。[知识数据](../../../../backend/src/db/knowledge.rs)、[全文检索](../../../../backend/src/db/knowledge/context.rs)、[记忆检索](../../../../backend/src/db/memories.rs)。

**源码事实 Y05：** 当前“文档正文 grounding”不能直接称为 BM25+向量+Reranker 全管线；已有混合记忆检索也不能重新写成从零新增。

Cyrene HybridRetriever 合并向量与 BM25 候选，存在可选 reranker；MemoryMode 有 vector/summary/wiki/off，L0/L1/L2 调用点仅在 vector 模式工作。[retriever](https://github.com/Playa-Cyrene/Cyrene-Agent/blob/dc55c5f66768abf2fe52b3631b7df71b5449cd69/src/main/rag/retriever.ts#L32)、[memory mode](https://github.com/Playa-Cyrene/Cyrene-Agent/blob/dc55c5f66768abf2fe52b3631b7df71b5449cd69/src/main/memory/memory-mode.ts)。

**设计选择：** 先明确资料范围、原文 offset/hash 与检索来源。混合检索作为用户明确配置的一种策略；启用后失败显示真实失败，不新增隐式回退。个人记忆增加生命周期和纠正关系，不引进三个新的记忆库。

## 8. 生图与成果：地址不等于已保存文件

`BasicServiceTool` 的 image_generation 返回 Markdown 图片地址及结构化结果；它自身没有调用 ResourceService 保存图片。当前 ArtifactSource 只有 Task/Node，Artifact 也要求 task_id/task_execution_id。[工具](../../../../backend/src/tools/basic_services.rs) 257 行附近、[ArtifactSource](../../../../backend/src/modules/task/artifact.rs) 30 行、[Artifact](../../../../backend/src/modules/task/model.rs) 359 行。

**静态判断 Y06：** 普通聊天生成图片和文件需要显式登记输出，不能在 UI 中虚构 artifact_id。现有 task artifact materialize_as 与 DOCX/XLSX/PDF renderer 应继续复用。

**设计选择：** 通用输出投影以 Resource 为实际内容、以 source 为来源联合类型；task/node 来源附带真实 artifact_id，conversation 来源不伪造任务。版本、预览和下载建立在该真实关联上。

## 9. 画布、任务图与“当前任务”入口

本地 revision7 明确把工具过程从消息节点中移除；普通分支复制完整祖先协议前缀，但可见节点只有用户输入与助手输出。`currentTaskEntry.ts` 返回 `/connectors`，对应测试也要求不探测已移除的旧任务页面。

TaskWorldPage 并未消失：它由 `TaskGraphWorkflowList` 在能力中心的工作流管理中打开；WorkspaceDetailPage 还有 TaskDetailPanel。[实际入口](../../../../frontend/src/features/workflows/TaskGraphWorkflowList.tsx)、[消息分支](../../../../backend/src/db/connector_messages/overview.rs)、[头像入口](../../../../frontend/src/components/chat/currentTaskEntry.ts)。

**设计选择：** 不推翻已明确的消息画布交互。新结果卡如果确有 task/graph 关联，增加精准的“查看执行/验收”入口；不存在关联时不显示按钮。

## 10. 技能与插件：发现、可用、执行三个状态

本地 Skill 已进行 portable/adapted/external_host 分类，检查实际工具映射，外部宿主能力缺失时不暴露给模型；工具页已改为紧凑列表和单项详情。插件是 plugin.json 声明式贡献，不能运行任意 JS。

Cyrene 的 Skill 有 work/code/learn 范围，插件宿主提供独立会话、调度、语音输入等服务。[Skill 类型](https://github.com/Playa-Cyrene/Cyrene-Agent/blob/dc55c5f66768abf2fe52b3631b7df71b5449cd69/src/main/skills/types.ts)、[插件宿主目录](https://github.com/Playa-Cyrene/Cyrene-Agent/tree/dc55c5f66768abf2fe52b3631b7df71b5449cd69/src/main/plugin-host)。

**设计选择：** 补技能在当前会话中的选择、使用记录与所需服务说明；不安装 Cyrene SDK，不复制 Node 插件宿主。MCP 仍是外部执行能力的明确入口。

## 11. 浏览器、代码与学习：平台差异决定落点

Cyrene BrowserPanelController 使用 Electron WebContentsView，包含页签、观察、选择器、控制和会话持久化；不能复制到 Tauri renderer 当作普通 React iframe。[浏览器控制器](https://github.com/Playa-Cyrene/Cyrene-Agent/blob/dc55c5f66768abf2fe52b3631b7df71b5449cd69/src/main/browser/browser-panel-controller.ts#L2)。我们已有独立 Playwright MCP context，应显示真实观察与截图，并提供外部浏览器窗口操作入口，不假画可交互网页。

Cyrene Git 以会话的 Code 模式与 workspaceBinding 解析目录，已有 status/commit/push 等；我们当前 Git API 为指定 workspace 的只读状态与 diff。新增写操作必须逐项通过已有网关，不能仅移植一排按钮。[Git service](https://github.com/Playa-Cyrene/Cyrene-Agent/blob/dc55c5f66768abf2fe52b3631b7df71b5449cd69/src/main/code-git/git-service.ts#L133)。

Cyrene Learn 基于 Vault 读写章节和 progress.md；部分 mastery 为模型估算，post-turn 提取异步写回。[Vault service](https://github.com/Playa-Cyrene/Cyrene-Agent/blob/dc55c5f66768abf2fe52b3631b7df71b5449cd69/src/main/learn/obsidian/obsidian-workspace-service.ts#L149)、[进度类型](https://github.com/Playa-Cyrene/Cyrene-Agent/blob/dc55c5f66768abf2fe52b3631b7df71b5449cd69/src/main/learn/progress/learn-progress-types.ts)。我们选择知识集合+工作空间笔记，客观练习记录与模型建议分开，不把模型 mastery 当作已测准确率。

## 12. 定时与渠道：取流程，不复制授权政策

Cyrene 时间任务与 TaskGraph ready 调度不是同一概念；它支持多种时间规则，保存 workspaceBinding、可用工具与运行历史。渠道有 keyed queue、外部会话绑定和 delivery service。[时间任务类型](https://github.com/Playa-Cyrene/Cyrene-Agent/blob/dc55c5f66768abf2fe52b3631b7df71b5449cd69/src/main/scheduler/types.ts)、[消息调度](https://github.com/Playa-Cyrene/Cyrene-Agent/blob/dc55c5f66768abf2fe52b3631b7df71b5449cd69/src/main/channels/dispatcher.ts#L160)。

其 scheduler-runner 的 applyScheduledExecutionPolicy 显式设置 `permissionMode: allow_all`，渠道某些工具模式也如此，群聊或关闭工具时则不同。这是所读代码分支，不是对参考项目全部安全性的结论。[定时策略](https://github.com/Playa-Cyrene/Cyrene-Agent/blob/dc55c5f66768abf2fe52b3631b7df71b5449cd69/src/main/scheduler/scheduler-runner.ts#L55)、[渠道策略](https://github.com/Playa-Cyrene/Cyrene-Agent/blob/dc55c5f66768abf2fe52b3631b7df71b5449cd69/src/main/channels/agent-policy.ts#L12)。

**设计选择：** 借鉴冻结执行规格、会话归属和真实发送回执。我们继续走 SecurityExecutionGateway，风险审批不能因为定时或外部消息而消失。没有用户明确授权，不新增自动重试、转投或后台外发。

## 13. 已有证据与本轮证据的界线

已查看本地 revision7 的真实聊天、设置与消息树截图，也读取其验证报告。报告说明使用受控模型协议和真实文件/Git/SQLite/Tauri；这是仓库已有证据，不等于当前配置下所有外部服务可用。

远端 UI 截图链接读取未成功，因此本轮关于 Cyrene 页面结构的判断来自 AppRouter、Navigation、Inspector、Composer 代码，没有假称已运行或实际看到其最新桌面程序。

本轮新产生的证据只包括研究清单、文档检查和独立原型的浏览器交互/截图。后续产品开发仍需执行对应测试与真实服务验收。
