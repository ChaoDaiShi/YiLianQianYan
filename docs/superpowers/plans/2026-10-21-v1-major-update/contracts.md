# v1 重大更新开发契约

返回 [主开发文档](../2026-10-21-v1-major-update.md)。

R2 新增产品契约见本文第 10 节，对应 M12–M17；前述 M01–M11 继续有效。新增能力与新版预览均未实现到产品中。

## 1. 契约性质与公共规则

本文中的 `RunSnapshot`、`RunContextSnapshot` 等新类型、新字段、新 API、新表均为**拟新增规格**。现有结构必须以源码为准。实施时前后端同时更新，禁止仅在 UI 中假装这些字段已经存在。

以下约定适用于本次新增字段：

- API 字段使用 snake_case；已有 camelCase execution_history DTO 保持原序列化规则，不全局重命名。
- ID 为服务端生成的不透明字符串；前端不拼接业务 ID，不把文件路径当作资源 ID。
- 时间统一为 UTC 毫秒；用户计划时间另外保存 IANA timezone，界面按任务时区展示。
- 字符范围采用 Unicode scalar 的零基、左闭右开区间；前端不能直接用 UTF-16 string.slice 解释此范围。
- 新错误采用 `{code,message,run_id?}`；旧 API 保留现有响应，不因新增能力建立通用兼容代理。
- HTTP 400 表示请求不合法，404 表示实体不存在，409 表示状态/版本冲突，422 表示条件不满足，500 表示真实内部失败。
- 保留本地控制会话认证、SecretStore、取消及风险审批。新增接口并入现有 protected router，密钥内容不进入 DTO。
- 未识别的未来 SSE 事件不得被解释为 token/done；仍按现有未知事件处理规则处理。

## 2. 运行身份与数据归属

### 2.1 归属表

| 对象 | 唯一写入责任 | 其他模块如何使用 |
|---|---|---|
| 会话正文、assistant/tool 协议消息 | 现有 db/conversations 与 chat lifecycle | 按消息 ID 或合法协议前缀引用 |
| 工具执行与验证 | SecurityExecutionGateway、现有 execution_history | 关联 run_id/tool_call_id，不能由 UI 修改成功状态 |
| 会话每轮运行 | 拟新增 db/chat_runs，由 chat lifecycle 统一写入 | 画布、语音、日志、定时任务读取投影 |
| TaskGraph 执行 | 现有 Task runtime/harness | 聊天运行通过 task/graph/goal 关联，不复制其状态机 |
| Workflow 定义与执行 | 现有 workflow 模块 | 调度器启动现有运行服务，不直接解释节点 |
| 目标合同与验收 | 现有 ProofLoop | 以 goal_id + goal_version 查询 |
| 文档正文 | knowledge_documents + Resource | 派生片段、引用和学习记录引用正文版本 |
| 画布坐标、选中项 | connector layout/settings canvas | 仅控制显示，不能授予权限或触发执行 |
| 定时触发 | 拟新增 automation service | 记录一次触发后调用既有任务入口 |

### 2.2 拟新增类型

```ts
type Scenario = "general" | "code" | "learn";
type WorkspaceMode = "chat" | "work" | "create";
type ChatRunStatus =
  | "running" | "waiting_approval" | "waiting_input"
  | "completed" | "failed" | "cancelled" | "interrupted";

type KnowledgeScope =
  | { kind: "all" }
  | { kind: "none" }
  | { kind: "documents"; document_ids: string[] }
  | { kind: "collection"; collection_id: string };

interface RunContextSnapshot {
  workspace_mode: WorkspaceMode;
  scenario: Scenario;
  workspace_id: string | null;
  workspace_root: string | null;
  agent_id: string | null;
  model_source: { kind: "profile" | "pool" | "configured"; id: string | null };
  thinking_mode: string; // 值域复用现有 ThinkingMode，不创建另一套枚举
  capability_ids: string[];
  knowledge_scope: KnowledgeScope;
  knowledge_versions: Array<{ document_id: string; resource_id: string; hash: string }>;
  memory_scope: { include_global: boolean; workspace_id: string | null };
  binding_revision: number;
  captured_at: number;
}

interface RunSnapshot {
  run_id: string;
  conversation_id: string;
  user_message_id: string;
  status: ChatRunStatus;
  revision: number;
  last_event_sequence: number;
  context: RunContextSnapshot;
  started_at: number;
  finished_at: number | null;
  active_ms: number;
  user_wait_ms: number;
  pending_approval_id: string | null;
  pending_question_id: string | null;
  task_ref: { task_id: string | null; graph_id: string | null } | null;
  goal_ref: { goal_id: string; goal_version: number } | null;
  error: { code: string; message: string } | null;
}
```

