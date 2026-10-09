# Stable Studio C2 验收报告

日期：2026-09-26。结论：本轮前端重构及要求的自动验证 PASS。

- 工作目录：`F:\项目开发\忆涟千言\YiLian-v1-ui-foundation`
- 分支：`feat/v1-canvas-studio-c2`
- 起点：`e98254c6c1fa08a475984d676bc3e20ea9031657`
- 最终实现提交：`4c13600`；本报告另以文档提交收尾。
- 全程单代理、本地提交，未推送、未合并，未进入下一轮。
- 原始仓库 `YiLianQianYan` 的 checkout 未改动；复用已结束上一阶段的干净工作区。

## 交付效果

1. 顶栏收至 60px，保留返回、真实图标识、视图保存状态和面板入口。现有协议没有独立图名称编辑能力，因此显示 graph ID，不增加改名 API。
2. 画布填满剩余工作区，用不透明、随明暗主题变化的底色和低对比点阵，消除背景插画对图内容的干扰。
3. 缩小、缩放读数、放大、Fit、100%、显式 Auto Layout、添加节点与运行选中节点集中到底部左侧。移除常驻缩略图和分散控制条。
4. 节点保持 240×128，统一图标、标题、类型/执行器、两行摘要和执行信息。入口任务、结果、审批、人工确认和 MCP 预留形态以图标与轻量色彩区分。
5. 辅助面板默认关闭，同一时刻只打开一个。面板浮在画布上，不挤压视口；折叠保持挂载和属性草稿。原有编辑、依赖、执行、版本、审查和分组操作保留。
6. 连线细化至 1.25px，保留方向箭头，选中时强化。点击节点只选择；定位仍在执行轨迹中显式触发。
7. 发布和 MCP 参数/能力/动作区域明确标注未接入且禁用；没有任何假发布、MCP 调度或新后端节点类型。

设计依据见 [UI/UX 说明](canvas-studio-c2-design.md)，稳定性约束见 [行为契约](canvas-studio-c2-contract.md)。

## 几何对比

相同真实后端 fixture、相同保存视口与世界坐标；比较默认工作区，after 的辅助面板关闭。面积指画布 CSS 包围矩形，未扣除浮动工具条，不代表性能指标。

| 窗口 | Before 画布宽×高 | After 画布宽×高 | 面积增加 |
| --- | ---: | ---: | ---: |
| 1280×720 | 654.81×496.75 | 1200×660 | 143.5% |
| 1366×768 | 737.38×542.16 | 1286×708 | 127.8% |
| 1920×1080 | 1180.41×841.92 | 1840×1020 | 88.8% |
| 2560×1440 | 1786×1201.92 | 2480×1380 | 59.4% |

四组的节点世界坐标、240×128 尺寸及镜头 `translate(20px, 0px) scale(1)` 完全一致。屏幕坐标随画布容器移至顶栏下方而变化，不是重新布局。

## 自动验证

| 检查 | 结果 | 证据 |
| --- | --- | --- |
| 前端全量测试 | PASS，96 文件 / 471 测试 | `target/canvas-studio/verification/frontend-tests.json` |
| 保存后 Escape 修正的定向测试 | PASS，9 文件 / 50 测试，包含 task-world 与 architecture boundaries | `target/canvas-studio/verification/final-focused-tests.json` |
| 最终生产构建 | PASS，TypeScript + Vite | `target/canvas-studio/verification/final-production-build.log` |
| 原有 core + Canvas Stability E2E | PASS，3 个真实图、12 个快照、1 次真实 stale 409、真实 workflow succeeded | `target/canvas-e2e/2026-09-26T15-07-19-869Z/result.json` |
| C2 Studio E2E | PASS，4 种桌面宽度，以及保存、执行、面板、明暗与语音浮层检查 | `target/canvas-studio/2026-09-26T15-10-31-045Z/result.json` |
| before/after 世界几何一致性 | PASS，4 组自动断言 | `target/canvas-studio/verification/comparison.json` |
| Git whitespace / 范围检查 | PASS，受保护实现路径差异为空 | `target/canvas-studio/verification/protected-paths.diff.txt` |

全量测试在主实现完成后执行；最后仅修正 Escape 事件范围，再跑相关 50 项、完整生产构建和扩展的 Studio E2E。没有反复运行无关全量矩阵。

现有 Canvas Stability 的断言未删减，只增加三处显式打开轨迹/更多面板的导航步骤。覆盖：语义刷新位置稳定、延迟保存期间继续更新、真实 stale_view_revision 重放、拖动保存无回弹、用户平移缩放、鼠标/键盘选择不移镜头、真实执行尺寸不变、离开重进恢复、显式 Locate / Auto Layout / 100% / Fit、分组折叠展开及增删节点。

C2 新增验证在四个尺寸逐一检查：无横向溢出、顶栏高度、默认收起、7 个工具按钮的屏幕范围与中心命中测试、选中不打开面板、面板互斥、草稿保留、Escape 返回焦点、属性保存按钮可达、MCP/发布入口禁用、面板操作不改变节点或镜头。随后通过真实 PUT 的延迟放行验证 saving→synced，通过工具条启动本地工作流并等待后端 succeeded，再通过属性保存的真实 PUT 验证保存后 Escape。

最后通过设置页真实“夜间”单选控件和真实全局语音入口验证 1280×720：暗色节点前景 `rgb(243,239,248)`、背景 `rgb(37,32,50)`，语音浮层与工具条不相交。语音 Provider 不可用时保留真实“语音暂不可用”，未声称完成语音能力验证。

