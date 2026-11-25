# 2026-11-24 能力升级：架构、迁移与验收入口

需求基线见同目录的 [开发说明书](requirements.md)。本次在 `feat/personal-proactive-assistant` 的工作树上开发，开始时 HEAD 为 `2817d110b945c2d919aa3019fed458388ae799fc`。工作树包含早于本次开发的改动。提交号只标识共同祖先。

## 模块复用与差距审计

| 需求 | 起点 | 实施位置与决策 |
| --- | --- | --- |
| 初始化与画像 | 既有首次模型配置、系统偏好和账号隔离数据库 | 在原 `/setup` 增加七步向导。用户资料由 `modules/onboarding.rs` 管理，助手角色继续由 `api/agent_persona.rs` 管理。旧版完成标记沿用。 |
| 交互聊天 | 对话消息、Markdown、附件、任务审批和成果库 | 在原消息上扩展声明式组件，将提交状态与消息绑定。资源引用沿用现有 ResourceService。 |
| 长期记忆 | SQLite 记忆原文、修订、关系、来源、撤回日志和检索 | 复用治理和检索链路，增加本地向量推理和后台索引。见 [本地向量与 CLI 对话](../../architecture/local-embedding-and-cli-chat.md)。 |
| 工作空间 | 原有工作空间注册、默认工作空间、学习与创作项目 | 页面表引用现有 workspace ID，新增页面树、块和修订。在工作空间详情内编辑。 |
| 模型与上下文 | 模型档案、模型池、聊天轮次、上下文清单、任务恢复 | 将新增 CLI 轮次挂接原有会话。REST API、CLI 的真实执行能力和外部会话可恢复性分开记录。 |
| 桌宠与感知 | 独立透明窗口、图层角色、闲置和对话反馈、窗口位置持久化 | 扩展原桌宠窗口。用户选择的 Live2D 模型、渲染运行时和单次原生屏幕选择分别管理。 |
| 桌面与开放生态 | Tauri 主窗口、软件挂载、插件托管、主题、服务发现 | 继续使用可信窗口和平台能力；探测、连接、调用回执和第三方状态分别报告。 |
| 跨模块基础 | 控制会话、审批网关、SQLite 迁移、产品事件总线、外部执行器信号量 | 事务内记录资源变更、增加可恢复游标、按硬件和设置配置执行器名额。 |

## 初始化和授权

七步向导位于 `/setup`。各步骤可选，输入会写入当前数据库的草稿。草稿和已生效的资料分别存储。提交时，用户资料、字段最近修改时间、默认工作空间和旧版 `setup_completed` 在同一个 SQLite 事务更新。主题和桌宠使用原有界面即时设置机制。当前个人资料用于普通个人对话，确认后的记忆偏好继续沿用候选审阅流程。

- `GET /api/v1/onboarding/status` 返回当前步骤、草稿、有效资料和修订。
- `POST /api/v1/onboarding/steps` 保存草稿。`POST /api/v1/onboarding/complete` 提交向导。请求包含 `expected_revision`、`current_step`、`profile` 和可选 `workspace_id`。
- `GET /api/v1/profile` 只返回有效资料及来源时间，`PATCH /api/v1/profile` 支持部分更新。传 `{"expected_revision":7,"fields":{"display_name":null},"permissions":{"screen":false}}` 可撤销称呼和截屏授权。409 要求重新读取。删除字段会清除同一档案中残留的向导草稿。
- `memory`、`screen`、`microphone`、`files`、`external_model` 分别控制相应能力。首次向导以显式选择开始。已有配置缺省沿用原有策略，屏幕截图须明确打开。工具仍须通过现有执行网关和审批。撤销麦克风会结束现有语音会话。

## 事件和资源预算

旧版 `GET /api/events` 接口继续返回具名 SSE。客户端重连时传 `?after=<最近事件 UUID>`。服务端在同一把锁内订阅实时事件并取得可回放区间；游标已过期时发 `core.events.lagged`，消费方可重新读取所属领域的状态。

重要持久化变更由 SQLite 触发器写入 `activity_events`。索引保存序号、流标识、对象 ID、变更类型与必要的修订或状态。正文和凭据由原数据所有者管理。消息、轮次、任务、记忆、页面修订、组件提交、资源、个人资料共用这套提交顺序。新迁移之前的历史仍从原数据表读取。

- `GET /api/v1/events?after=0&limit=100&stream_id=conversation:<id>` 返回有序事件和 `next_cursor`。分页最多 500 条。`GET /api/v1/conversations/:id/events` 提供会话视图。
- `GET /api/v1/events/stream?after=<sequence>` 支持跨重启按序继续读取和心跳。该接口使用持久化整数序号。旧接口使用 UUID。两类游标应按各自接口保存。
- `GET|PUT /api/v1/resources/policy` 读取或更新自动、节能、平衡和性能模式。CLI 并发及后台索引间隔可以手动覆盖。响应列出配置预算、本次运行预算和可用名额。执行器并发设置在下次启动时生效，避免运行中的会话丢失名额。原有插件租约和任务预算仍由各自执行服务管理。

主服务每 15 秒检测一次可用内存。可用内存比例低于“运行资源”设置的阈值时会发出 `resource.pressure.changed`，索引条目间隔至少 750 毫秒，开启的桌宠切换到减少动效模式。内存恢复到阈值以上 3 个百分点后解除降速。设置的阈值默认为 10%。

## 升级与数据恢复

现有本地数据库第一次进入此版本时，在同一目录创建 `<库名>.before-capability-upgrade-<UUID>.backup`。SQLite 在线备份包含 WAL 已提交事务，备份通过完整性快速检查后才取得最终文件名。备份位于本地，含升级前的数据。新安装跳过备份步骤。摘要校验与迁移版本继续使用既有注册机制。新版本写入的数据需要使用新版本读取。

恢复升级前状态时，先退出所有使用该档案的客户端和服务并另存当前档案目录。把备份恢复到一个独立副本，在原版本中打开该副本，核验会话、记忆和附件后再决定切换。资源文件、模型文件、系统凭据和记忆撤回日志具有独立生命周期，SQLite 备份只覆盖数据库。迁移资产目录时，内容寻址附件继续按摘要从当前目录读取并验证内容。

## 测试入口

- `cargo test --workspace --offline --no-fail-fast`、`cargo check --workspace --offline --all-targets`、`cargo fmt --all -- --check`
- `npm --prefix frontend test`、`npm --prefix frontend run build`、`npm --prefix frontend run test:e2e`
- `node frontend/e2e/core-paths.mjs --capability-upgrade` 使用临时用户目录、真实本地 HTTP 和 Edge 验证向导中断恢复、版本冲突、权限撤销、事件游标和资源设置。
- `python scripts/qa/verify-capability-upgrade.py seed <旧版后端> <新的临时目录>` 生成隔离的旧版档案。随后用 `verify <新版后端> <同一目录>` 校验升级、完整性、外键及从备份恢复三条固定数据。