`configured` 专指现有配置模型来源，并不意味着新增备用模型。新快照要记录模型池配置版本及实际路由结果；不能把整个池中的每个候选都标为本轮已调用。

run_id 属于一次用户轮次。用户新发一条消息才建立新 run；同一轮的批准、回答问题、流连接恢复均不建立新 run。同一会话仅允许一个非终态 run；新输入冲突返回 409，由 UI 显示当前运行，不能覆盖其状态。

### 2.3 状态流转

| 当前状态 | 触发 | 目标状态 | 必须发生的持久化 |
|---|---|---|---|
| 无活动轮次 | 用户发送 | running | 用户消息、上下文快照、run_id |
| running | 提交高风险动作 | waiting_approval | pending approval 和原工具调用 |
| waiting_approval | 用户批准 | running | 审批决议、有效权限重新检查 |
| waiting_approval | 用户拒绝 | running 或 cancelled | 拒绝结果写入原工具协议；仅当现有引擎允许时继续解释/改计划，不能执行被拒绝动作 |
| running | ask_user | waiting_input | 问题、关联 tool_call_id、挂起边界 |
| waiting_input | 用户答复 | running | 答案、已解决问题、工具返回结果 |
| running | 本轮生成与持久化结束 | completed | 最终消息、终态事件、finished_at |
| 任意非终态 | 用户停止 | cancelled | 取消标记、未完成调用的实际状态 |
| running | 模型/持久化/执行失败，无法继续 | failed | 具体错误与已有结果 |
| 非终态 | 进程中断后启动检查 | interrupted | 最后可靠边界与未结算调用 |
| interrupted | 用户显式继续 | running | 同 run 的 continuation 序号；先确认不重放未知副作用 |

终态不得被迟到的 SSE 增量改回 running。执行 timeout 使用 failed + error.code=`execution_timeout`，不新增含义重复的全局状态。结果未知保留在工具执行结果中，不表示整个目标成功。

现有 ConversationRunStatus 增加 WaitingInput，前后端状态显示一并更新。Idle 继续是尚无本轮运行的会话状态，不写入 ChatRunStatus。

## 3. 上下文解析与设置生效

### 3.1 初始轮次解析顺序

1. 确认 conversation_id 与用户消息；新会话先持久化得到真实 ID。
2. 读取当前会话绑定的 agent、工作空间、资料范围与场景。
3. 应用本次用户显式选择的模型和 thinking；保留现有 ChatRequestOptions 的覆盖语义。
4. 模型来源沿用 `resolve_model_source`：会话所属智能体的显式模型绑定、默认模型池、当前激活/已配置模型。若显式绑定实体不存在，返回错误，不能自动选另一个模型。
5. 计算当前实际可用 capability_ids，并用既有权限服务检查；列表发现成功不等于所有工具已获准执行。
6. 读取范围内知识与记忆，记录来源版本，生成本轮快照。
7. 用快照构建 AgentState 并启动现有 engine。

没有新增选择的旧会话保持原行为；迁移时将既有全库 grounding 标为 scope=all，将受绑定限制的会话转成相应显式 scope。不能将“空数组”解释成“所有资料”。

显式资料选择始终与当前智能体/会话已有的绑定限制取交集；scope=all 只表示授权范围内全部资料，不能取消 restricted 路径。集合含越权文档或选择为空时返回明确条件错误，不悄悄扩大范围。

### 3.2 审批、分支与设置变更

- 续接使用原快照和已保存的模型选项，不重新读取当前 UI 的选择。
- 权限撤销、能力停用和实体删除仍以执行时现状为准。发现失效就显示具体阻塞原因。
- 新分支继承已完成祖先轮次与分支时的设置，取得新的 conversation_id；不继承一个仍在等待审批的可执行授权。
- 若选中节点不构成完整 assistant/tool 协议边界，分支操作返回明确错误，UI 提示选择已完成回答。
- 拖动画布、折叠节点、隐藏设置节点不能改变模型绑定或撤销运行能力。
- 修改工作空间目录影响下一轮；当前运行不能在一次工具执行期间切换 cwd。

## 4. HTTP 与事件增量

### 4.1 保留的现有入口

