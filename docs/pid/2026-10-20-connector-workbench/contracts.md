# 数据与接口契约

本文件中的“新增”保留设计阶段的接口与数据约定。2026-10-21 已进入实现，实际模块位置和接入范围见 [实现报告](implementation-report.md)。契约条目仍需按实现、路由与验收结果逐项核对，不以类型或接口名称出现在本文中视为已经交付。

## 1. 连接器关系

### 1.1 实体与存储

不建立第二套对话、智能体、模型和能力数据。节点通过 `entity_id` 引用各模块对象，标题和状态从原模块读取。

```ts
type ConnectorNodeKind =
  | "conversation" | "agent" | "constraint"
  | "skill" | "workflow" | "mcp" | "tool" | "model" | "plugin"
  | "memory" | "knowledge";
type ServiceKind =
  | "chat" | "stt" | "tts" | "search" | "vision" | "image" | "embedding";
type BindingKind =
  | "conversation_agent" | "agent_capability"
  | "agent_model" | "agent_memory" | "agent_knowledge";
interface ConnectorNode {
  id: string; kind: ConnectorNodeKind; entity_id: string;
  label: string; x: number; y: number;
  model_source?: "profile" | "pool"; // kind=model 时必填
}
interface ConnectorBinding {
  id: string; kind: BindingKind;
  source_id: string; target_id: string;
  service_kind?: ServiceKind;
  model_source?: "profile" | "pool";
}
interface ConnectorView {
  canvas_id: string;
  name: string;
  agent_id: string;
  nodes: ConnectorNode[];
  bindings: ConnectorBinding[];
  viewport: { x: number; y: number; zoom: number };
  display: ConnectorDisplay;
}
interface ConnectorDisplay {
  mode: "all" | "conversations" | "capabilities" | "custom";
  visible_kinds: ConnectorNodeKind[];
  hidden_node_ids: string[];
  show_edges: boolean;
  show_labels: boolean;
  density: "compact" | "comfortable";
}
```

`source_id`/`target_id` 均为本图节点 ID；节点 ID 使用 `kind:entity_id`。API 将它们解析为实体引用。布局数据仅保存 ID、位置、视口；不复制资源正文、API 密钥或角色提示词。

| 关系 | source → target | 基数/效果 |
|---|---|---|
| conversation_agent | conversation → agent | 每个对话一个当前智能体；替换绑定影响后续回复 |
| agent_capability | agent → skill/workflow/mcp/tool/plugin | 多选；接入运行时可用能力集合 |
| agent_model | agent → model | 按 service_kind 单选；后续调用选择该模型档案 |
| agent_memory | agent → memory | 多选；限定可用记忆集合 |
| agent_knowledge | agent → knowledge | 多选；限定可检索知识集合 |
| 安全约束 | constraint → agent | 由权限规则投影，不另写一条独立权限数据 |

同一类型同一实体同一用途不重复建边。删除关系不删除引用实体。删除资源后图中移除相应关系和布局记录；不以同名对象自动替换。

SQLite 新增 `connector_canvases` 保存画布 ID、名称、所属智能体和显示设置，`connector_layouts` 按 canvas_id 保存位置与视口。一个 agent_id 可有多张画布，不能对 agent_id 建唯一画布约束。绑定优先使用现有实体关联；现有字段无法表达的关系才加入 `connector_bindings`，以实际业务主体关联，不随某张视图画布删除。对话到智能体的绑定必须由对话运行入口读取，不能仅供画布展示。

### 1.2 API（新增）

| 请求 | 输入/返回 |
|---|---|
| `GET /api/connectors?agent_id=…` | 返回画布列表 `{canvases:[{id,name,agent_id,updated_at}]}`；agent_id 为可选筛选 |
| `POST /api/connectors` | `{name,agent_id}`；返回新 ConnectorView |
| `GET /api/connectors/:canvasId` | 返回 ConnectorView；未保存布局按四方向紧凑排布 |
| `PATCH /api/connectors/:canvasId` | `{name}`；重命名 |
| `PUT /api/connectors/:canvasId/layout` | `{nodes: [{id,x,y}], viewport}`；返回保存后的布局 |
| `PUT /api/connectors/:canvasId/display` | ConnectorDisplay；返回保存后的显示配置 |
| `DELETE /api/connectors/:canvasId` | 删除指定画布及其布局/显示设置，返回 204；保留实际业务实体和绑定 |
| `POST /api/connectors/:canvasId/bindings` | `{kind, source_id, target_id, service_kind?, model_source?}`；模型绑定必须显式给出 profile/pool 来源，解析画布主体后修改实际绑定 |
| `DELETE /api/connectors/:canvasId/bindings/:id` | 解除实际绑定；返回 204 |

