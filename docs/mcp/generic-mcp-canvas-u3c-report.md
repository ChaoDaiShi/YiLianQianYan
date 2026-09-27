# Generic MCP Canvas U3-C 验收报告

日期：2026-09-27。**U3-C：PASS。Generic MCP Canvas U3 STRICT GATE：PASS。**
原 U3 的 PARTIAL 历史不改写；本轮补齐持久化 MCP 资源授权硬门禁。
验收代码提交：`74c620b`，其后仅整理文档。

## 基线与范围

- 基线：`feat/v1-generic-mcp-canvas-u3 @ 3adeb10f87f32c619c4786af2778fef17ef96757`。
- 分支：`fix/v1-mcp-resource-grants-u3c`。
- 独立工作区：`F:/项目开发/忆涟千言/YiLian-v1-mcp-grant-u3c`。
- 本轮只关闭 MCP Resource Grant 缺口。复用既有 Grant CRUD、SQLite JSON
  存储、Gateway、审批恢复和 Canvas Task Harness。
- 无子智能体；无 push、merge、tag、release；不进入下一周期。

## 实现

`GrantResource::Mcp { server_id, tool_name: Option<String> }` 是新增 JSON
variant。`None` 为精确 server 的全部工具，`Some` 为精确工具名。身份区分
大小写，不按展示名称或子串匹配；限制为非空白、256 UTF-8 字节、无控制字符、
无星号。只允许搭配 `McpInvoke`。

McpInvoke 进入现有资源授权评估，显式 Deny 优先；缺少匹配或 Allow 过期为
RequireApproval。保持 MCP High Risk，持久化 Allow 不能免除风险审批；
单次审批不会创建持久化 Grant。`resource_scope` 正确返回 McpServer。

Gateway 生成 MCP AuthorizedResource：持久化 Allow 带真实 grant ID，
one_shot=false；无匹配 Allow 的单次批准调用不带 grant ID，one_shot=true。
拒绝不生成执行授权。审计保留精确身份、decision_status 和
details.result.matched_grant_id。

Grant Editor 在现有“权限与安全”页增加 MCP 高级输入，真实复用创建、列表、
删除接口。工具名留空的 server-wide 含义、拒绝优先及 High Risk 审批规则均
在界面说明，不显示传输配置或凭据。

## 验证方法与环境

后端定向测试先证实旧实现无法解析 MCP Grant，且缺失授权被当成 Allow；
随后验证新增模型、匹配、持久化和既有资源类型回归。Gateway 用真实
McpToolAdapter。Task Harness 用真实 loopback HTTP MCP 和审批 handler。

Edge E2E 使用 REAL PRODUCT BACKEND + LOCAL MCP FIXTURE，独立临时数据目录，
唯一 graph/server/grant ID；真实 UI 创建与删除 Deny、Canvas 执行，以及真实
Task Harness 审批恢复。远端 tools/call 是计数依据。finally 删除测试授权，
runner 回收自己启动的进程与临时数据，不操作用户真实授权。

`CARGO_TARGET_DIR=E:/cargo-target/yilian/mcp-grant-u3c`，
`CARGO_PROFILE_TEST_CODEGEN_UNITS=4`，全部 Cargo 检查使用 `--locked -j1`。
E: 可用空间约 4 GB，因此该专用 target 是指向本工作区
`target/rust-build` 的目录联接，底层使用 F:。缓存为独立复制，未共享可写
文件、修改旧 target 或更改 Cargo profile。

## 编译与测试结果