`POST /api/chat`、`POST /api/chat/stop`、`GET /api/events`、会话 CRUD、审批 approve/reject/cancel、`/api/connectors`、`/api/message-canvas`、`/api/knowledge/documents`、`/api/knowledge/search`、`/api/workspace-git`、`/api/workspace-git/diff`、现有 Task/ProofLoop/Workflow API 全部保留。

现有 POST /api/chat 以扁平请求传入 `workspace_mode/model_profile_id/thinking_mode`，不要包成新的 options 对象。拟追加可选 `scenario/workspace_id/knowledge_scope`；传入时校验并持久化会话偏好，未传入时读取原偏好。

### 4.2 拟新增接口

| 方法与路径 | 输入 | 输出及副作用 |
|---|---|---|
| GET `/api/conversations/:id/runs` | `before?`、`limit` 1–100 | `{runs: RunSnapshot[], next_cursor: string|null}`，按开始时间倒序 |
| GET `/api/chat/runs/:run` | 无 | RunSnapshot；不调用模型 |
| GET `/api/chat/runs/:run/events` | `after_sequence`，默认 0 | `{events: [], last_sequence, has_more}`，最多 200 个已持久化事件 |
| POST `/api/chat/runs/:run/resume` | `{expected_revision}` | 202 `{run_id,status}`；仅允许 interrupted，启动前检查未结算副作用 |
| GET `/api/chat/runs/:run/questions/:question` | 无 | UserQuestion；用于刷新后恢复待回答卡，不执行模型 |
| POST `/api/chat/runs/:run/questions/:question/answer` | `{expected_revision,selected_option_id?,text?}` | 202 `{run_id,status}`；在同一事务解决问题并取得续接权 |
| GET `/api/chat/runs/:run/context` | 无 | token 分类、摘要边界、资料/记忆来源、估算标识；不返回密钥 |
| GET `/api/chat/runs/:run/artifacts` | 无 | `{artifacts: []}`；读取既有 Resource/Artifact 元数据 |
| GET `/api/knowledge/collections` | 无 | `{collections: []}` |
| POST `/api/knowledge/collections` | `{name,document_ids}` | 201 集合；不复制文档正文 |
| PUT `/api/knowledge/collections/:id` | `{expected_revision,name,document_ids}` | 更新后集合；409 拒绝过期编辑 |
| DELETE `/api/knowledge/collections/:id` | 无 | 204；仅删除集合关系，不删除文档 |
| POST `/api/knowledge/documents/:id/versions` | `{resource_id,expected_revision}` | 201 新版本；解析成功后才切换当前正文和派生片段，历史 Resource 引用不变 |
| GET `/api/knowledge/documents/:id/references/:ref` | 无 | Citation；来源删除时返回 source_status=deleted，引用本身不存在才返回 404 |
| GET `/api/memories/:id/lifecycle` | 无 | MemoryLifecycle |
| PUT `/api/memories/:id/lifecycle` | `{expected_revision,status,scope}` | 更新后对象；知识派生片段不支持此写入 |
| POST `/api/memories/:id/replace` | `{expected_revision,content,reason}` | 201 `{previous_id,current_id}`；原记忆变 superseded |
| GET `/api/workspaces/:id/files` | `path` 相对目录 | `{entries: [{path,kind,size}]}`；不接受跨根路径 |
| POST `/api/workspaces/:id/code/query` | `{method,path,line,character}` | `{locations,diagnostics,server_status}`；method=definition/references/diagnostics，位置坐标遵循 LSP UTF-16 |
| POST `/api/workspaces/:id/code/verify` | `{command,args,conversation_id}` | 202 `{task_id}`；经既有网关执行，结果查既有任务 |
| POST `/api/workspace-git/actions` | `{workspace_id,action,paths?,message?,remote?,branch?}` | `{status,approval_id?,operation_id}`；action=stage/unstage/commit/push，结果可从日志和刷新 Git 状态核对 |
| POST `/api/learning/sessions` | `{workspace_id,collection_id,conversation_id}` | 201 LearningSession |
| GET `/api/learning/sessions/:id` | 无 | 会话、笔记、练习列表和真实学习统计 |
| POST `/api/learning/sessions/:id/notes` | `{title,content,reference_ids}` | 201 `{note_id,resource_id,revision}`；保存用户编辑或已生成的 Markdown |
| PUT `/api/learning/notes/:id` | `{expected_revision,title,content}` | 新 Resource 与新 revision；不覆盖旧版本 |
| POST `/api/learning/sessions/:id/exercises` | `{count,question_types}` | 202 `{run_id}`；当前学习 conversation_id 必须空闲，生成结果在会话详情中读取 |
| POST `/api/learning/exercises/:id/attempts` | `{answer,expected_revision}` | 201 `{attempt_id,status,result?}`；开放题可 status=grading，结果通过下方 GET 查询 |
| GET `/api/learning/attempts/:id` | 无 | `{attempt_id,status,result,error}`；未判完不得展示最终分数 |
| GET `/api/automations` | `workspace_id?` | `{automations: []}` |
| POST `/api/automations` | AutomationDraft | 201 Automation，enabled=false |
| PUT `/api/automations/:id` | `{expected_revision,...AutomationDraft}` | 新版本；执行内容变更后 enabled=false |
| POST `/api/automations/:id/enable` | `{expected_revision}` | 当前版本 enabled=true，计算 next_run_at |
| POST `/api/automations/:id/pause` | `{expected_revision}` | 停止未来触发，不取消在途任务 |
| POST `/api/automations/:id/run` | `{expected_revision}` | 202 occurrence；显式“现在运行” |
| GET `/api/automations/:id/occurrences` | `before?`、`limit` 1–100 | 分页触发记录，包含关联任务/运行 ID |
| DELETE `/api/automations/:id` | 无 | 204；无在途运行时删除定义，保留触发审计；在途返回 409 |

