# R3 开发与验收规格

返回 [总文档](README.md)。本文件全部新增类型/路径均为设计，不是现有 API。功能包一次完整交付；包编号表示职责与依赖，不表示周期。

## 1. 保留的运行结构

```text
主页 / 消息分支 / 语音 / 用户配置的时间与渠道入口
    ↓ 使用同一普通会话接收服务
现有 chat lifecycle + conversationRuntime
    ↓ 本轮模型/角色/目录/资料/工具快照
现有 ReAct Engine
    ↓ SecurityExecutionGateway → 工具 → 观察与验证
现有 messages / execution_history + 拟新增轮次元数据
    ↓ Resource、Task Artifact、知识引用、页面投影
```

已有 Task/Workflow/ProofLoop 运行继续由对应服务拥有。普通聊天不因为上传文件或生成图片就自动伪造 Task。TaskGraph、Workflow、消息画布的节点与状态不能互相替代。

不引入另一套 Agent、前端 store、事件总线、云数据库或插件执行宿主。新增外部能力仍经现有 Registry/Gateway。原有模型池与已授权策略按当前代码保持，本次不新增自动 fallback 或自动修复机制。

## 2. 数据归属与最小增量

| 数据 | 当前 owner | R3 改动 |
|---|---|---|
| 用户/助手/tool 正文 | messages | 保留主键与完整历史，新增可空 turn_id 关联 |
| 审批/执行/验证 | execution_history + 现有批准存储 | 增加 turn_id，不由 UI 回写执行成功 |
| 当前会话状态 | conversations + ChatTurnEvents | 继续投影最近轮次状态；增加 waiting_input 的显式含义 |
| 每轮身份与上下文 | 尚无完整独立记录 | 新增 conversation_turns；不复制 messages 正文 |
| 附件与文件内容 | ResourceService + resource_bindings | 增加轮次使用快照；复用现有上传、预览、内容读取 |
| Task 成果 | Artifact + provenance | 不削弱真实 task/execution 要求 |
| 通用聊天成果 | 尚缺统一关联 | 新增 conversation_outputs，以 Resource 为内容，来源指向真实轮次 |
| 知识正文与片段 | knowledge_documents + 派生 chunks | 增加文档范围与版本引用，避免全库/绑定范围混用 |
| 个人记忆 | memories | 附属 lifecycle/usage，不重写原 memory ID 与 embedding |
| 画布/设置布局 | 既有布局存储 | 只存显示位置、展开与视口，不负责执行授权 |
| 定时/渠道 | 拟新增领域 | 只管触发和传输，运行交给已有服务 |

本地迁移 owner 继续使用既有分区规则。R1/R2 的 1030–1038 只是旧提议，不能直接执行；R3 实施时按真正需要的表分配未占用编号并登记 digest。不能为了套旧表清单重复建库。

## 3. 核心新增契约

### 3.1 ConversationTurn

```ts
type TurnState = "running" | "waiting_approval" | "waiting_input"
  | "completed" | "failed" | "cancelled" | "interrupted";
interface ConversationTurn {
  id: string;
  conversation_id: string;
  user_message_id: string;
  state: TurnState;
  context_snapshot_id: string;
  pending_input_id: string | null;
  approval_ids: string[];
  started_at: number;
  finished_at: number | null;
  active_ms: number;
  waiting_ms: number;
  revision: number;
}
```

所有时间为 UTC 毫秒。一次普通用户发送创建新 turn；同轮批准/回答询问沿用该 turn；编辑历史消息创建新分支和新 turn。现有 conversation mutex 语义保留，不按 R2 草案直接改为冲突错误；显式的“编辑/再生成”操作在已有运行时需用户先停止。

接收成功条件：模型/绑定检查完成，用户消息与 turn 记录已持久化，再进入运行。数据库写入失败不能用 `let _` 丢弃后仍报告已接收。把这个局部事务接在现有 lifecycle 上，不创建第二条状态发布路径。