| 检查 | 最终结果 | ignored target/mcp-grant-u3c/ 证据 |
|---|---|---|
| cargo fmt --all -- --check | PASS / exit 0 | fmt-final.log、rust-final-gates.json |
| cargo check --workspace --all-targets --locked -j1 | PASS / exit 0 | workspace-check.log、rust-gates.json |
| cargo test --workspace --all-targets --locked -j1 | PASS / exit 0；1009 passed / 0 failed，其中后端 lib 911 | workspace-test-final.log |
| cargo check -p yi-lian-qian-yan --locked -j1 | PASS / exit 0 | desktop-check.log |
| 原有授权与新增 MCP 定向测试 | PASS / 25 项；随后完整测试再次覆盖 | grants-focused.log |
| npm test -- --maxWorkers=4 --minWorkers=1 | PASS / exit 0；102 文件、492 项 | frontend-test-bounded.log |
| npm run build | PASS / exit 0 | frontend-build.log |
| 后端服务程序构建 | PASS / exit 0 | backend-build.log、backend-binary.json |
| U3-C + system Edge 154.0.4258.37 | PASS / exit 0 | acceptance/u3c/result.json |
| 原 U3 完整执行回归 + system Edge | PASS / exit 0 | acceptance/u3-regression/result.json |

## 安全闭环验收

| 项目 | 结果 | 依据 |
|---|---|---|
| GrantResource::Mcp | PASS | additive enum、验证、JSON roundtrip |
| Persistent roundtrip | PASS | 原 DB CRUD，关闭并重新打开 SQLite 后 get/list 一致 |
| MCP exact resource allow | PASS | 精确 Allow 仍等待审批，明确批准后执行一次 |
| MCP exact resource deny | PASS | 真实 UI 创建 Deny，Canvas publish 为 security_denied |
| Server-wide allow | PASS | A/read 通过资源匹配，High 审批后调用一次 |
| Specific deny precedence | PASS | A/all Allow + A/publish Deny，publish 调用 0 |
| Cross-server isolation | PASS | Deny A/echo，B/echo 批准后成功，A/echo 调用 0 |
| Approval current-policy revalidation | PASS | 先 pending，再创建精确 Deny，批准旧审批仍被拒绝 |
| Denied remote call count | **0** | 三个拒绝场景的调用增量均为 0 |
| New Allow does not auto-consume approval | PASS | pending 身份/状态保持，批准一次成功，重复批准 409 |
| UI create/delete/recovery | PASS | 创建与删除均从 Grant Editor 进行，删除后 publish 审批成功 |
| 用户可见拒绝原因 | PASS | 节点执行记录显示“当前安全策略或授权拒绝此操作” |
| Trusted authorization evidence | PASS | Gateway 测试检查 grant_id / one_shot_approval / High |
| Audit grant ID + decision | PASS | 与执行 ID、精确 MCP resource 和实际 grant ID 关联 |
| Secret exposure | **0** | 本轮限定表面的 URL / env / header 配置扫描 |
| Disposable grant cleanup | PASS / **0 remaining** | 保留 2 条基础授权，没有创建永久审批授权 |
| U3 regression | **PASS** | Discovery、Binding、Approval、Reject、Cancel、exactly once、Unavailable、同名路由 |
| DB migration / schema | **NO / 0 changes** | scope-audit.json；backend/src/db 整体 0 变更 |
| Canvas production / Task Harness semantics | **0 changes** | scope-audit.json |
| v2 touched | **NO** | src-tauri / Cargo / v2 与架构边界检查 |

U3-C fixture 最终累计 A=3（read、获批 echo、删除 Deny 后的 publish），B=1
（echo）。这些成功调用不计入拒绝场景；拒绝阶段的 publish、A/echo、旧审批
均增加 0 次调用。`scenarios.json` 保留各阶段执行详情及 before/after 计数。

## 证据与画布回归

证据入口：ignored `target/mcp-grant-u3c/result.json`。全部检查日志、失败运行、
最终 JSON、截图、工具调用和 SHA-256 清单保留在该目录。

- U3-C 最终运行：`2026-09-27T09-45-01-885Z`，集中副本 `acceptance/u3c/`。
- 原 U3 回归：`target/mcp-canvas-u3/2026-09-27T09-43-54-016Z`，集中副本
  `acceptance/u3-regression/`。
- 截图：`grant-editor-deny.png`、`canvas-publish-denied.png`、
  `canvas-publish-recovered.png`；已经实际查看授权表单与可见拒绝原因。
- 本次 U3 几何对比：ready / waiting-approval / rejected / succeeded 的节点
  240×128、位置及 `translate(0px, 0px) scale(1)` 相机完全一致；响应式截图
  覆盖 1280×720、1366×768、1920×1080、2560×1440。