上述接口均须真正注册到 `backend/src/app/router.rs`，并在对应 frontend/src/api 模块提供类型化调用。新增 API 返回的 202 只代表已受理，UI 必须查询状态或订阅已有事件确认完成。

问题 answer 请求中的 expected_revision 是 UserQuestion.revision；resume 使用 RunSnapshot.revision；文档版本使用该文档当前 revision。不要将不同对象的 revision 混用。学习、知识、记忆和自动化列表应按用户实际规模实现分页，不能在界面显示伪造的全量计数。

### 4.3 SSE 扩展

现有 `token/tool_start/tool_end/approval_required/done/error/reasoning/reasoning_snapshot/model_route/workspace_mode` 保留原有格式。拟追加顶层 `run_id`、`sequence`；不能把旧事件整体嵌入 payload 后要求所有旧消费者重写。

新增事件采用 `type,conversation_id,run_id,sequence,data`：

| type | data | 持久化条件 |
|---|---|---|
| `run_started` | RunSnapshot | run 与用户消息已保存后 |
| `run_state_changed` | `{status,revision,error}` | 状态事务提交后 |
| `user_input_required` | UserQuestion | 问题和挂起边界已保存后 |
| `context_usage` | ContextUsage | 请求前或终态时；estimated 标识明确 |
| `context_compacted` | `{checkpoint_id,through_message_id,summary_resource_id}` | 摘要及边界保存后 |
| `source_references` | `{message_id,references: Citation[]}` | 只登记已验证属于本轮范围的引用 |
| `artifact_registered` | `{artifact_id?,resource_id,name,verification_status}` | 实际文件和元数据确认后 |

低频运行变化通过已有 YiEvent/Product Event 发布关联 ID；聊天 token 仍走原 chat SSE，不将所有 token 塞进全局事件总线。

新事件顺序由后端分配。正文增量不要求逐 token 写 SQLite：持久化完成消息和最终 reasoning snapshot，重开页面先读取 conversation/run snapshot，再接收新事件。事件分页缺少的增量通过完整消息快照恢复显示，不通过重新调用模型补发。

## 5. 询问、压缩和工具输出

### 5.1 UserQuestion

```ts
interface UserQuestion {
  question_id: string;
  run_id: string;
  tool_call_id: string;
  title: string;
  kind: "text" | "single_choice";
  options: Array<{ id: string; label: string }>;
  allow_text: boolean;
  status: "pending" | "answered" | "cancelled";
  revision: number;
}
interface ContextUsage {
  input_tokens: number;
  reserved_output_tokens: number;
  context_window: number;
  estimated: boolean;
  parts: { system: number; tools: number; knowledge: number; memory: number; history: number };
  checkpoint_id: string | null;
}
```

单选题必须有 2–6 个 ID 不重复的选项；text 题 options 为空。空回答、越界选项或已解决问题返回 400/409。用户答案是输入数据，不能视为权限批准。

模型同轮提出 ask_user 和其他调用时，ask_user 成为该轮唯一等待动作，尚未执行的其他调用写入明确 not_executed 工具结果，之后再让模型基于答案重新决定；不得留下无法配对的 tool_call。

### 5.2 压缩规则

触发条件：`estimated_input > context_window * trigger_threshold`，且配置 enabled。输出预留取当前模型请求 max_tokens，必须满足 `input + reserved_output <= context_window`。