浏览器为系统 Microsoft Edge **153.0.4234.48**，headless。后端复用原有 `target/debug/yilian-server.exe`，SHA256：`AFF6732E1EE3595BA1C6A0BF3B1AFF11280B67241ADB48734594A39482F333C5`。每次使用隔离临时数据目录、工作目录和控制会话，结束后仅清理本次运行的子进程与临时数据。没有修改用户数据库或凭据。

## 证据索引与复现

所有大文件均保留在被 Git 忽略的 target 中。

- Before：`target/canvas-studio/2026-09-26T14-53-57-098Z/`，四张 `before-WIDTHxHEIGHT.png` 与原始 geometry/result JSON。
- 最终 After：`target/canvas-studio/2026-09-26T15-10-31-045Z/`，四张 `after-WIDTHxHEIGHT.png`、各尺寸 inspector/trail/tools 截图、`after-real-execution.png`、`after-dark-voice-1280.png`、`studio-geometry.json`、`studio-checks.json`、`real-execution.json`、网络/控制台/进程日志和退出码。
- 稳定性：`target/canvas-e2e/2026-09-26T15-07-19-869Z/`，12 快照、结果、截图和实际请求记录。
- 汇总：`target/canvas-studio/verification/`，测试 JSON、构建日志、几何比较、变更清单、范围检查与最终证据 manifest。
- 排错证据：`target/canvas-studio/2026-09-26T15-09-13-536Z/` 保留了属性保存后的 Escape 失败；最终运行记录 `focus_after_save: BODY` 且关闭/返回焦点成功。

从本工作目录执行：

```powershell
npm.cmd --prefix frontend test -- --maxWorkers=2 --minWorkers=1
npm.cmd --prefix frontend run build
node frontend/e2e/core-paths.mjs
$env:YILIAN_E2E_STUDIO='after'
node frontend/e2e/core-paths.mjs
Remove-Item Env:YILIAN_E2E_STUDIO
```

需要已有的兼容后端可执行文件、已安装的前端依赖和系统 Edge。C2 不安装全局软件、不重编译 Rust。Before 证据已在修改前采集，应保留原始文件，不能把修改后的代码当 Before 重跑。

## 变更文件清单

| 文件（相对仓库） | 类型与作用 |
| --- | --- |
| `frontend/src/features/task-world/TaskWorldPage.tsx` | 修改：Studio 编排、面板焦点、原有操作接线 |
| `frontend/src/features/task-world/StudioHeader.tsx` | 新增：薄顶栏与真实视图保存状态 |
| `frontend/src/features/task-world/canvas/StudioToolbar.tsx` | 新增：底部浮动工具条 |
| `frontend/src/features/task-world/canvas/TaskWorldCanvas.tsx` | 修改：面板容器、工具条、箭头与入口角色上下文 |
| `frontend/src/features/task-world/canvas/TaskNode.tsx` | 修改：固定尺寸流程卡片 |
| `frontend/src/features/task-world/canvas/nodePresentation.ts` | 新增：展示类型映射，不增加执行能力 |
| `frontend/src/features/task-world/canvas/nodePresentation.test.ts` | 新增：节点类型优先级与 MCP 未接入边界测试 |
| `frontend/src/features/task-world/canvas/studio.css` | 新增：仅作用于 Studio 的布局、面板、节点与主题样式 |
| `frontend/src/features/task-world/hooks/useCanvasView.ts` | 修改：订阅现有队列完成/错误通知，提供保存中状态 |
| `frontend/src/features/task-world/inspector/TaskInspector.tsx` | 修改：原有配置分节折叠 |
| `frontend/src/features/task-world/inspector/ExtensionSlots.tsx` | 新增：禁用的 MCP 能力/参数/发布结构 |
| `frontend/src/features/task-world/taskCorePaths.test.tsx` | 修改：跟踪移动后的添加节点入口及接线 |
| `frontend/src/features/task-world/taskWorldCanvas.test.ts` | 修改：契约源范围包含提取的工具条 |
| `frontend/e2e/canvas-studio.mjs` | 新增：真实 fixture、几何截图与 C2 交互验收 |
| `frontend/e2e/canvas-stability.mjs` | 修改：仅三处显式面板导航 |
| `frontend/e2e/core-paths.mjs` | 修改：Studio 场景分支、失败诊断与浏览器元信息 |
| `docs/ux/canvas-studio-c2-design.md` | 新增：现状、设计取舍与实施清单 |
| `docs/ux/canvas-studio-c2-contract.md` | 新增：画布行为与验证契约 |
| `docs/ux/canvas-studio-c2-report.md` | 新增：本报告 |

## 限制与范围

- **预期未实现**：真实发布、MCP 发现/调用/参数保存、uDig/GeoServer 业务；预留入口均明确禁用。
- **保留现有语义**：运行入口执行当前节点，不是完整工作流发布器或新的图级调度器。长摘要限制为两行，完整信息在属性面板中。
- **保留的构建警告**：主包 520.80 kB（gzip 163.51 kB），超过 Vite 500 kB 提示阈值。本轮未做无关 bundle 拆分。
- **未验证**：原生 Tauri/安装包、Windows OS DPI 缩放、超大图性能、触屏、真实外部 MCP/发布或语音服务。桌面浏览器视口验证不替代这些测试。
- **范围核对**：Backend、Cargo、数据库 schema/migration、REST API 层、Secret、v2、依赖清单均未修改。`canvasViewWriter.ts`、`canvas/model/*`、`taskGraphProjection.ts` 也保持基线内容。
- **未完成项**：本轮约定的前端交付与自动验收无待修复失败。视觉喜好仍可由用户对照真实截图评审，但不再自动进入下一轮改造。
