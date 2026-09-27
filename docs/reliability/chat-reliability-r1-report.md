# Chat Reliability R1 验收报告

日期：2026-09-27（Asia/Shanghai）。结论：**PASS**。

U2 已知的“新会话即时回复被首次历史加载覆盖”问题已在本轮修复。U2 原报告未修改，继续作为历史证据。本轮仅处理 Chat 前端可靠性。

## 交付身份与范围

| 项目 | 值 |
| --- | --- |
| 基线 | `feat/v1-workbench-ux-u2` @ `dabf3fc9552574f743cfd51e7b6568a667ac11a4` |
| 分支 | `fix/v1-chat-init-race` |
| 工作树 | `F:\项目开发\忆涟千言\YiLian-v1-chat-reliability` |
| 生产修复提交 | `d2e2f62`，只有 ChatView 与局部 ownership helper |
| 最终 E2E 代码提交 | `1fd53dc` |
| 最终 HEAD 与远端校验 | ignored `target/chat-reliability-r1/manifest.json`、`push.json` |
| 保护范围 | Backend/Rust/Cargo/schema/migration/REST/SSE wire/Secret 语义改动为 0；CSS/Canvas/AppShell/API/voice runtime 改动为 0 |
| Git | 仅本轮分支提交与授权推送；不 merge/tag/release |

复用已有依赖和已编译后端，没有新依赖、Cargo 执行或锁文件修改。后端二进制 SHA256：`AFF6732E1EE3595BA1C6A0BF3B1AFF11280B67241ADB48734594A39482F333C5`。

## 根因与修复

基线 loadConversation 返回后无条件 setMessages(snapshot)，并 hydrate_history 替换执行历史。新会话 live completion 先追加助手消息后，旧 history 可能将其移除。effect cleanup 的 cancelled 检查不足以处理同一活动会话内的先后竞争。

现在每个请求携带 epoch、revision、generation；身份改变、请求过时、请求期间发生更新、请求开始时属于 live turn 或刚晋升草稿的首次加载，均禁止旧快照替换。合法空闲历史仍正常加载。发送/审批/停止的异步回写绑定会话，Voice anchor 检查身份和请求代次。

另一个实测问题是：空闲刷新以 persisted assistant ID 替换 live ID 后，重复 done 会再次追加原 live ID。会话级已消费 completion ID 集合解决该重复。没有改写 execution reducer、REST 或 SSE 协议。完整规则见 [行为契约](chat-init-race-contract.md)。

## 失败到通过的证据

以下相对目录位于本工作树 ignored `target/chat-reliability-r1/`，时间目录使用 UTC。

| 阶段 | 目录 | 结果 |
| --- | --- | --- |
| R0，生产代码仍为基线 | `2026-09-27T05-20-18-054Z` | 预期 FAIL：Older history snapshot overwrote the instant live assistant；user+assistant 变为仅 persisted user |
| 重复完成回归 | `2026-09-27T05-29-32-031Z` | 预期 FAIL：刷新后 duplicate done 得到 3 条，期望 2 条 |
| 最终真实 ChatView 组件 | `2026-09-27T05-31-15-969Z` | PASS，8 组；StrictMode、system Edge、受控 transport/leaf components |
| 最终真实后端 R1 | `2026-09-27T05-38-21-488Z` | PASS，同一浏览器 10 会话、5 强制旧快照、10 重载 |

调试失败记录保留但不计作成功证据：早期 harness 的路由匹配/控制会话初始化错误、未 verify 模型导致 activation 400，以及 `05-37-19-340Z` 的过早读取。后者把流式文本出现当作消息已经提交，在第 10 例过早读取 ID；最终改为等待非 streaming 助手 DOM 与真实已提交消息状态，并从当前 React root 读取 ID。修正仅在测试中。`05-28-29-539Z` 较早成功记录缺少 optimistic user ID，已由最终完整记录替代。

## 最终验收矩阵