摘要保留：用户目标、明确限制、已作决定、工作空间、验证过的成果、未完成事项、未答问题、关键引用 ID。最近消息按完整轮次保留，不能从 assistant 工具调用与 tool 结果之间截断。

checkpoint 包含 run_id、through_message_id、summary_resource_id、source_message_ids、model_profile_id、created_at、input_estimate、output_estimate。已有摘要必须作为下一次压缩输入，避免连续压缩遗忘早期约束。

写入摘要与 checkpoint 后才更新上下文指针；失败不覆盖旧指针，不删除原始消息。新分支只能引用其祖先边界内的摘要；不能借用包含另一个分支内容的摘要。

若压缩后仍无法满足输入加输出预留预算，返回 `context_budget_exceeded`，展示占用构成并等待用户减少资料或调整配置；本次不增加再次压缩、换模型或删消息的自动兜底循环。配置关闭压缩时也必须遵守实际模型窗口，不能发送明知超限的请求后无限重试。

### 5.3 read_tool_result

拟新增工具输入：`{output_ref,offset_chars,limit_chars}`；offset ≥ 0，limit 1–12000。

输出：`{text,next_offset_chars,total_chars,truncated}`。output_ref 对应本轮或合法祖先轮次实际创建的 Resource；不接受任意本地路径。文本分段按 Unicode scalar 计算，工具长输出原文件仍可供用户下载。

## 6. 知识、记忆与学习数据

```ts
interface Citation {
  reference_id: string;
  document_id: string;
  resource_id: string;
  content_hash: string;
  document_name: string;
  start_char: number;
  end_char: number;
  quote: string;
  source_status: "available" | "deleted";
}
interface MemoryLifecycle {
  memory_id: string;
  status: "active" | "archived" | "superseded";
  scope: { kind: "global" } | { kind: "workspace"; workspace_id: string };
  source_message_ids: string[];
  confirmation_kind: "legacy" | "manual" | "existing_extraction";
  change_reason: string | null;
  replaces_id: string | null;
  replaced_by_id: string | null;
  last_used_at: number | null;
  revision: number;
}
interface LearningSession {
  id: string;
  workspace_id: string;
  collection_id: string;
  conversation_id: string;
  note_resource_ids: string[];
  created_at: number;
}
```

引用的 quote 必须与目标 hash 对应正文区间一致；模型生成的编号只能匹配本轮已注入的来源表。未知编号显示无可核对来源，不能凭近似标题自动配对。

删除来源后保留引用 ID 与 deleted 标记供历史回答定位；隐私删除同时清除该引用中存储的原文 quote。此时不能提供原文件下载或声称仍可校验原文。

知识集合只持有 document_id，运行快照展开为具体版本。运行开始后的资料更新不改变本轮已用来源；用户新发一轮再使用新版本。

记忆生命周期存于附属表；历史 Memory 的 ID 和 API 不变。首次迁移以 active 保留原有可检索性，不虚构 source_message_ids、last_used_at 或确认记录。知识派生项继续由文档管理。

练习表应保存 question、question_type、reference_ids、answer_key、rubric、revision。提交前的读取 DTO 排除 answer_key/rubric 中的答案信息；提交后通过 attempt 返回解析。不能把隐藏答案放入前端状态再只用 CSS 隐藏。

笔记内容是 Resource；编辑产生新资源版本和关联。将笔记导出到工作空间使用现有写入工具和权限网关，不把浏览器下载成功当作文件已写入用户选定目录。

练习生成结果必须按既定字段解析并校验后写入 learning_exercises；生成失败保存真实错误，不通过任意正则猜测题目和答案。若使用供应商结构化输出，能力必须已核验；不支持的模型明确不可用于该操作，不新增协议降级。

## 7. 定时任务契约

```ts
type ScheduleRule =
  | { kind: "once"; run_at: number; timezone: string }
  | { kind: "daily"; local_time: string; timezone: string }
  | { kind: "weekly"; weekdays: number[]; local_time: string; timezone: string };
// local_time=HH:mm；weekdays 使用 ISO 1=周一、7=周日。

interface AutomationDraft {
  name: string;
  workspace_id: string;
  schedule: ScheduleRule;
  target:
    | { kind: "conversation"; conversation_id: string; prompt: string }
    | { kind: "workflow"; workflow_id: string; input: Record<string, unknown> }
    | { kind: "goal"; graph_id: string; goal_version: number };
  notification: { desktop: boolean; conversation_id: string };
}
interface Automation extends AutomationDraft {
  id: string;
  revision: number;
  enabled: boolean;
  next_run_at: number | null;
  last_occurrence_id: string | null;
}
```