- `geometry-comparison.json` 另将冻结 U3 与本轮 U3 的上述四个状态逐项比较，
  尺寸、位置、相机全部一致；只按 anchor / MCP 角色归一化独立测试图随机 ID，
  原始 JSON 保留真实 ID，记录旧证据 SHA-256。
- Canvas 生产文件 0 改动，完整 Stability / Studio C2 按任务书引用冻结证据：
  `F:/项目开发/忆涟千言/YiLian-v1-mcp-canvas-u3/target/mcp-canvas-u3/acceptance/core/result.json`
  与同目录 `../studio/result.json`。本轮未重复完整 C2/Stability 套件。

## 限制与历史记录

- 本轮 Grant Editor 使用任务书允许的高级 Server ID / tool 输入，未新增
  capability 下拉选择器。
- 验证对象为本地 MCP fixture，不代表外部商用 MCP 服务验收。
- Secret exposure 指本轮能力、授权编辑器、执行详情、审计中的传输 URL / env /
  header 配置扫描，不代表任意外部工具输出的全面秘密检测。
- 默认 `npm test` 初跑为 491 passed / 1 timeout；原有 TaskCenter 动态导入
  超过 5 秒。相同完整套件使用 `--maxWorkers=4 --minWorkers=1` 后为
  492 passed / 0 failed。未改变产品、断言或超时阈值。并发资源竞争是符合
  观察的解释，未做系统级性能归因。两次完整日志保留。
- MCP 定向测试首跑为 38 passed / 1 failed：新增审计断言误读字段层级。
  修正为既有 `details.result` 后由最终后端测试验证；原失败记录保留。
- Rust 完整运行中，桌面 5 项及后端 910 项通过；新增用例在最后的清理检查
  要求整个授权表为空，忽略了应用初始化的基础文件授权。已修正为前后基础
  授权完全一致，单用例复查通过。未更改产品语义，也未删除默认授权。
- 补跑剩余集成目标时，架构检查发现授权测试文件 694 行，超过 600 行限制。
  已将新增 MCP 测试拆到 `grant/tests/mcp.rs`，不修改架构阈值或豁免表。
  因测试模块组织变化，重新执行最终完整 Rust 测试；此前 exit 101 的日志
  保留，不覆盖为成功。
- 生产构建保留既有大 chunk 提示，Rust 保留既有 dead_code / q_embed 警告。
- Edge 初跑的下拉框精确标签定位超时，已修正测试定位方式；产品代码未变。
  首次通过后补充“界面可见拒绝原因”断言，仅复跑 U3-C，并再次通过。
- 本轮要求无未完成项。高级输入、外部商用服务与已有构建警告为上述明确限制；
  不扩展至 Typed Data Binding、Secret Binding、GIS 或后续版本。

完整行为契约见 [MCP Resource Grants](../security/mcp-resource-grants.md)。
原 U3 PARTIAL 历史保留于 [U3 报告](generic-mcp-canvas-u3-report.md)。

## 变更文件清单

生产代码：

- `backend/src/safety/grant/model.rs`
- `backend/src/safety/grant/evaluator.rs`
- `backend/src/safety/execution_gateway.rs`
- `frontend/src/features/security/GrantEditor.tsx`
- `frontend/src/features/security/grantEditorModel.ts`

测试与 E2E：

- `backend/src/safety/grant/tests.rs`
- `backend/src/safety/grant/tests/mcp.rs`
- `backend/src/safety/gateway_mcp_e2e.rs`
- `backend/src/modules/task/api/tests/mcp_canvas.rs`
- `frontend/src/features/security/grantEditor.test.ts`
- `frontend/e2e/core-paths.mjs`
- `frontend/e2e/mcp-canvas-u3.mjs`
- `frontend/e2e/mcp-grant-u3c.mjs`

文档：

- `docs/security/mcp-resource-grants.md`
- `docs/mcp/generic-mcp-canvas-u3c-report.md`
- `docs/mcp/generic-mcp-canvas-u3-report.md`（仅附加 Closure Addendum）
