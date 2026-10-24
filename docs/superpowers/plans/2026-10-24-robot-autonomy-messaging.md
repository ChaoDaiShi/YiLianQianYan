# 机器人自主能力与消息工作台实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 实现 QQ 主人目标驱动的探索与指定对象自动发言、消息工作台重排，以及 Akasha-WeChat 接入。

**Architecture:** QQ 使用现有 OneBot 连接、ChatRuntime/ReAct 与安全执行网关；定时目标扩展现有 Automation 的 AgentTurn 目标。微信使用独立 wechat_onebot provider，按 Akasha 实际回执展示状态。

**Tech Stack:** Rust / Axum / Tokio / SQLite / Tauri；React / TypeScript / Vite。

## Global Constraints

- 已批准设计：docs/superpowers/specs/2026-10-24-robot-autonomy-messaging-design.md。
- 在 codex/robot-autonomy-messaging 开发，develop 与 baseline/v1-r3-before-robot-autonomy 保留在 b08c848。
- 默认探索好友和群，向指定联系人／群自动发言。主人身份不等于普通 allowed_senders。
- 不新造 Agent 核心、调度器，不自动回退到鼠标操作，不虚构联系人或发送回执。
- 保留现有自动回复、人工审阅、取消与 SSE。主人指令单独分流，避免重复回复。
- 现有运行数据、凭据及 artifacts 不进入提交；无真实外部发送，除已明确选择的测试对象。
- Figma 插件额度已耗尽，用户知情批准继续；不把代码截图说成 Figma 已更新。

## 接口冻结

渠道新增 allowed_groups:string[] 与 group_trigger:mention|all；ChannelMessage/Session 新增 peer_type，消息新增 peer_id。旧记录显式迁移为 private。

机器人 API 基址 `/api/channels/:id/robot`：

| Method | Path | Input / Output |
|---|---|---|
| GET / PUT | /policy | policy: RobotPolicy |
| GET | /directory?kind=friends,groups,recent | items: {id,name,peer_type}[] |
| POST | /history | {peer_type,peer_id,count} → {data:OneBot实际结果} |
| POST | /send | {peer_type,peer_id,text,approved,request_id:UUID} → {action:RobotAction} |
| GET / POST | /missions | missions / {name,goal,preset_id,workspace_id} → mission |
| PUT / DELETE | /missions/:mission | 同上 / deleted |
| POST | /missions/:mission/run | run |
| GET | /runs | runs |
| GET | /runs/:run | run,actions |
| POST | /runs/:run/stop | run |

Rust / TypeScript 模型以 `backend/src/modules/channel/robot/types.rs` 为事实源，API 前端实现同步。RobotPolicy 包含 account_id,enabled,owner_ids,read_friends,read_groups,send_mode,send_private_ids,send_group_ids,send_all_private,send_all_groups,updated_at,channel_id。默认自动发送目标为空，不把 owner 身份默默扩大为全部联系人发送授权。

共享传输接口：

```rust
// 返回成功 response.data，保留 dispatched 以区分未发送和未知送达。
async fn call_action(server: &AppServer, channel: &Channel, action: &str,
    params: serde_json::Value, dispatched: &AtomicBool) -> Result<serde_json::Value, String>;
fn registry(server: std::sync::Weak<AppServer>, channel_id: &str, run_id: &str) -> ToolRegistry;
```

DB 接口：robot_policy、robot_run_for_conversation、robot_run、save_robot_action；工具侧每次以当前持久策略和运行绑定判断权限。控制会话不关联 channel_sessions，避免普通渠道工具隔离规则清空其 QQ 工具。

## Task 1：数据、目标运行与 API（主智能体）

**Files:** 新建 channel/robot/types.rs、mod.rs、tests.rs，db/channel_robot.rs、api/channels/robot.rs；修改 channel/types.rs、mod.rs、db/channels.rs、db/mod.rs、api/channels.rs、chat_runtime.rs。

**Interfaces:** 产生冻结模型与上述 API；消费 Task 2 的 registry/call_action。运行通过 chat_handler 消费真实 SSE，保存 conversation/user_message 绑定、真实工具动作及最终结果；停止取消同一 ChatRuntime。

- [ ] 先增加 DB 测试：私聊/群相同 ID 不冲突，策略持久化，主人 event 唯一，停止与历史保留。
- [ ] 运行 `cargo test -p yilian-backend --lib channel_robot --no-default-features`，确认新增用例失败原因是缺少实现。
- [ ] 实现 migration 1063（peer 类型）和 1064（策略、目标、运行、动作），不修改已发布迁移摘要。
- [ ] 接入本地目标、QQ 主人指令与 SSE 状态；普通聊天继续旧链路，主人最终报告有独立一次回执。
- [ ] 受控模型调用验证探索→发送工具、无重复回复与取消；复跑相关测试。