每个触发记录持有 automation_id、definition_revision、scheduled_at、trigger_kind、status、run_id/task_id、started_at、finished_at、error。status=`queued|running|waiting_approval|waiting_input|completed|failed|cancelled|missed|skipped_busy`。

规则：

1. 定时触发用 `(automation_id,definition_revision,scheduled_at)` 唯一键。手动触发另用服务端 occurrence_id，不覆盖定时记录。
2. 一个任务实例仍在执行或等待用户时，后续到期记录为 skipped_busy，不自动并发启动另一份，也不无限排队。
3. daily/weekly 先按 IANA 时区计算当地时间，再转换 UTC。夏令时不存在的时间记 missed；重复的当地时间只选第一次 UTC occurrence，并在预览中显示 UTC 偏移。
4. 应用运行中的正常计时误差，以同一次 occurrence 入队处理；启动时已经错过的时间不补跑。一次性未来任务继续等待，过去任务标 missed。
5. 启用记录绑定具体 revision；更新 prompt、target、时间或工作空间后设 disabled，用户重新启用。
6. 未完成审批沿用原运行，不用下一次触发替代批准。
7. 用户暂停、取消、删除分别操作未来调度、当前运行、任务定义；删除后保留必要执行审计。
8. 主动提醒默认发送用户配置的本地提醒。R2 的外部渠道只有实际连接且用户明确选择目标与授权范围后才能成为投递目标；没有连接的渠道不能显示可发送。

## 8. 数据库迁移与升级

新增表归属 v1_task_world，使用现有 MigrationSpec/迁移摘要校验。以下版本号在本次搜索中未发现被占用，作为本文分配；真正实施前重新检查当前分支是否已登记，发现冲突应统一调整清单，不覆盖其他迁移。

| 拟定版本 | 文件归属 | 新增数据 |
|---|---|---|
| 1030 | `db/chat_runs.rs` | chat_runs、chat_run_events；对既有执行记录追加可空 run_id 关联 |
| 1031 | `db/chat_runs.rs` | chat_run_questions、chat_context_checkpoints、chat_run_resources |
| 1032 | `db/knowledge_collections.rs` | knowledge_collections、collection_documents、knowledge_references、knowledge_document_versions |
| 1033 | `db/memory_lifecycle.rs` | memory_lifecycle、memory_usage |
| 1034 | `db/learning.rs` | learning_sessions、learning_notes、learning_exercises、learning_attempts |
| 1035 | `db/automations.rs` | automations、automation_occurrences |

所有表包含必要外键、created_at/updated_at 和查询索引；revision 用于状态编辑，不代替业务对象 ID。chat_run_events 使用 `(run_id,sequence)` 唯一键；question 的解决与 run 状态更新同一事务；automation 触发的唯一键按上一节建立。

迁移 1032 同时为知识文档追加 revision；版本表以 `(document_id,revision)` 唯一，引用固定到该版本与资源 hash。旧版文档初始化 revision=1，不改变已有 resource_id。

历史消息不能凭数量推算真实 run。旧记录保持 run_id=null，UI 标记“历史记录，无本轮快照”，仍按现有 history 显示。不得把旧 completed 自动转换为当前目标验收通过。

历史知识文档建立初始 version 记录并指向现有 resource/hash，不复制一份原文，不重置文档 ID。历史 memory 原文、embedding 与 metadata 保留。

升级验证使用用户数据的测试副本。应用打开既有数据库之前完成必要版本检查；迁移错误返回真实失败，不删除数据库“恢复默认”。回退旧应用需要兼容已升级 schema 或恢复用户明确选择的备份，不能偷偷执行破坏性逆迁移。

新增运行快照、学习和定时数据默认仅本地。账号中心本地导出与删除说明必须覆盖这些数据，但不得未经协议设计把它们塞进已有云同步实体。私有云仓库保持独立。

## 9. 接线清单