### 3.2 本轮上下文

```ts
interface TurnContext {
  conversation_id: string;
  turn_id: string;
  workspace_mode: "chat" | "work" | "create";
  preset: "general" | "code" | "learn";
  workspace_id: string | null;
  agent_id: string | null;
  model_selection: { kind: "configured" | "profile" | "pool"; id: string | null };
  thinking_mode: string;
  resource_ids: string[];
  knowledge_scope: { kind: "none" | "all_allowed" | "selected"; document_ids: string[] };
  memory_ids: string[];
  capability_ids: string[];
  created_at: number;
}
```

thinking_mode 复用现有枚举。model_selection 复用 prepare_with_model/ChatRequestOptions 的选择优先级。resource_ids 是真实持久化绑定的快照；不是模型自由生成的路径。all_allowed 受 agent 绑定限制约束，不能跳过 restrict_knowledge。

审批续接使用原快照；执行前仍重新核对已撤销的权限与已停用能力。用户保存的新配置用于下一轮。快照不等于永久授权。

### 3.3 通用成果投影

```ts
type OutputSource =
  | { kind: "conversation"; conversation_id: string; turn_id: string; tool_call_id: string | null }
  | { kind: "task"; task_id: string; execution_id: string; artifact_id: string }
  | { kind: "node"; graph_id: string; node_id: string; execution_id: string; artifact_id: string };
interface OutputView {
  id: string;
  resource_id: string;
  name: string;
  mime_type: string;
  size: number;
  hash: string;
  source: OutputSource;
  parent_output_id: string | null;
  verification: "not_checked" | "file_checked" | "content_accepted" | "failed";
}
```

OutputView 是整合已有来源的投影，不是重新实现 ArtifactService。conversation_outputs 只补普通聊天关联，Task/Node 成果仍读已有 provenance；任务原文件导入 Resource 时使用已有真实字节，不把模型宣称的文件路径当作成果。

图像服务返回远端 URL 时，需要在本次已授权生成操作内明确取得实际文件并注册 Resource，才展示“已保存”；只得到地址则显示远端预览，不能计入可离线打开的成果。下载失败返回真实失败，不自动换 provider。

### 3.4 询问与知识引用

```ts
interface PendingInput {
  id: string;
  owner: { kind: "conversation"; turn_id: string } | { kind: "task"; decision_id: string };
  prompt: string;
  options: Array<{ id: string; label: string; description: string }>;
  allow_text: boolean;
  state: "pending" | "answered" | "cancelled";
  answer: { option_id: string | null; text: string } | null;
  revision: number;
}
interface SourceReference {
  id: string;
  resource_id: string;
  document_id: string | null;
  content_hash: string;
  start_char: number;
  end_char: number;
  label: string;
  source_state: "available" | "deleted";
}
```

TaskDecision 的既有 ID 与表继续保留。新增答案字段/附属记录及续接关联，UI 共用 PendingInput 投影；不向旧 TaskDecision 写入假的 task_id。字符范围使用 Unicode scalar 零基半开区间，前端按相同单位定位。

## 4. API 接线表

既有 `/api/chat`、stop、conversations、approvals、resources、resource-bindings、knowledge、workspaces、workspace-git、task/graph/workflow/ProofLoop API 保留。

