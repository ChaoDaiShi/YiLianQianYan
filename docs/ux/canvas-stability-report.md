# Canvas Stability 验收报告

日期：2026-09-26。状态：**PASS**。本轮到此结束，等待下一任务书。

## 基线与交付

| 项目 | 记录 |
|---|---|
| Baseline | `ce3dca2f48e98fe2a334c94b41a7edd3d31a535b` |
| Baseline branch | `refactor/v1-architecture-semantic` |
| Branch | `feat/v1-canvas-stability` |
| Final HEAD | `refs/heads/feat/v1-canvas-stability`（包括本报告的 C7 收尾提交；精确哈希由 `git rev-parse HEAD` 获取，并在交付消息中记录） |
| 已验证的功能/测试 HEAD | `819e41b9917c8cc7495f6e4f91e0942ca1c4476b`；其后仅更新本报告和实施计划 |
| Worktree | `F:\项目开发\忆涟千言\YiLian-v1-canvas-stability` |
| Git | 阶段提交并推送当前分支；没有 merge、tag 或 release |

Architecture R2 冻结。本轮没有继续 R3、拆后端/Protected Kernel/facade、拆大文件、主题或字号重设计。

## 行为门禁

| 检查 | 结果 | 证据 |
|---|---|---|
| Node position | PASS | 纯函数覆盖 pending→running、running/succeeded 投影、标题和焦点变化、添加/删除；真实 API 修改语义后 DOM transform 相等 |
| Viewport | PASS | 选择、语义保存、执行刷新、文档返回、窗口 resize 均保持当前镜头 |
| Drag persistence / Drag race | PASS | 纯测试 x=100→500；浏览器真实拖动，延迟 PUT 期间语义刷新仍保持新 transform，保存后不回弹 |
| Stale rebase | PASS | 真实 API 提高服务端 view revision，使浏览器产生一次真实 409；读取最新文档、重放移动后保存成功 |
| Semantic refresh | PASS | 刷新标题，旧节点位置和镜头不变；新节点 D 加入不移动 A/B/C |
| Execution refresh | PASS | 真实本地 output 工作流达到 `succeeded`；节点 transform、240×128 卡片尺寸、镜头和轨迹顺序不变 |
| Execution Trail | PASS | 纯测试时间戳连续变化不重排；浏览器始终按 A/B/C 图顺序 |
| Auto Layout explicit-only | PASS | 只有按钮调用布局算法；先更新本地，再提交文档；布局后语义刷新不回滚，也不移动镜头 |
| Locate explicit-only | PASS | 轨迹选择与独立定位按钮分开；选择不移动，点击定位改变镜头；有 aria-label |
| Saved viewport restore | PASS | 初次恢复 `(340, -120, 1.35)`；用户平移缩放后离开再进入，恢复相同 transform |
| Group / multi-select | PASS | Ctrl 多选、创建分组、折叠/展开后位置和镜头不变；隐藏成员仍保留本地模型 |
| Keyboard / Delete | PASS | 最终完整 E2E：聚焦画布外侧 D、Enter 选择不自动平移；Delete 经真实 API 删除 D，A/B/C 保持原位 |
| React identity | PASS | 100 节点纯测试，单节点状态变化只替换该节点对象；其余 99 个引用保留；无变化保留数组引用 |

## 前端与应用验证

| Gate | 命令/方式 | 结果 |
|---|---|---|
| Focused tests | `npm.cmd test -- src/features/task-world` | 7 文件 / 42 测试通过 |
| Frontend tests | `npm.cmd test -- --maxWorkers=2 --minWorkers=1` | **95 文件 / 468 测试通过**，退出 0 |
| Architecture frontend checks | 上述全量测试中的 `src/architecture/boundaries.test.ts` | **5 项通过** |
| Build | `npm.cmd run build` | TypeScript + Vite 成功，退出 0 |
| Real-backend E2E | `npm.cmd run test:e2e` | **PASS**，退出 0；system Edge + 真实服务 + 独立临时数据 |
| Diff | `git diff --check` 与基线文件清单审查 | PASS；0 Rust / Cargo / migration 文件 |

