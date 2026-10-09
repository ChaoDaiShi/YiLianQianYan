# Chat Reliability R1 — 历史快照与实时状态契约

基线：Workbench UX U2 `dabf3fc9552574f743cfd51e7b6568a667ac11a4`。
分支：`fix/v1-chat-init-race`。工作树：`YiLian-v1-chat-reliability`。

## 已完成计划

- [x] R0：真实 ChatView + system Edge，先确认旧 history 覆盖即时回复的失败，再修改生产代码。
- [x] R1：增加局部 ownership，保护 messages、execution、approval hydration 和 voice anchor。
- [x] R2：7 项纯规则测试；8 组真实组件检查，覆盖切换、刷新、终止、错误、停止与 StrictMode。
- [x] R3：删除 U2 首 token 等待；真实后端、即时 loopback、system Edge，同一浏览器 10 新会话、5 强制旧快照、10 重载和实际 ID 记录。
- [x] R4：完整前端测试和构建各一次；Core/U2/R1 E2E；报告、边界复核、小步提交。最终交付 HEAD 与推送结果另存 ignored target 的 manifest/push JSON。

## History snapshot ownership

历史 GET 返回快照，不拥有实时消息的最终写入权。每个请求捕获：

- `epoch`：当前会话身份代次。
- `revision`：本地/实时状态更新版本。
- `generation`：最新 hydration 请求代次。
- `protected`：请求开始时是否属于 live turn，或者刚由本地草稿晋升的会话。

只有 **身份仍有效、请求仍是最新、revision 未改变、protected 为 false**，才允许替换 messages 和执行历史。messages 函数式更新内部再次检查，避免排队更新期间所有权变化。

较早请求不能覆盖较晚请求。切换 A→B 后，A 的迟到结果不得更新 B 的 messages、execution、approval 或 voice anchor。卸载也使未完成请求失效。

## Live update ownership

发送在 optimistic user append 前标记 live 并增加 revision；真正处理的 AgentEvent、completed assistant append、重试删除本地消息等路径推进 revision。审批提交同样取得当前所有权。done/error/stream_end 结束 live 状态并推进 revision。

请求期间产生更新，或请求开始时已存在 live turn，均使快照不能替换当前内容。执行历史与 messages 使用同一 admission 条件。

新会话 connected（或附件发送前创建会话）将本地草稿晋升为后端会话 ID：增加 epoch、保留本地内容和运行状态，并更新当前 stream callback 的 ticket。这是同一次发送的身份确认。晋升后的首个 history 请求受到保护，即使 connected/token/done 已在请求前被 React 合并处理，也不会覆盖刚完成的回复。

发送前 readiness/附件异步操作、stream callback、审批 callback 和 stop 异步完成均检查捕获的 epoch。stop 仍使用现有 abort 与 stopGeneration。

## Conversation identity 与消息 ID

会话身份与消息 ID 各有职责。后端只读代码检查和真实 E2E 确认：

| ID | 来源 | 与持久化 ID 的关系 |
| --- | --- | --- |
| optimistic user | 前端本地 UUID | 不等于 persisted user |
| persisted user | 后端保存用户消息时生成 | 权威历史 ID |
| live assistant | engine done.message_id | 不等于 persisted assistant |
| persisted assistant | 后端保存助手消息时生成 | 权威历史 ID |

因此不做按 ID 拼接或按内容猜测合并。有竞争时跳过整个快照；空闲刷新、重新进入和 reload 正常采用完整持久化历史，以权威 ID 替换本地 ID，同时避免重复消息。

每个活动会话保存已消费的 live completion ID。重复 done 只能 append 一次，即使空闲刷新已用 persisted ID 替换 live ID，也不能再次 append 原 live ID。真正切换会话时清空集合。既有 execution reducer 不变。

## Approval 和 Voice anchor

`listPendingApprovals/getApproval` 完成后重新检查 epoch、generation、revision，拒绝旧会话或过时请求；既有 conversation_id 过滤和 reconciliation 保留。

Voice anchor 检查 identity/request。合法的当前会话元数据可以更新 anchor，即使 messages snapshot 因 live mutation 被拒绝；A 的迟到响应不能把 B anchor 改回 A，真实切换时先清空旧 anchor。

错误显示、loading 结束、approval required/resolved 和停止生成都有 focused 回归。异步 ownership 仅管理当前 UI，不改变传输协议或后端修订语义。

## 验证与边界

见 [R1 验收报告](chat-reliability-r1-report.md)：修复前失败证据、7 项纯测试、8 组真实组件检查、真实后端 10 会话验证。

生产修复没有 timeout/sleep/requestAnimationFrame 或 E2E 分支。既有完成状态提示计时器保留。测试等待明确网络、DOM、已提交消息状态，不靠固定延迟掩盖竞态。

Backend/Rust/Cargo/DB schema/migration/REST/SSE wire/Secret 语义、CSS、Canvas、AppShell、voice runtime 均未修改。E2E 使用隔离数据库和工作目录，经现有 API 创建并删除专用模型 profile 及 SecretRef，不修改用户默认密钥。

保守限制：竞争期间不合并历史，尚未显示的旧内容需要后续空闲刷新或重新进入会话恢复。本轮不增加复杂 reconciliation engine。