| 类别 | 接口/修改 | 接线规则 |
|---|---|---|
| 复用 | 现有 resource ingest/preview/content 与 resource-bindings | 不再新增第二套附件上传协议 |
| 扩展 | POST `/api/chat` 的现有 options 增加 preset/workspace_id/knowledge_scope | 不再追加同义 upload API；从 conversation 绑定中取得附件并快照 |
| 新增 | GET `/api/conversations/:id/turns?cursor&limit`、GET `/api/conversations/:id/turns/:turn` | 查询轮次和 pending input，读取不重新发起模型 |
| 新增 | POST `/api/conversations/:id/turns/:turn/input` | `{input_id,expected_revision,option_id?,text?}`；事务保存答案并取得一次续接权 |
| 新增 | GET `/api/conversations/:id/outputs` | 返回 OutputView 列表；只读，不自动生成 |
| 新增 | GET `/api/outputs/:id`、POST `/api/outputs/:id/versions` | 新版本输入 `{resource_id,parent_output_id}`；验证关联与字节，保留旧版 |
| 新增 | GET `/api/conversations/search?q&cursor&limit` | 标题/正文匹配返回 conversation_id/message_id/excerpt |
| 扩展 | 既有消息画布 branch 创建 | 复用分支协议前缀；增加编辑/重新生成动作时明确祖先边界 |
| 新增 | GET/POST `/api/knowledge/collections`、PUT/DELETE `/api/knowledge/collections/:id` | 集合只保存文档成员；移除集合不删文档 |
| 扩展 | 现有 knowledge detail/search | 返回实际 SourceReference 与所选 scope；历史版本单独引用 |
| 扩展 | 现有 memory update/read | 增加 lifecycle 投影；归档/替代只影响未来召回，不改原消息 |
| 新增 | POST `/api/workspace-git/actions` | stage/unstage/commit/push，参数、批准、执行与观察闭合；不自动触发 |
| 新增 | POST `/api/workspaces/:id/code/query` | LSP definition/references/diagnostics，显式配置 server，不隐式安装 |
| 新增 | GET/POST `/api/workspaces/:id/learning` | 学习资料、笔记与作答记录；内容由 Resource 拥有 |
| 新增 | GET/POST `/api/automations`、PUT/DELETE `/api/automations/:id` | 保存定义与 revision，默认停用 |
| 新增 | POST `/api/automations/:id/enable`、`/pause`、`/run`；GET `/:id/history` | enable 固定定义；pause 不取消在途；run 新建明确 occurrence |
| 新增 | GET/POST `/api/channels/accounts`、PUT/DELETE `/:id` | 元数据与 SecretRef，响应不回显凭据 |
| 新增 | POST `/api/channels/accounts/:id/test`、`/connect`、`/disconnect` | 连接测试不自动向联系人发消息 |
| 新增 | POST `/api/channels/accounts/:id/send`、GET `/api/channels/operations/:id` | 进入现有外发权限规则，结果含平台回执或 failed/unknown |

新增接口只在对应功能实现时注册。请求字段必须给出实现时的 Rust/TS 类型并测试；表中是路由设计，不可用不存在的路径渲染“已连接”。读取分页 limit 1–100、默认 30，下一页返回 next_cursor；全局数量从统计查询获取。

SSE 保留原事件名和形状，新增可选 turn_id；新增 `input_required/context_compacted/output_registered/source_references`。没有 turn_id 的历史消息按旧记录显示，不猜测轮次。Product Event 只传播低频状态和实体 ID，token 仍走原聊天 SSE。

## 5. 功能包详细要求

### P01：输入与范围真正进入运行

**修改：** ChatView、chat_options、chat_runtime、chat_grounding、resource binding；**拟新增：** `backend/src/modules/resource/conversation_context.rs`。

- [ ] 用 ResourceTarget::Conversation 读取本轮绑定，复用 resource_preview/文本提取；普通聊天与审批续接调用同一上下文构造函数。
- [ ] 图片采用现有 vision_analysis 工具路径：给它增加 resource_id 输入，与既有 image_url 二选一；在原 Gateway 内核对本轮资源并取得图像，交给明确配置的 vision 服务。普通 ChatMessage 保持 String，不把图片对象直接塞入 content，不新增两种协议自动尝试。
- [ ] 工作空间选择形成明确会话绑定与执行 cwd，不能只改变 Git 面板；无目录时不隐式改用应用目录。
- [ ] 绑定 agent 的模型、能力、记忆、知识与本轮选择按既有优先级解析；界面可查看实际生效来源。
- [ ] 附件“可预览”“本轮已读取”“仅作为文件”分别显示，不能一律显示已理解。