E2E 还保留了原有任务画布创建、能力页面、监控页面和设置页检查；设置页分别验证 1280×720、1366×768、1920×1080。全场景创建三个图，其中专项图初始含 A/B/C，并临时加入再删除 D。没有使用假后端替代返回值；仅暂停一次真实浏览器 PUT 来制造竞态。冲突响应为实际服务端 409，执行结果为真实本地 output workflow 的 `succeeded`。

本次最终证据目录：

`target/canvas-e2e/2026-09-26T13-36-53-808Z/`

包含：`result.json`、`canvas-snapshots.json`、`canvas-stability.png`、`backend-stdout.log`、`backend-stderr.log`、`frontend-stdout.log`、`frontend-stderr.log`、`browser-console.json`、`task-network.json`、`ports-and-processes.json`、`exit-code.txt`、`frontend-tests.log`、`frontend-build.log`。日志/截图留在本地工作区的 ignored target 目录，不作为源代码推送。已查看最终截图，节点固定信息区、选择与定位按钮可见，连接端点未被裁剪。

关键实际记录（完整文件含 12 组快照）：

| 阶段 | A transform | viewport transform |
|---|---|---|
| 首次恢复 | `translate(40px, 140px)` | `translate(340px, -120px) scale(1.35)` |
| 拖动待保存 | `translate(100.129px, 187.941px)` | `translate(59.7727px, 198.441px) scale(1.23068)` |
| 冲突重放成功 | 与拖动待保存相同 | 与拖动待保存相同 |
| 用户平移缩放并选择 B | 与拖动待保存相同 | `translate(13.8273px, 88.2541px) scale(1.47682)` |
| B 执行成功 | 相同 | 相同 |
| 离开后重新进入 | 相同 | 相同 |
| 显式定位 C | 相同 | `translate(-490.5px, 134.1px) scale(1.35)` |
| 显式自动布局 | `translate(40px, 40px)` | 与定位后相同 |

## 保存策略与状态归属

TaskGraph 提供语义与依赖，ExecutionProjection 提供运行结果，CanvasDocument 提供初次布局与持久化视口，CanvasInteraction 持有挂载期间的交互位置、尺寸和选择。完整规则见 [行为契约](canvas-behavior-contract.md) 和 [状态归属](../architecture/canvas-state-ownership.md)。

旧实现整批 `setNodes(model.nodes)` 和 focus/nodes 驱动的 `fitView` 已移除。纯 reconciler 保留已有位置、测量和选择，只合并数据与显式分组隐藏。新节点优先使用保存布局，否则按图顺序一次性分配位置。Auto Layout 以显式命令直接传入布局，不借用服务端 revision 冒充用户指令。

现有保存队列串行写入，stale 时重放 mutation 并仅自动重试一次。第二次冲突或读取失败会保留未保存 mutation 和本地视觉位置；后续写入排队，用户点击“重试保存画布”恢复。纯测试覆盖重复冲突、最新文档读取失败，以及较旧 fetch 不能覆盖已接收的更新 SSE revision。重试入口位于画布页面，不依赖 Inspector 是否选中节点。

## Backend / Database / v2

| 项目 | 结果 |
|---|---|
| Backend changed | **NO** |
| Database changed | **NO**（应用 schema/持久化契约未改；E2E 只写独立临时数据） |
| Migration changed | **NO** |
| Cargo changed | **NO** |
| REST / Secret contracts changed | **NO** |
| v2 touched | **NO** |
| Rust gate rerun | **NO**，按任务书引用 Architecture Baseline `ce3dca2` |

后端二进制从 `YiLian-v1-architecture/target/debug/yilian-server.exe` 复制一次到当前工作区 `target/debug/yilian-server.exe`，未冷编译 Rust。源与目标大小均为 37,013,504 字节，SHA256 完全一致：

`AFF6732E1EE3595BA1C6A0BF3B1AFF11280B67241ADB48734594A39482F333C5`

## 失败诊断与修复记录

所有 E2E 失败在重跑前均保存了四份服务 stdout/stderr、浏览器日志、截图、9420/1420 端口归属、进程列表与退出码；没有把先前失败改记为通过。