- [ ] Rust 新模块在 `agent/mod.rs`、`modules/mod.rs`、`db/mod.rs`、`api/mod.rs` 的实际归属中声明。
- [ ] 新 router 合并到 protected router；审批与控制凭据沿用现有路径。
- [ ] 新数据库初始化在现有 bootstrap 生命周期中执行，测试数据库与正式数据库使用同一迁移入口。
- [ ] ask_user 和 read_tool_result 经现有 ToolRegistry 注册；工具元数据与风险级别真实声明。
- [ ] chat options、AgentEvent 序列化、frontend api、SSE parser、execution reducer 与历史加载同步更新。
- [ ] 新面板挂入已有页面；设置节点有对应真实 editor 与服务端配置，不添加只有标题的节点。
- [ ] 模型池实际路由、知识范围和记忆来源写入运行记录，日志页能通过 run_id 查询。
- [ ] 调度服务只在应用后端启动一次，退出时取消未来计时；不注册未经用户要求的 Windows 常驻服务。
- [ ] Tauri 悬浮窗和主窗口共用事件订阅与会话锚点；不启动第二个后台任务执行器。
- [ ] README、API 说明、数据导出说明和验收用例与最终实现一致。

## 10. R2 产品能力增量契约

### 10.1 会话操作与附件

以下拟新增接口归属 conversation_actions：

| 方法与路径 | 输入 | 结果 |
|---|---|---|
| GET `/api/conversations/search` | `q,cursor?,limit` | `{items:[{conversation_id,message_id,excerpt}],next_cursor}`；检索真实正文 |
| PATCH `/api/conversations/:id/presentation` | `{pinned?,archived?}` | 更新显示属性；不修改消息内容和运行状态 |
| PUT `/api/conversations/:id/draft` | `{text,resource_ids}` | 本会话草稿；Resource 必须已上传 |
| GET `/api/conversations/:id/draft` | 无 | `{text,resource_ids,updated_at}` |
| POST `/api/conversations/:id/branches` | `{anchor_message_id,action:"edit"|"regenerate",replacement_text?}` | 202 `{conversation_id,run_id}`；edit 必须提供替换内容 |
| GET `/api/conversations/:id/export` | `format=markdown|json` | 下载流；包含可见正文和资源引用，不带控制凭据 |

`/api/conversations/search` 等静态路径在路由注册时与 `:id` 区分。重新生成用原用户轮次的合法前缀创建兄弟分支；编辑用新用户输入创建分支。两者都不修改原历史、不继承旧批准、不把旧产物计入新目标完成率。

新会话尚无 ID 时草稿存前端当前用户的本地 draft key；得到 conversation_id 后迁移一次到服务端。后端上下文必须检查消息所属会话，不能仅凭 message_id 读取其他分支的正文。

附件复用现有 resource ingest、preview、content。新增 POST /api/chat 请求的 resource_ids 字段时，先核对现有协议是否已支持，已支持则复用；不新增第二个上传接口。

### 10.2 多模态任务与能力

```ts
type MediaOperation = "image_understand" | "image_generate" | "image_edit" | "ocr" | "pptx_read" | "document_generate";
interface MediaRequest {
  operation: MediaOperation;
  conversation_id: string;
  workspace_id: string;
  input_resource_ids: string[];
  prompt: string;
  output_format: "text" | "png" | "jpeg" | "markdown" | "docx" | "xlsx" | "pdf" | "pptx";
  service_profile_id: string | null;
  parent_resource_id: string | null;
}
interface MediaResult {
  run_id: string;
  operation: MediaOperation;
  resource_ids: string[];
  artifact_ids: string[];
  pages: Array<{ index: number; text_resource_id: string | null; status: "extracted" | "empty" | "failed" }>;
  verification_status: "pending" | "passed" | "failed";
}
```

拟新增 `POST /api/media/jobs` 接收 MediaRequest，返回 202 `{job_id,run_id}`；`GET /api/media/jobs/:id` 返回 `{status,result:MediaResult|null,error}`。它是既有任务执行的 API 包装，不自建模型调用队列。取消使用关联运行的既有取消路径。

image_edit 必须有 parent_resource_id 且支持编辑的服务档案；image_understand 至少一张可解码图片；ocr 不得无声忽略失败页；文档生成必须验证扩展名、MIME 与文件结构匹配。读取或生成 PPTX 时 DeliverableFormat 和上传 MIME 白名单同步扩展。

新增 capability 标识与实现挂钩：`image.edit`、`document.ocr`、`document.pptx.read`、`document.pptx.write`。未适配时 unavailable，已配置但未测试时 unknown，只有实际测试成功才 ready。

### 10.3 成果版本与个人偏好

```ts
interface ArtifactVersion {
  id: string;
  artifact_id: string;
  resource_id: string;
  parent_version_id: string | null;
  author_kind: "user" | "agent";
  run_id: string | null;
  created_at: number;
  current: boolean;
}
interface UserInteractionPreferences {
  preferred_name: string;
  language: string;
  response_length: "concise" | "standard" | "detailed";
  default_workspace_id: string | null;
  proactive_enabled: boolean;
  quiet_hours: { timezone: string; start: string; end: string } | null;
}
```