**验收：** 含唯一字符串的附件实际出现在 provider 请求；切换会话不泄漏附件；批准后继续仍使用同一附件版本；无效绑定返回真实错误。

文本附件以带 resource/hash 的有限摘录进入上下文，完整内容通过已有 Resource 服务按需读取。图像附件注入真实资源清单与可用 vision 工具说明，图像内容由该工具处理；若 vision 未配置，界面明确标记“图像尚未分析”，不能仅凭文件名回答图像内容。

### P02：运行、压缩与询问

**修改：** chat lifecycle、engine、state、approvals、conversationRuntime；**拟新增：** `backend/src/agent/context_compaction.rs`、`backend/src/db/conversation_turns.rs`。

- [ ] 接收事务、终态与 SSE 发布由现有 lifecycle 统一处理；轮次 ID 在工具、批准、usage 与结果中一致。
- [ ] 请求前计算 system/tools/history/资料和输出预留；估算明确标记，不当作计费用量。
- [ ] 开启且超阈值时，按完整工具协议边界压缩；旧摘要进入下一次摘要输入，原历史不删除。
- [ ] 保存摘要 Resource、覆盖消息边界与 hash；压缩失败停止本次继续请求，显示原因，不丢历史或换模型。
- [ ] 工具长输出登记 Resource，新增按 output_ref 回读工具；只允许读本轮/合法祖先输出。
- [ ] ask_user 独占本轮交互，其他未执行工具补 not_executed；用户答案持久化后只续接一次。
- [ ] 执行时间与用户等待分开；既有审批有效期不因此自动延长。

**验收：** 刷新不重复工具；迟到 token 不覆盖取消；长对话仍遵守早期约束；重复回答只有一次生效；数据库写入失败不报告已接收。

### P03：知识与记忆

**修改：** knowledge 数据层/grounding、memory 检索与页面。

- [ ] 资料集合、会话所选范围与 agent 限制取交集，空范围不等于全库。
- [ ] 引用保留 resource/hash/字符范围，答案只使用本轮来源编号；长文尾部检索保持当前完整正文语义。
- [ ] 文档更新形成版本；旧引用仍指向旧版，删除后显示来源已删除。
- [ ] 向量/BM25/reranker 仅作为明确配置的检索策略，不能把配置按钮当已实现管线。
- [ ] memory 增加 active/archived/superseded、来源与作用域；旧数据默认 active，但不伪造来源证据。
- [ ] 用户纠正保留替代关系；文档派生知识不进入个人偏好自动合并。

**验收：** 选 A 不引用 B；长文尾部定位正确；新旧偏好不同时作为当前事实；删除知识不删用户手动记忆。

### P04：成果与多模态

**修改：** basic_services、Resource、artifact projection、现有 renderers；**拟新增：** conversation_outputs、通用结果预览组件。

- [ ] 普通聊天的文件/图片输出登记真实 Resource，记录 turn/tool_call；Task Artifact 保持原 provenance。
- [ ] DOCX/XLSX/PDF/Markdown/JSON/PNG 继续用现有格式能力；生成结构化内容到实际 renderer 的链路要完整。
- [ ] OCR、PPTX 读取/生成、image edit 是独立新增能力；每项有真实 provider/parser 与 capability readiness。
- [ ] image edit 使用选定原图，另存新版本；不以一次 text-to-image 请求冒充编辑。
- [ ] 文件存在检查与内容验收分开；用户编辑后当前版重新验收，不复用旧 report。
- [ ] 对话默认一行结果，点击才打开预览/来源/版本。

**验收：** 新文件可离线打开；图片可解码；office 文件真实类型匹配且中文可读；PPTX 页序正确；扫描件无文字页不编造内容。

### P05：代码协作