| 证据目录（均在 `target/canvas-e2e/`） | 当次结果与处理 |
|---|---|
| `2026-09-26T13-26-11-035Z` | FAILED：新场景以 networkidle 等待带持续事件连接的页面；截图已渲染三个节点且无浏览器异常。改为 DOMContentLoaded + 节点/数据条件等待 |
| `2026-09-26T13-27-39-907Z` | FAILED：脚本找 `zoom in`，安装的 ReactFlow 实际 aria-label 为 `Zoom In`；本次拖动与真实冲突已通过。核对本地依赖后修正选择器 |
| `2026-09-26T13-29-24-769Z` | PASS：最初 9 组核心画布快照 |
| `2026-09-26T13-30-43-889Z`、`2026-09-26T13-32-41-090Z` | FAILED：新增键盘删除等待超时。后续追加按键诊断，没有伪称已证明是环境问题。发现 key arrays 每次 render 改变会重建监听，已固定引用；测试等待真实 DELETE 响应后释放按键 |
| `2026-09-26T13-33-41-244Z`、`2026-09-26T13-34-33-734Z` | 专项 PASS；后者带临时本地按键诊断。临时依赖修改已恢复并核对 SHA256，未进入提交。通过本身不证明此前超时的全部因果 |
| `2026-09-26T13-36-05-472Z` | FAILED：完整场景同时存在分组和语音面板的“展开”按钮，脚本选择器不唯一。限定到“画布视图工具” |
| `2026-09-26T13-36-53-808Z` | **最终完整场景 PASS**；使用已恢复的正常依赖，12 组快照，包含键盘 Delete |

前端首次全量测试与 E2E 并发时，TaskCenterPage 的首次动态导入超出 5 秒：467 项通过、1 项超时。该文件单独运行 10/10 通过；随后不与 E2E 并发，以最多两个 worker 运行完整套件，468/468 通过。没有修改任务中心代码或提高测试 timeout。

开发期构建曾发现 `FitViewOptions` 缺少节点类型泛型，已补上 `TaskGraphCanvasNode`，最终构建通过。

## 文件清单

新增：

- `docs/ux/canvas-behavior-contract.md`
- `docs/ux/canvas-stability-plan.md`
- `docs/ux/canvas-stability-report.md`
- `docs/architecture/canvas-state-ownership.md`
- `frontend/src/features/task-world/canvas/model/reconcileCanvasNodes.ts`
- `frontend/src/features/task-world/canvas/model/reconcileCanvasNodes.test.ts`
- `frontend/src/features/task-world/canvas/model/canvasViewport.ts`
- `frontend/src/features/task-world/canvas/model/canvasViewport.test.ts`
- `frontend/e2e/canvas-stability.mjs`

修改：

- `frontend/src/features/task-world/canvas/TaskWorldCanvas.tsx`
- `frontend/src/features/task-world/canvas/TaskNode.tsx`
- `frontend/src/features/task-world/TaskWorldPage.tsx`
- `frontend/src/features/task-world/TaskExecutionTrail.tsx`
- `frontend/src/features/task-world/taskGraphProjection.ts`
- `frontend/src/features/task-world/taskGraphProjection.test.ts`
- `frontend/src/features/task-world/canvasViewWriter.ts`
- `frontend/src/features/task-world/canvasViewWriter.test.ts`
- `frontend/src/features/task-world/hooks/useCanvasView.ts`
- `frontend/src/features/task-world/taskWorldCanvas.test.ts`
- `frontend/src/styles/shell.css`（仅 Task Canvas 局部样式）
- `frontend/e2e/core-paths.mjs`

## 限制与下一步

- 保存失败的修改保留在当前画布实例与队列内；本轮没有新增离线持久化或跨进程恢复机制。
- 没有新增 Restore、复杂 Undo 或跟随执行 toggle；任务图 checkpoint restore 沿用既有语义，不作为隐式 Canvas restore。
- AI Review 没有调用外部模型；其语义刷新复用已验证的 reconcile 路径。本次真实执行为本地 output 工作流，不宣称验证了模型、麦克风、桌面控制或外部业务动作。
- 100 节点用例验证对象复用，没有进行吞吐基准、长时间压力或覆盖所有键盘时序的测试。
- Vite 成功构建仍提示入口包 520.48 kB 超过 500 kB 阈值；本轮不做无关拆包。
- 保留独立 worktree 与分支，等待下一任务书；不启动 Typography、Responsive UI、UI redesign、MCP Canvas、GIS、v1.1、v1.2 或 v2。