拟新增：GET `/api/artifacts/:id/versions`；POST `/api/artifacts/:id/versions` 输入 `{parent_version_id,resource_id}`；POST `/api/artifacts/:id/versions/:version/activate` 输入 `{expected_current_version_id}`。生成器先注册真实 Resource，再写版本关系。当前版本变化时失效的是该版本的验收引用，历史报告仍保留原版本证据。

用户偏好采用 GET/PUT `/api/settings/interaction`。角色配置仍使用既有 `/api/agents/:id/profile`；个人偏好不复制到每个 Persona。角色试聊通过普通创建会话接口及 agent 绑定完成，不增加另一种聊天执行器。

### 10.4 外部渠道

```ts
type ChannelKind = "feishu" | "weixin_ilink" | "onebot";
interface ChannelAccount {
  id: string;
  kind: ChannelKind;
  display_name: string;
  agent_id: string;
  enabled: boolean;
  status: "unconfigured" | "disconnected" | "connecting" | "connected" | "error";
  revision: number;
  allowed_capability_ids: string[];
  allowed_external_conversations: string[];
  reply_policy: "manual" | "approved_scope";
  credential_configured: boolean;
}
interface IncomingChannelMessage {
  channel_account_id: string;
  external_conversation_id: string;
  external_message_id: string;
  sender_id: string;
  text: string;
  resource_ids: string[];
  received_at: number;
}
```

内部 adapter 接口固定为 `validate/configure/connect/disconnect/send`，返回明确结果和外部消息 ID；真实平台字段只在 adapter 内部存在。通过平台消息身份去重，防止同一接收事件生成两轮会话；不得仅凭文本相同去重不同消息。

拟新增 GET/POST `/api/channels/accounts`、PUT/DELETE `/api/channels/accounts/:id`、POST `/api/channels/accounts/:id/test`、`/connect`、`/disconnect`、GET `/api/channels/accounts/:id/conversations`。保存时密钥写 SecretStore；响应只有 credential_configured。PUT 携带 expected_revision。

`POST /api/channels/accounts/:id/messages` 输入 `{external_conversation_id,text,resource_ids,source_run_id}`，走既有外发权限策略；返回 `{operation_id,status,approval_id?}`。发送操作查询 `GET /api/channels/operations/:id`，状态为 pending/approval_required/sent/failed/unknown。sent 仅代表外部平台接受，不代表收件人已读。

approved_scope 是用户显式设置的会话和能力范围，不是全局自动授权。渠道不继承桌面会话的工作目录权限。测试连接只发协议测试或查询，不自动给联系人发送消息；实际外发测试必须由用户明确触发。

### 10.5 新面板路由与预览范围

拟新增 `/automations`、`/channels`、`/workspaces/:id/learning`、`/workspaces/:id/creation`，在 WorkspaceSurface 注册。更多页的学习/创作入口先选择工作空间再进入对应路径；没有选定 workspace_id 时不构造空路径。

既有 `/more/plugins/:pluginId/:featureId` 继续承载声明式贡献。`/sync`、`/privacy` 保留现有账号页重定向。语音是全局浮层，不新增第二套聊天路由。

日常聊天显示模式新增 UI 偏好 `history_pinned`、`details_open`；首次默认均 false。使用原有 product preferences 持久化 history_pinned；details_open 只属于当前会话视图。窄屏仍使用互斥抽屉。

### 10.6 迁移与生命周期边界

| 拟定迁移 | 归属 | 数据 |
|---|---|---|
| 1036 | db/conversations.rs 或同域附属文件 | 会话展示属性与草稿；原消息表不复制 |
| 1037 | db/artifact_versions.rs | 产物版本、parent/current 关系；M07 笔记沿用同一版本能力 |
| 1038 | db/channels.rs | 渠道账号元数据、外部会话映射、消息去重身份与发送记录 |

编号实施前再次核查；不能覆盖并行发生的其他开发迁移。media job 复用 Task/Run 数据，仅需结果 Resource 关联，不建立重复任务状态表。

更新客户端必须先明确实际发布源、签名/校验方式和用户触发的安装入口。本规格不捏造尚未配置的更新地址；源缺失时显示“未配置更新来源”。导出/恢复需版本化清单、文件 hash 和明确的目标数据范围；恢复失败停止并报告，不自动重置用户数据库。