**复用：** 已有 workspace-git read、文件工具、Gateway、任务记录。

- [ ] 代码预设显式绑定 Workspace；读取、编辑、Git 和验证使用同一目录。
- [ ] 使用已配置 LSP 做定义/引用/诊断；未配置时明确不可用，文本检索不冒充 LSP。
- [ ] 查看 diff 与运行验证保留实际退出码、日志 Resource 和文件版本。
- [ ] stage/commit/push 仅由用户显式触发，经既有批准规则；没有自动清理、换分支、建 worktree 或提交。
- [ ] 保留当前 Git 默认关闭、未选目录不请求的行为。

**验收：** 受控项目实际文件发生预期变更；未选目录无 Git 请求；拒绝 push 后远端不变；不接触用户其他修改。

### P06：学习与持续创作

**复用：** Workspace、知识集合、普通聊天与 OutputView；不建立第二套资源仓库。

- [ ] 资料学习与代码同属工作预设，学习上下文限定所选资料；笔记归属空间，内容版本归属 Resource。
- [ ] 练习题保存来源、题目、答案、rubric；提交前 API 不返回隐藏答案。
- [ ] 客观题确定性评分；模型评阅与客观结果分别标识，不把模型掌握度当正确率。
- [ ] 文稿/图片版本用 parent_output_id 串联，恢复旧版不删除历史；再次生成保留用户编辑版。
- [ ] 复习提醒用 P08，文件导出用既有授权写入；不新增学习专用执行器。

**验收：** 选资料→笔记→题目→作答→更正→再次阅读完整可达，刷新不重生成题目或丢用户编辑。

### P07：能力与扩展闭环

- [ ] 保留 Skill portable/adapted/external_host 与真实 missing_tools；加载 Skill 不代表它的外部宿主已安装。
- [ ] 输入框支持选择本项目可用 Skill；选择时查看依赖和作用域，用后关联当前轮次。
- [ ] 能力主页仍为六类入口及管理弹窗，不改成同时展开所有列表的大 dashboard。
- [ ] Workflow 继续区分 prompt recipe 与可执行 graph；用户选择时清楚说明当前类型。
- [ ] 插件贡献与设置继续采用本项目 plugin.json。更新/停用后入口、发现列表和下一轮工具同步。
- [ ] 模型/服务配置、测试与实际调用分别显示；已有模型池路由只补可见性，不扩展自动回退。

**验收：** 缺宿主工具的 Skill 不进入模型可用列表；真实可用 Skill 能执行；插件停用后新轮次不可调用；Workflow 类型不混淆。

### P08：时间任务

- [ ] 新增 once/daily/weekly 与 IANA timezone；显示下次 UTC 对应当地时间，不混用 cron 与图调度。
- [ ] 定义冻结目标、目录、能力范围、模型选择；启用用户核对的 revision。
- [ ] occurrence 唯一；等待批准期间重复到期记录 skipped_busy；重启错过记录 missed，不自动补跑。
- [ ] 调用既有聊天/任务/Workflow，等待批准回到桌面；不能采用 allow_all 或强制禁用询问后盲执行。
- [ ] 暂停未来触发、取消当前运行、删除定义分别处理；只有真正后台运行才承诺计时有效。

**验收：** 真实后台触发一次；重启不重复；夏令时重复时段只触发一次；批准等待不绕过网关。

### P09：消息渠道

- [ ] 飞书、微信 iLink、QQ/OneBot 各有独立 adapter、配置校验与真实适配清单；不能仅提供同一个通用 URL 表单。
- [ ] 外部账号/会话/消息 ID 与本地会话明确关联；同消息重复投递去重，不按相同文本去重。
- [ ] 工具范围由用户明确选择，不能继承桌面全部能力；同一绑定会话沿用现有串行接收语义。
- [ ] 接收、生成、发送三个状态分别记录；平台接受不等于已读；unknown 不自动重发或转投。
- [ ] 默认不主动外发；用户在正式产品中配置回复范围及授权后才能自动处理范围内消息。
- [ ] 平台协议/SDK/权限在实现时查官方资料；本研究不授权现在配置账号或给联系人发消息。