拖拽结束保存位置，视口操作停止后保存视口。绑定请求完成才显示“已连接”；失败直接显示本次操作错误，不添加假连线。

### 1.3 点击、显示与删除契约

节点编辑统一入口为 `openConnectorEditor({canvas_id,node_id,entity_id,kind})`，按 kind 调用对应模块管理弹窗。保存回调重新读取对象摘要，保留原画布位置；不刷新整张图回默认视口。工具定义不可编辑的字段显示为只读，但其权限和绑定可修改。

mode=conversations 的可见集合为 agent/conversation；mode=capabilities 为 agent/skill/workflow/mcp/tool/model/plugin；custom 使用 visible_kinds；all 为全部已选择类型。中心 agent 始终保留。hidden_node_ids 叠加在类型筛选上。只渲染两端均可见的连线；筛选不调用任何绑定删除 API。

画布列表删除通过列表行携带的 `{kind,id,name}` 发起。确认期间切换活动画布不能改变已选删除目标。删除当前画布时按删除前列表顺序选后一张、否则前一张；删除非当前画布不触碰活动 ID、未保存表单和视口；空列表显示创建入口。批量删除逐条调用目标类型的删除 API，按响应返回每条结果。

### 1.4 原任务画布的完整删除链路

已核对现有 `/api/task-world/graphs/:graph_id` 只有 GET，节点与边已有 DELETE。`api/task_world.rs` 是 re-export，实际实现位于 `modules/task/api/graph_routes.rs`；开发应在那里新增整图删除，不能只隐藏任务中心列表项。

新增 `DELETE /api/task-world/graphs/:graph_id?expected_revision=N`：

- 成功返回 204；目标不存在返回 404；版本改变返回 409 `revision_conflict`；有运行中执行返回 409 `graph_running`。
- 在图级变更锁内阻止新执行进入，确认无活动执行；一次事务删除该 graph_id 的可变图定义、画布视图、专属检查点与执行控制数据。不可按当前选中节点 ID 删除整图。
- 更新 runtime 的 supervisors、harnesses 和相关图缓存，随后发布图删除通知；其他已打开的该图视图进入“画布已删除”，不能再保存或执行。
- 对已完成运行记录按现有审计保留规则处理，保留只读审计所需快照；不删除外部成果文件、对话、Skill、模型和知识资源。实施前列出所有 graph_id 从属表及引用，保证事务与外键一致。
- “停止并删除”先复用已有执行取消机制，等待该图全部活动执行结束，再提交同一目标的删除；取消失败不继续删除。版本改变时展示新状态，用户重新确认目标。
- 重启后列表及 GET 均不能恢复该图。删除不通过后台恢复或保存旧快照重新创建对象。

新增前端 `deleteTaskGraph(graphId, expectedRevision)`；“历史任务画布”列表复用现有 `listTaskGraphs()`。这是用户明确提出的删除缺陷修复，不把历史图转换为连接器，也不恢复旧任务页面。

## 2. 三态权限

复用 `PermissionId`、`ResourceScope`、`PolicyDecision` 和现有授权存储；不得创造第二套绕过执行网关的前端权限。

```ts
type AccessMode = "full" | "approve" | "deny";
interface AccessRule {
  id: string;
  scope: "default" | "agent" | "capability" | "resource";
  subject_id: string | null;
  permission_id: string;
  resource: Record<string, unknown> | null;
  mode: AccessMode;
}
interface AccessResolution {
  mode: AccessMode;
  matched_rule_ids: string[];
  effective_rule_id: string;
  reason: string;
}
```

`permission_id` 必须来自现有 PermissionId 的序列化值；`resource` 使用已有资源授权结构。新增三态规则存储是现有 grant 模型的扩展，实施时将 Allow/Deny 与 RequireApproval 统一到同一解析入口。

决策顺序：

1. 解析工具描述、主体、实际参数和资源对象。
2. 找出作用于本次请求的规则。
3. 任意匹配 `deny` 得到 Deny。
4. 没有 deny 时，按资源 > 能力/工具 > 智能体 > 默认取最具体规则；同级 approve 优先。
5. full → Allow；approve → RequireApproval；deny → Deny。
6. 没有用户规则时复用项目已有默认风险判定。
7. 审批同意只放行匹配的那次调用；执行前再次读取是否已被禁止。