## Task 2：OneBot API、QQ 工具与消息权限（Luna xhigh）

**Files:** onebot_ws.rs、runtime.rs、protocol.rs、robot/tools.rs、安全模块及 ToolExecutionContext。与其他任务无共享编辑文件。

**Interfaces:** 产生 call_action、registry、消息权限／资源／授权证据；消费 Task 1 模型和 DB 函数。

- [ ] 先测试 echo 对应、平台拒绝、断线未知、群目标、策略撤销、伪造上下文和超范围审阅。
- [ ] 实现泛化 action 传输与 private/group 解析，不自动重试写入。
- [ ] 注册 qq_account/friends/groups/group_info/group_members/recent/private_history/group_history/message/send_private/send_group；目标绑定服务端渠道和 run。
- [ ] 向网关增加消息 read/send 描述，指定目标持久授权无需每条批准；显式拒绝和其他工具既有行为保留。
- [ ] 工具仅通过可信上下文执行，保存实际动作与回执。聚焦测试，主智能体统一 broad checks。

## Task 3：Akasha-WeChat（Luna xhigh）

**Files:** 新建 channel/akasha.rs 与测试、docs/message-channels-akasha.md 和显式桥接补丁。共享传输由 Task 2 接入 helper，主智能体接入 provider metadata 与创建检查。

**Interfaces:** parse_event、private/group send builders、classify_send_ack。微信 provider 为 wechat_onebot，只配置 reverse_websocket。

- [ ] 核对上游固定源码版本，测试实际事件省略 message_id 与提前空成功回执。
- [ ] 保留 sender 与 group peer，不能伪造远端 message_id；空 ack 标示桥接受理但送达未知。
- [ ] 对上游制作显式补丁，加入独立 OneBot token、稳定 ID 与发送后响应，不把 WeFlow token 当 OneBot token。
- [ ] 检查本机 WeFlow/API 前提，不读取私有 DB 或凭据；提供本机可执行的接入准备与安装说明。

## Task 4：消息工作台（Luna xhigh）

**Files:** frontend/src/features/channels、api/channels.ts、api/channelRobot.ts、e2e/channels-robot.mjs。

**Interfaces:** 消费冻结 API。设置、二维码、连接检查、预设使用原组件抽屉；实际会话与通讯录分别展示。

- [ ] 先增加实际选择会话、范围设置、发信确认、任务停止与草稿保留的交互测试。
- [ ] 完成左导航、顶栏、会话区、配置抽屉；窄窗切换不遮挡操作。
- [ ] QQ 通讯录、历史、主动发送、机器人策略、目标运行与计划均连接 API。
- [ ] 不使用假未读数／假的在线状态；Akasha 显示已知能力与未知送达，不误用微信 iLink 二维码。
- [ ] 运行受影响测试、生产构建、Playwright 实际界面检查，留下截图与结果。

## Task 5：Automation AgentTurn（主智能体）

**Files:** automation/model.rs、mod.rs、db/automations.rs、api/automations.rs、frontend/api/automations.ts 与现有 automation editor/history 必要适配。

**Interfaces:** Automation 增加显式 target，kind=workflow 或 agent_turn；agent_turn 对应 mission_id。旧 workflow_graph_id 字段保留，agent_turn 传空串；旧存储记录显式迁移。

- [ ] 用受控任务测试一次／周期触发、不补跑、运行映射及取消，旧 Workflow 目标测试继续通过。
- [ ] 调度器 AgentTurn 分支调用 Task 1 同一运行入口；occurrence 保存 robot_run_id，不把它伪装为 Workflow run_id。
- [ ] 历史、重启中断与停止按目标类型处理。UI 计划配置实际保存任务，用户能够查看与停用。

## Task 6：整合审查与桌面验收

- [ ] 主智能体检查每个任务的范围和代码质量，再做全分支审查；修复重要问题后再验证。
- [ ] `cargo fmt --all`、`cargo fmt --all --check`、`cargo check -p yilian-backend`。
- [ ] 受影响后端测试及 security_execution 集成测试；最终库测试一次。非仓库 fixture 使用外置目录。
- [ ] 前端相关测试、全量测试一次及 `npm --prefix frontend run build`。
- [ ] 真实 NapCat 只读检查账号／好友／群／历史，不泄露联系人完整列表；真实发送仅针对已授权测试联系人。
- [ ] 更新本机桌面程序，检查实际页面、任务运行与设置。微信缺失本机登录时记录 HUMAN_PENDING，其他交付继续完成。
- [ ] 记录提交、修改文件、证据与局限；不擅自合并后续功能到 develop 或推送远端。