**验收：** 三种渠道分别取得真实收/发/停用证据；缺账号明确 BLOCKED，不能以受控 fixture 冒充接通。

### P10：语音与个人化

- [ ] 复用 AgentProfile 的姓名、语气、语言、头像与 user_prompt；增加用户明确偏好的来源说明，不创造关系数值。
- [ ] 全局 VoiceHost 与悬浮窗口仍共享 conversational anchor；一次 ASR 只发一轮。
- [ ] 打断播放、取消生成、结束通话三个按钮含义独立；后台页切换不改当前会话归属。
- [ ] 需要补充信息时可说或输入；高风险批准仍采用现有明确批准动作。
- [ ] 提醒偏好、静默时段可关闭；试播来自实际 provider，不能用示例音频冒充连接成功。

**验收：** 真实设备完成说话、回答、打断、续接与结束；主窗/浮窗不重复执行；角色关闭保留用户数据。

### P11：会话与页面体验

- [ ] 搜索正文、标题和消息定位；归档与删除不同；草稿按会话保存。
- [ ] 编辑/重新生成复用已有消息分支，原历史与成果不被覆盖。
- [ ] 页面入口与 capability modal 类型保持当前结构；新增时间/渠道只从更多进入。
- [ ] 主页最多一个辅助面板；正常完成只显示结果，不默认要求“确认交付范围”。
- [ ] 现有 avatar→消息画布行为保留，结果卡新增准确 task/graph/goal 跳转。
- [ ] 800×600、960、1180、1440 宽度下输入、停止、批准可达；深浅主题与用户背景保留。

**验收：** 所有实际/新增页面无死入口；抽屉互斥、焦点可恢复；复制/朗读/来源/打开成果有真实数据目标。

### P12：生命周期与交付

- [ ] 首次配置沿用已有向导，账号可选；保存模型、连接测试、首次成功分别记录。
- [ ] 关闭窗口与退出应用明确区分；不能在实际退出后仍承诺任务继续。
- [ ] 导出含版本清单与文件 hash，排除凭据；恢复用测试副本验证且需用户明确选择目标范围。
- [ ] 更新源必须真实配置，显示签名/校验与版本；没有发布源不伪造“已是最新版”。
- [ ] 构建、安装、启动、退出、重启与旧数据保留均有证据，继续保护私有云仓库边界。

**验收：** 新安装与既有数据升级都可使用；游客无需登录；安装包不依赖开发服务器；退出不遗留无主任务。

## 6. 完成判据与检查命令

每包至少形成输入、持久化、执行、输出/页面、取消/失败五类证据。测试从真实行为断言，不以源码字符串包含某个函数名代替接线成功。

以下是后续开发完成时的命令，本轮未运行产品测试：

```powershell
# backend 工作目录
cargo fmt
cargo fmt --check
cargo check
cargo test

# frontend 工作目录
npm.cmd run test
npm.cmd run build
npm.cmd run test:e2e

# 根目录；使用既有 Windows 脚本
npm.cmd run build:windows
```

当前 test:e2e 默认是 workbench-revision7。新包的交互必须补进实际 E2E runner，再执行；旧 revision7 通过不能代表 P01–P12 通过。新测试建议按附件上下文、轮次/询问、成果、范围、时间/渠道五个领域归档，不新增第二套测试框架。

报告状态使用 PASS/FAIL/BLOCKED/NOT TESTED/HUMAN_PENDING。真实模型、图像编辑、OCR、PPTX 程序、语音与各渠道分别记录环境和实际输入输出。完整交付条件是必选行为全部闭合；只有文档、原型或 fixture 时，必须写明尚未完成产品验收。
