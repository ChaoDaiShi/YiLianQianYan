# QQ 自主能力与消息工作台交付记录

## 版本位置

- 原版本已合并至本地 develop：`b08c848c8c7a254d495144e6002e87d933237364`。
- 原版本标签：`baseline/v1-r3-before-robot-autonomy`，本次未移动该标签。
- 新功能分支：`codex/robot-autonomy-messaging`。应用清单版本仍为 `1.1.0-rc.1`；没有生成新正式安装包，也没有推送远端。
- 设计与默认范围由用户于 2026-10-24 确认：探索好友和群，向指定联系人／群自动发言；微信使用 Akasha-WeChat。

## 已实现

QQ 使用原 OneBot HTTP／反向 WebSocket 传输，新增账号、好友、群、成员、最近会话、历史、已知消息与主动发信工具。主人从桌面或 QQ 私聊下达目标，进入同一 ChatRuntime / ReAct 链；普通联系人不会获得跨会话 QQ 工具或本机工具。已授权范围内自动发信；指定范围之外继续明确审阅。

策略、目标、运行和动作落入本地 SQLite。策略和运行／动作都有账号绑定；换账号后旧授权不自动生效。停止会取消原 ChatRuntime；进行中的发送保留未知送达状态。工具调用 ID 按运行命名空间区分，手动发送按 UUID 保存并重放原回执，同一任务对结果未知的对象不继续发信。

目标可以保存预设和工作空间，并通过现有 Automation 的显式 AgentTurn 目标按一次／每日／每周执行。原 Workflow Agent 节点仍为单次文本生成，未改写成另一套 Agent；运行、取消和定时历史都指向真实会话／轮次。

消息工作台调整为左侧账号、会话与通讯录，顶部集中状态和操作，中央展示消息／目标／回执，配置与登录使用抽屉。新增空会话时首条消息自动出现；回复、目标和预设草稿在本次应用内导航保留；目录和历史不会用过期响应覆盖当前对象。

Akasha 使用独立 `wechat_onebot` provider，不混用微信 iLink。提供固定上游提交的桥接补丁：独立 OneBot token、稳定目标 ID、唯一 bridge_event_id、UIA 操作后响应。没有微信服务器 message_id 时保持送达未知；不提供或伪装成 QQ 式通讯录／历史工具。环境和补丁步骤见 [Akasha 接入说明](../message-channels-akasha.md)。

## 验证结果

| 验证 | 结果 | 证据边界 |
| --- | --- | --- |
| Rust 格式、backend cargo check | PASS | 当前源码 |
| 后端库测试 | 1,271 项通过 | 完整受控库测试；不等于外部平台实号验收 |
| 机器人最终局部回归 | 18 项通过 | 包含审批优先级、停止、未知送达、重复回执和迁移 |
| security_execution | 10 项通过 | 原安全执行链未退化 |
| 前端单元测试 | 166 文件、788 项通过 | 最终工作台改动纳入现有测试 |
| 前端生产构建 | PASS | 保留已有 Vite 分包警告 |
| 浏览器工作台验收 | 22 项通过 | API fixture；1440、900、760 宽度，无页面／控制台错误 |
| Windows 桌面程序 | 构建、启动、真实资料库页面与控制会话 PASS | 本机 Tauri / WebView2，截图为实际旧对话记录 |
| Akasha 补丁 | 语法、diff、固定提交的应用／反向校验 PASS | 不执行真实微信 UIA 或账号发信 |

完整库测试首轮发现并修复两项问题：旧 SQL fixture 的会话表列数，以及已授权 QQ 发送仍被全局部分审批拦截。修复后完整回归通过。全局完全审批和明确拒绝规则仍优先。

## 实机账号与设计限制

- QQ：NapCat 服务已启动，但本轮原登录已失效，桌面正确显示“已配置 · 等待连接”。新增好友／群／历史的实号读取仍待用户重新扫码；不把旧 QQ 收发记录当作本轮新的主动操作验收，也没有新增对外测试消息。
- 微信：用户明确选择先完成代码，WeFlow 与桌面微信环境稍后补齐；真实 Akasha 账号收发为 HUMAN_PENDING。
- Figma：本轮插件返回 Starter MCP 额度已用尽，新布局没有回写 Figma。实现依据已批准的设计与现有主题；本地截图不冒充新 Figma 稿。

## 资料保护

新版本启动前备份了当前资料库，备份完整性为 ok。迁移 1063／1064／1065 已在真实资料库应用；迁移后原有 6 条对话、26 条消息、1,994 条资源、2 个渠道、2 个渠道会话与 4 条渠道消息的 ID 均保留，quick_check 为 ok。凭据沿用原 SecretStore，运行数据库、备份、实际账号日志和截图不进入 Git。

本机证据保存在仓库之外的测试目录与临时截图目录；本记录只提交功能、命令和结果。复验命令：

```powershell
$env:YILIAN_GIT_TEST_NONREPO_ROOT = '<已确认不在Git工作树内的外置目录>'
cargo fmt --all --check
cargo check -p yilian-backend
cargo test -p yilian-backend --lib --no-default-features -- --test-threads=1
cargo test -p yilian-backend --no-default-features --test security_execution -- --test-threads=1
npm --prefix frontend test -- --run --maxWorkers=1 --minWorkers=1
npm --prefix frontend run build
cargo build -p yi-lian-qian-yan
```
