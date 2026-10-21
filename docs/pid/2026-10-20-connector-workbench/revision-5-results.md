# 第5轮开发与验证结果

日期：2026-10-21。分支：`codex/connector-workbench`。本轮对应五项最新验收反馈。代码保留在当前工作区，未提交、推送或发布安装包。

## 交付行为

| 反馈 | 当前行为 |
|---|---|
| 收起浮窗不能移动、缺少状态 | 监控/通话收起圆形共用6px阈值手势；拖动使用真实Tauri startDragging，点击才展开。64逻辑像素客户区显示实际状态，位置保存延续既有机制。 |
| 输入区越来越大 | 聊天改为单行胶囊；工作与创造使用文本区加一行控件。模型/思考统一入口，审批放进紧凑弹层，发送/停止为图标。长输入按实际内容扩展并限制高度。浏览器实测聊天58px、工作122px。 |
| 聊天/工作/创造与自切换 | 三模式可手动选择、保存、刷新恢复。实际AI开始文件写入、命令、进程或桌面操作时聊天转工作，图像生成转创造；未做用户文字关键词推断。工作与创造具有对应执行指引并显示Git入口，模式不会改变审批权限。 |
| 对话与画布连通 | 默认画布直接读取已有主页会话，不要求创建画布或绑定智能体。每段会话独立成组，用户/助手消息逐节点纵向连接；新分支从真实来源回答以贝塞尔曲线跨组相连。保留运行状态、历史、组与消息位置、视口；已保存旧画布仍可管理与删除。 |
| Git与文件变更 | 工作/创造右栏显示Git节点管理器，选择真实工作空间，查看分支、HEAD、本地上游领先/落后、暂存/工作区/未跟踪文件及差异。读取不修改index或仓库配置。 |
| 设置过长 | 一级收敛为模型与服务、语音、智能体与上下文、安全、界面、扩展六组；二级为紧凑列表，继续进入真实设置表单。右侧管理器分组默认折叠，保留逐项显示控制与插件设置。 |

## 实际运行证据

本轮在普通浏览器与原生WebView2分别验证15项流程：普通交流保持聊天；让AI写工作说明后实际调用write_file、自动进入工作并生成文件；Git差异展示对应文件的新内容，读取前后index字节保持相同；手动进入创造并实际生成中文文件，刷新保留模式；无需绑定即可在画布找到主页消息；从第一条回答创建新会话组，来源曲线连接，模型上下文只包含该回答之前的祖先和分支新问题，不混入后续兄弟消息，源会话记录不变；刷新保留分组关系；六组设置及二级表单往返有效；审批后切走和刷新仍可完成后台文件写入。

两个浮窗另外通过Win32 SendInput执行了真实左键按下/移动/抬起。监控窗口坐标(128,128)变为(166,166)，通话窗口(192,192)变为(230,230)；客户区始终64×64逻辑像素，没有误展开，并恢复了测试前的鼠标位置。Windows外框98×90物理像素与80×80客户区分别核验（缩放1.25），没有把外框误作圆形内容尺寸。

模型服务使用本地可控HTTP响应，所有审批、工具执行、文件写入、Git读取、SQLite保存及Tauri窗口行为均走项目实际实现。工作目录与进程独立，未读取用户密钥，未关闭用户桌面实例。

## 代码与接口

- 浮窗：`frontend/src/features/voice/floatingSurface.ts`、`CallOverlay.tsx`、`floatingSurface.css`、`features/monitor/MonitorOverlay.tsx`与`TokenUsagePanel.tsx`。
- 三模式与输入：`components/chat/ChatInput.tsx`、`ComposerPopover.tsx`、`WorkspaceModeControl.tsx`、`ComposerModelControls.tsx`、`compactComposer.css`；`features/execution/conversationRuntime.ts`、`pages/ChatPage.tsx`。
- 模式持久化与自动事件：新增`backend/src/db/conversation_modes.rs`与`api/conversation_modes.rs`，接入`chat.rs`、`chat_lifecycle.rs`、`approvals.rs`、`ChatRequestOptions`和Conversation详情。
- 消息分组：新增`db/connector_messages/overview.rs`及`frontend/src/features/connectors/messages/ConversationCanvas.tsx`、`groupedMessages.ts`，扩展既有`connector_messages` API。迁移1024仅保存新分支来源关系，旧关系沿用原数据。
- Git：新增`backend/src/api/workspace_git.rs`、`workspace_git/read.rs`，前端`api/workspaceGit.ts`与`features/git/`。
- 二级设置：`features/settings/canvas/settingsGraph.ts`、`SettingsGroupDialog.tsx`、`SettingsNodeManager.tsx`与`SettingsCanvasPage.tsx`。

接口：

```text
GET /api/message-canvas
PUT /api/message-canvas/layout
POST /api/message-canvas/branches
PUT /api/conversations/:id/mode {mode: chat|work|create}
GET /api/workspace-git?workspace_id=
GET /api/workspace-git/diff?workspace_id=&path=&staged=
```

workspace_mode SSE返回mode/reason/automatic。创建分支继承实际祖先协议、模式、模型选项、推理内容及已存在的智能体绑定；原会话未绑定时，新分支也不自动绑定。

## 验证结果

| 检查 | 结果 |
|---|---|
| 前端完整测试 | 152文件，730/730通过 |
| 后续聚焦检查 | 15/15通过 |
| 前端生产构建 | PASS |
| Rust单元测试 | 1088/1088通过 |
| 架构 / 安全集成 | 5/5、10/10通过 |
| cargo fmt --check / cargo check | PASS |
| 浏览器流程 | 15/15通过 |
| 原生工作台流程 | 15/15通过 |
| 原生窗口/浮窗检查 | 10项通过，包含两次真实收起圆形拖动 |
| 原生未处理异常 / 意外控制台错误 | 0 / 0 |

命令：`npm --prefix frontend run test`、`npm --prefix frontend run build`、`npm --prefix frontend run test:e2e`、`cargo fmt --check`、`cargo check`、`cargo test -p yilian-backend --lib`、`cargo test -p yilian-backend --test architecture_boundaries --test security_execution`。Git测试临时目录使用F盘仓库外目录，以真实确认非Git目录不会误命中本项目祖先仓库。原生QA从当前源码复制构建独立可执行文件并核对哈希，后端引用当前仓库。

[验证摘要](verification/revision5-summary.json)、[原生结果](verification/revision5-native.json)、[浏览器结果](verification/revision5-browser.json)、[原生构建证据](verification/revision5-native-build.json)、[10张原生预览](revision5-previews/index.md)。截图保留原始内容，附来源和SHA-256。

## 边界

Git面板本轮提供本地版本状态与变更查看；领先/落后按本地已存在引用计算。完整刷新恢复实际后台状态与持久化结果，刷新前未保存的流式片段不回放。官方厂商付费调用及真实麦克风连续通话未在本轮运行。构建仍有既有大chunk和未使用项提示，无编译错误。