对于明确保存的 full 授权，现有 `High/Critical → RequireApproval` 条件需要识别持续授权，否则无法实现 PID 的“完全权限”。风险级别仍记录在审计中；未获持续授权的高风险调用保持现有审批。

新增 `GET/PUT /api/security/access-rules` 与 `POST /api/security/access-rules/resolve`。后者用于界面显示有效规则，实际执行仍由 gateway 独立计算。PUT 输入 `{rules: AccessRule[]}`，一次事务保存；返回规范化规则。resolve 输入 `{subject_id,tool_name,arguments}`，返回 AccessResolution，不执行工具。

权限规则和资源授权的升级在同一数据库迁移中完成，旧明确 Allow/Deny 保持原语义。不能直接把旧 `yolo` 值转换成对所有资源的完全权限；用户需要在新界面明确设置范围。该迁移不新增长期兼容执行分支。

## 3. 统一基础配置

### 3.1 厂商与服务档案

```ts
interface ProviderPreset {
  id: string; label: string; official_docs_url: string;
  services: ServiceKind[];
  regions: Array<{ id: string; label: string; base_url: string }>;
  auth_fields: Array<{ key: string; label: string; secret: boolean; required: boolean }>;
  adapter_by_service: Partial<Record<ServiceKind, string>>;
}
interface ProviderAccount {
  id: string; provider_id: string; label: string;
  region_id: string; base_url: string; credential_id: string | null;
}
interface ServiceProfile {
  id: string; label: string;
  provider_account_id: string;
  provider_id: string; service_kind: ServiceKind; region_id: string;
  base_url: string; model: string;
  credential_id: string | null;
  parameters: Record<string, string | number | boolean>;
  revision: number;
  test: null | { revision: number; ok: boolean; tested_at: string; message: string };
}
interface ModelPool {
  id: string; label: string; service_kind: "chat" | "vision" | "image";
  enabled: boolean; revision: number;
  attempt_timeout_ms: number; total_timeout_ms: number;
  tiers: Array<{ id: string; label: string; profile_ids: string[] }>;
}
```

厂商预设、已配置账户、模型档案和模型池为四种对象。账户管理凭据与默认地址；ServiceProfile 的账户、provider、区域、地址、credential_id 由服务端一致投影，写入时不得同时提交相互矛盾的账户属性。模型池只引用档案 ID。现有 `LlmModelPayload.api_format: "openai"` 不能承载所有服务协议；调用仍由对应 adapter 执行。

密钥统一经现有 secret store 保存，公开档案返回 credential_id 与是否已配置；不返回密钥值。更新凭据区分“保留”“替换”“清除”，空输入默认保留。

新增 API：

| 请求 | 行为 |
|---|---|
| `GET /api/provider-presets` | 返回已注册预设与实际实现的服务类型 |
| `GET/POST /api/provider-accounts` | 查询/添加厂商账户；账户之间独立保存 |
| `PUT/DELETE /api/provider-accounts/:id` | 修改/删除账户；删除先明确处理被引用的档案和池 |
| `GET/POST /api/service-profiles` | 查询/创建服务档案 |
| `PUT/DELETE /api/service-profiles/:id` | 更新/删除档案 |
| `POST /api/service-profiles/:id/test` | 执行所选服务最小请求，保存所属 revision 的结果 |
| `GET/POST /api/model-pools` | 查询/创建模型池 |
| `PUT/DELETE /api/model-pools/:id` | 按 expected_revision 更新层级/候选或删除池；实际服务档案不随池删除 |
| `PUT /api/service-defaults/:kind` | `{source:"profile"|"pool",id}` 设置默认来源；pool 仅用于对应模型用途 |

厂商账户创建/更新接收 `credential_action: keep | replace | clear` 与 `credential_fields`，写入密钥存储后不回显后者。ServiceProfile 创建/更新仅接收 provider_account_id、模型用途、模型 ID 和参数等可编辑字段，不重复写账户凭据。`revision`、`test` 由服务端产生。

已有模型/语音/搜索配置一次性归入账户和档案，保留其真实地址、模型和凭据引用；旧默认模型先保留为固定档案，用户建立池并设为默认后才启用池。不得自动添加未选择的厂商。调用路径完成切换后删除旧编辑入口，不双写两套配置。

数组顺序定义层级及同层候选顺序，服务端返回的 revision 固定本轮调度快照。档案用途、输入模态、工具调用/结构化输出能力必须满足本次请求，选择器只允许相应候选。可用性失败触发的顺序切换、流式输出处理、工具结果保留、取消与用户提示遵循 [模型池细则](model-pools.md)。