| 要求 | 结果 | 依据 |
| --- | --- | --- |
| 即时回复、同一浏览器连续 10 新会话 | PASS | R1 cases 1–10 |
| 强制旧 history 晚返回 | PASS | cases 6–10，5/5 done 早于 history end |
| reload persistence | PASS | 10/10 重开后各 1 user + 1 assistant |
| 无重复助手 | PASS | R1 每例 1 条；组件涵盖 refresh 后 duplicate done |
| A→B 切换、A 晚返回 | PASS | 组件检查 3 |
| 旧 Voice anchor 不回写 | PASS | 组件检查 3、7 |
| 既有会话正常 hydrate | PASS | idle refresh、B history、R1 reload |
| refresh revision 竞争 | PASS | 组件检查 4、6 |
| execution / approval | PASS | 组件检查 4、7，既有 reducer/approval 单测 |
| done/stream_end 两种顺序及重复 done | PASS | 组件检查 1、2、4 |
| provider error / stop | PASS | 组件检查 5 |
| connected/token/done 先于首次 history | PASS | 组件检查 8 |
| 纯 ownership 规则 | PASS | 7 项 |
| 全前端测试 | PASS | **100 文件、483 项**；verification/frontend-tests.json |
| Architecture 前端检查 | PASS | 完整测试内 architecture/boundaries.test.ts **5 项**，另有 API domainBoundary 1 项 |
| 生产构建 | PASS | verification/build.log |
| Core 真实后端 E2E | PASS | regression/core/result.json；2 graphs、3 Settings 尺寸 |
| U2 真实后端 E2E | PASS | regression/workbench-u2/result.json；16 响应式记录及既有 flows，首 token 等待已删除 |
| R1 真实后端 E2E | PASS | 2026-09-27T05-38-21-488Z/result.json |
| 后端代码改动 | NO | scope-review.json，保护路径差异为空 |

最终完整测试和生产构建各执行一次，之后只修正测试读取条件并定向重跑 Chat R1 E2E。生产代码未在最终 gate 后更改。

System Edge：`C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe`，版本 `154.0.4258.37`。R1 browser error 为 0；开发模式保留 React Router future flag 警告。U2 的 1 次主动 provider 拒绝已明确核算，其余非预期错误为 0。

Canvas Stability / Studio C2 **本轮未重跑**。Canvas/CSS/AppShell/共享 API 无改动，按任务书引用冻结的 PASS 基线；C2 为 `cd5100eb153ce0a2415b5db5215581b6f018f895`。历史证据为 docs/ux/canvas-stability-report.md、canvas-studio-c2-report.md 和 workbench-u2-report.md。Core result 明确记录 canvas_stability=not_run。

## 实际消息身份与时序

最终 R1 第 6 会话：`391141ea-df7f-4ab8-abb6-982e42d462cb`。

| 来源 | 实际 ID |
| --- | --- |
| optimistic user | `df3ee9c1-cd7a-441d-ab7a-34008ed37d5c` |
| persisted user | `b67642bf-bb94-4bf9-9cfe-3127988815a5` |
| live assistant | `cb614d90-f41f-4431-b593-c80f0a60e81e` |
| persisted assistant | `d6cb4a1c-e823-4fce-9389-685b2af50a40` |

全部 10 例实际断言两组 ID 不等。分别来自真实 UI MessageList、真实 SSE done 与真实后端 GET，没有内容匹配或猜测；全部 ID 见最终 result.json。

第 6 例观测时间（Unix ms；response clone 诊断时间，不声称是 React reducer 内部调度时间）：connected=1790487516473，token=1790487516483，history start=1790487516484，done=1790487516490，stream_end=1790487516498，旧 history end=1790487516562。

强制场景等待真实助手已提交，再释放旧 history。message-state-snapshots.json 保存 live / after_stale_history / reloaded；forced-order-proof.json 汇总 5 次时序。旧响应后保留 live assistant ID，reload 后采用 persisted assistant ID，每次仍只有一条助手消息。

## E2E 解释与隔离

后端、HTTP/SSE、SQLite、system Edge 均真实运行。外部模型由明确标记的 loopback protocol fixture 代替，收到 stream request 后立即返回，不等待 history，也不人为延迟首 token。U2 beforeStream 和双 requestAnimationFrame 等待已删除。既有 U2 非 Chat 流程的条件轮询不承担本竞态修复。