### 3.2 测试内容

对话：短回复；STT：固定短音频识别；TTS：短文本生成可播放音频；搜索：指定问题获取来源；识图：上传图像返回内容描述；生图：生成并显示图像；Embedding：返回向量并验证维度。测试视图显示请求用途和结果，敏感信息不进入日志。

## 4. 通话状态与音频所有权

继续使用 `GlobalVoiceSession.voice_session_id`、generation、输入 lease、conversation anchor 和现有 `/api/voice/*` 生命周期。新增连接阶段与静音字段，不另造第二个音频会话系统。

```ts
type CallPhase = "idle" | "connecting" | "listening" | "thinking"
  | "speaking" | "muted" | "ended" | "error";
interface CallPresentation {
  session_id: string | null;
  phase: CallPhase;
  conversation_id: string | null;
  muted: boolean;
  speaker_enabled: boolean;
  connected_at: string | null;
  caption: string;
}
```

CallPresentation 是界面投影；`thinking` 映射运行时 processing，`speaking/listening` 映射同名状态。connecting 的检查位包括麦克风、会话、STT、TTS、输出设备。每项由真实完成事件更新。

一个通话 host 负责采集、VAD/断句、上传或流式 STT、LLM 续答、TTS 和播放。Tauri 浮窗只发送 start/mute/unmute/end/expand/collapse 等控制命令并订阅状态；禁止主窗口、浮窗同时开启 MediaRecorder。

产品默认采用已选择的 STT → LLM → TTS 管线实现连续交谈。LLM 绑定模型池时执行明确配置的自动降级并发送提示；STT/TTS 保持所选服务。需要流式识别/合成时直接补齐对应 provider。VAD 负责断句和插话，AEC 负责回声处理。将端点检测、首次音频时间、插话停止时间写入开发验收记录。

状态关键路径：

- start：先显示 connecting；依次完成会话和设备/服务建立；成功进入 listening。
- utterance_end：本轮输入提交；thinking；生成音频开始 speaking；播放结束回 listening。
- user_speech_start during speaking：停止旧播放并取消旧续答，切换到本轮输入。
- mute：暂停向识别服务发送麦克风音频；保留会话和输出。
- end：取消采集、识别、续答、播放并调用现有 stop/cancel；所有窗口进入 ended。

已有 generation/lease/cancel 机制直接复用。通话中不增加逐轮确认按钮。

## 5. 导入与发现

```ts
type ImportKind = "skill" | "workflow" | "mcp" | "plugin";
interface ImportSource {
  kind: ImportKind;
  source_type: "directory" | "archive" | "git" | "config_file";
  location: string;
}
interface ImportPreview {
  import_id: string; source: ImportSource;
  name: string; description: string; install_path: string;
  files: string[]; required_actions: string[];
  existing_id: string | null;
}
interface LocalServiceCandidate {
  id: string; origin: "config" | "listener";
  address: string | null; command: string | null;
  protocol: "mcp_stdio" | "mcp_http" | "openai" | "ollama" | "unknown";
  name: string;
}
```

新增：

- `POST /api/capability-imports/preview`：输入 ImportSource，读取/下载内容并返回 ImportPreview。
- `POST /api/capability-imports/:id/install`：输入 `{action:"install"|"update"}`，保存到用户指定能力目录，返回 `{kind,entity_id}`。
- `POST /api/local-services/discover`：输入 `{directories:string[],read_known_configs:boolean,inspect_loopback_listeners:boolean}`，返回 `{discovery_id}`。
- `GET /api/local-services/discoveries/:id`：返回 `{status:"running"|"completed"|"failed",candidates:LocalServiceCandidate[],message}`。
- `POST /api/local-services/discoveries/:id/import`：输入 `{candidate_ids:string[]}`，返回每项导入结果；模型服务进入基础配置，MCP 进入能力。

读取候选和安装/启动服务分开。扫描不执行候选中的任意启动命令；实际启动按已配置权限运行。ZIP 文件安装到选定目录，保留相对资源引用；禁止把文件写到安装目录之外。这属于导入正确性要求，不增加额外审批系统。

导入完成后重新读取已安装对象，执行一次实际 Skill/工作流/MCP 调用或插件功能以验收。只有读取清单不能作为完成标准。

## 6. 角色与提示词

智能体公开 DTO 增加 `user_prompt`、`persona_enabled`、`persona`；persona 包含 `{display_name,description,tone,language,avatar_resource_id}`。官方项目提示词留在服务端执行模板，不包含在列表或详情响应中。

已有 `instructions` 必须按来源归属处理：用户创建/导入的内容转为 user_prompt；项目内置内容留在内部模板。字段切换更新当前全部 UI 消费者和 API 文档，避免同时返回旧 instructions 泄露项目模板。

新增/修改智能体请求只接收用户可编辑字段。执行时读取同一个角色档案，文字与通话共享。角色修改从下一轮开始；正在播放的回答不被新角色内容重写。

## 7. 用量

现有 `LlmUsageReport` 包含 prompt_tokens、completion_tokens、total_tokens、days，现有查询支持 model_id/from/to。首先接入这些已有字段，再扩展查询与统计所需字段。

本版新增 `request_count`、`unknown_usage_requests`、`timezone`、`by_model`；按日聚合使用指定时区边界。每次厂商请求记录一个请求 ID，最终 usage 更新同一条记录；不能逐 chunk 累加累计 total_tokens。

`total_tokens` 优先采用厂商最终值；厂商仅提供输入和输出时按明确协议计算。缺失用量保持未知计数。不同服务的音频秒数、图像张数不强行换算为 Token。无需新增费用显示，本版不假设固定价格。

监控和浮窗共享 `getLlmUsage` 查询与现有系统指标数据源。默认监控可见时每 5 秒刷新用量，系统指标复用现有刷新周期；关闭两个监控视图后停止新增的用量轮询。

## 8. 浮窗容器

新增 UI 契约 `FloatingSurfaceState = {kind:"call"|"monitor",expanded:boolean,position:{x,y},always_on_top:boolean}`。Tauri 窗口标签分别为 `call-overlay`、`monitor-overlay`。

窗口位置按屏幕工作区保存，展开和收起使用相同窗口标签。外部桌面浮窗共享主题消息；字体、颜色、表面透明度、减少动效设置随主窗口更新。主窗口最小化不销毁会话或监控，整个应用退出时关闭浮窗。

## 9. 弹窗和主题

外壳复用 `NavRail`、`AppShell` 和 `PageShell`。主栏为 80px（桌面）/68px（窄窗口），图标与标签上下排列。“更多”导航到 `/more` 页面，二级入口继续使用现有注册与偏好数据，但以卡片网格显示，不再使用弹层。更多页和所属二级页主栏均高亮更多。底部安全/设置/状态区域不改成带文字的系统菜单。

能力首页六张卡片使用三列两行网格；标题与整个网格相对内容区居中。更多页使用同一居中网格，尾行按从左向右排列。默认没有打开的管理弹窗。点击类别后打开现有同类管理界面，第六张卡打开导入选择弹窗。

## 10. 插件新增功能入口

当前 `backend/src/plugin/manifest.rs` 的贡献只有 agents、skills、workflows 和 mcp_server_templates；当前 `frontend/src/api/plugins.ts` 也没有 UI 功能条目。本节定义新增契约，不将其写成既有字段。

在现有声明式 manifest 增加 `contributes.features`，每项为 `{id,title,description,icon,target:{kind:"agent"|"skill"|"workflow"|"mcp",contribution_id}}`，引用该插件已安装的贡献。宿主生成功能页或对应操作表单，不借此引入任意原生代码或远程脚本执行。

新增 `GET /api/plugin-features` 返回 `{plugin_id,plugin_name,feature_id,title,description,icon,route,enabled}`，route 由宿主生成 `/more/plugins/:pluginId/:featureId`。更多页从同一响应生成入口，卡片注明所属插件；同名功能按 plugin_id/feature_id 区分。

插件启用并完成导入后其 features 生效；升级更新已有键，停用/卸载后入口移除，打开的功能页提示不可用并可返回更多。没有 features 的插件仅保留在能力管理中。功能动作复用所属能力执行流程和权限规则。

页头沿用标题、说明和操作同排的桌面布局，不增加独立面包屑栏。预览图引用 `frontend/public/favicon.png` 和 `cyrene-home-background.png` 的副本；实现直接复用项目现有资源。

管理弹窗使用同一组件契约：标题、分类导航、可滚动正文、固定操作栏、返回/关闭。打开时聚焦标题或第一个输入，关闭后返回原触发按钮；Tab 限制在当前弹窗。纯文本“查看更多”按钮本身为可聚焦 button。

新增 `--surface-alpha`、`--surface-elevated-alpha`、`--surface-blur`，在现有主题生成函数中与当前色彩方案合成。页面背景透明度和文字颜色分别计算。用户透明度建议范围 0.35～1.00；阅读/配置层最低 0.88，保证文字与输入框可读。