5 次强制场景在测试拦截器内获取真实 GET 响应，再移除新助手消息和 execution_history，形成标注 **test_only_stale_projection** 的旧快照，等待 live completion 后释放。即时持久化可能早于 GET，单纯延迟并不能保证不含助手，因此显式构造旧快照；这不等同于后端原始响应。原始/释放 ID 均记录。生产代码无测试分支。

每轮使用临时数据库和工作目录。模型 profile 唯一且一次性，verify 后 activate；finally 通过既有 DELETE 删除 profile 及对应 SecretRef，再 GET 验证不存在。不修改默认密钥。日志只记录鉴权是否匹配，不记录密钥。

最终目录保留 result、request timeline、message snapshots、fixture/browser/network 日志、backend/frontend 日志、端口进程记录、退出码和 instant-reply-reloaded.png。Core/U2 证据已复制到 regression 下；target 不进入 Git。

## 变更文件清单

| 文件 | 类型与用途 |
| --- | --- |
| frontend/src/components/chat/ChatView.tsx | 修改：ownership 接入、异步回写检查、完成去重 |
| frontend/src/components/chat/conversationOwnership.ts | 新增：epoch/revision/generation、完成 ID 集合 |
| frontend/src/components/chat/conversationOwnership.test.ts | 新增：7 项纯规则测试 |
| frontend/e2e/fixtures/chat-view.html | 新增：组件测试入口 |
| frontend/e2e/fixtures/chat-view.tsx | 新增：StrictMode 宿主、切换/刷新/anchor 控制 |
| frontend/e2e/chat-view-race.mjs | 新增：8 组真实组件检查 |
| frontend/e2e/chat-reliability.mjs | 新增：真实后端 10 会话与强制旧快照 |
| frontend/e2e/core-paths.mjs | 修改：focused/R1 模式、Core 显式跳过冻结 Canvas |
| frontend/e2e/workbench-flows.mjs | 修改：删除首 token 等待，复用即时 fixture |
| docs/reliability/chat-init-race-contract.md | 新增：行为契约与完成计划 |
| docs/reliability/chat-reliability-r1-report.md | 新增：本报告 |

提交：ac42a84 失败复现 → d2e2f62 修复 → fde3acd 扩充回归 → 1fd53dc 真实后端验证 → 文档交付提交。最终 HEAD/推送校验保存在 manifest/push JSON。

## 验证命令

本轮工作树内执行。每个 E2E 模式使用新的 PowerShell 会话，避免模式环境变量混用。

```powershell
# 最终完整 gate，本轮各一次
npm.cmd --prefix frontend test -- --reporter=default --reporter=json --outputFile=../target/chat-reliability-r1/verification/frontend-tests.json
npm.cmd --prefix frontend run build

# 每组独立会话
$env:YILIAN_E2E_CHAT_FOCUSED='1'
npm.cmd --prefix frontend run test:e2e

$env:YILIAN_E2E_SKIP_CANVAS='1'
npm.cmd --prefix frontend run test:e2e

$env:YILIAN_E2E_WORKBENCH='after'
npm.cmd --prefix frontend run test:e2e

$env:YILIAN_E2E_CHAT_R1='1'
npm.cmd --prefix frontend run test:e2e
```

## 保留限制与停止点

1. 本地 fixture 验证协议和时序，不声称覆盖真实云模型质量、网络波动或真实语音输入输出。
2. 竞争历史整体跳过，不做复杂合并；未显示的旧历史可通过后续空闲刷新/重新进入恢复。
3. 测试读取当前 React tree 仅为获得真实本地 ID；React 内部结构升级时需维护测试，生产代码无此依赖。
4. 构建保留主 chunk >500 kB 警告，本轮 522.15 kB（gzip 164.03 kB），不扩大为性能重构。
5. 本轮要求的可靠性验收全部完成，无遗留阻塞项。分支交付后停止，不进入 Generic MCP Canvas、GIS、v1.1、v1.2 或 v2。
