# UI Foundation / Responsive Gate

Date: 2026-09-26. Result: **PASS**, with real Windows DPI testing **HUMAN_PENDING**.

## Baseline and delivery

- Baseline: `dea238b65094100ed5c0835a01de7fd7a57a0508` (`feat/v1-canvas-stability`).
- Branch: `feat/v1-ui-foundation-responsive`.
- Worktree: `F:\项目开发\忆涟千言\YiLian-v1-ui-foundation`.
- Verified implementation HEAD: `0ca2c2692ac212f848a5b8344c20eea2de5e808d`.
- Final HEAD: recorded after this documentation commit in [delivery-head.json](../../target/ui-foundation/delivery-head.json), including the verified remote branch SHA. A commit cannot embed its own content-derived SHA; this report identifies the tested implementation separately from its documentation-only delivery commit.
- No merge into main, develop, release, or refactor branches.

## User-visible changes

建立了统一字号、控件、页面标题、间距和桌面宽度基础，保留现有淡粉、紫、月光、昔涟配色及背景。

- 默认字体从 14px 提高到 16px。`theme/typography.json` 是字号和行高的唯一数值来源，Tailwind 在构建时输出 CSS 变量，主题默认值引用同一来源；旧主题存储不会重新应用 14px。
- `--text-secondary` 原本是颜色变量，因此字号使用 `--font-size-*`，避免破坏主题颜色。
- 基础按钮至少 36px，主要按钮至少 40px，小按钮至少 32px。主要输入 16px，其他表单控件至少 14px。导航标签 12px、图标 20px，安全/审批/恢复入口至少 32px 高。
- PageHeader 标题 24–28px、描述 14px。Chat 消息 17px / 1.65，阅读列上限 1040px。
- Chat 侧栏使用有限范围的 clamp；WorkspaceMode 的 959/960/1179/1180 边界保持原样。
- Canvas 中间区域获取主要新增空间，Inspector 限定 300–380px。节点保持 **240 × 128**，只调整字号、padding、行高和截断。
- Settings 导航 180–220px，表单上限 1180px；字段与 Provider/Secret 流程完整保留。
- 监控网格在 >=1600px 时容纳三张默认卡片；自由布局继续使用原四列坐标空间。能力列表依据卡片最小宽度增加列数。Memory/Knowledge 保留真实空状态和有意义的信息分区，不增加虚构卡片。
- 全局语音条和工作区使用共同的 overlay safe area，页面保持明确滚动区域，body/main 不随页面内容一起滚动。

## Typography: PASS

真实 system Edge 的 `getComputedStyle` 读数：

| 内容 | Baseline | Final |
| --- | ---: | ---: |
| Body | 14px | 16px |
| 导航标签（1280×720） | 8.75px | 12px |
| 页面描述 | 12.25px | 14px |
| Chat 消息 | 12.25px | 17px |
| 主要输入 | 12.25px | 16px |
| Canvas node kicker | 10px | 12px |
| Canvas node title | 12.25px | 15px |
| Canvas node summary | 9.8px | 13px |
| Canvas execution meta | 9.1px | 12px |
| Trail item | 10.5px | 14px |

语义尺度：caption 12 / meta 13 / secondary 14 / control 14 / body 16 / reading 17 / title 18, 22, 28px。最终 61 个场景中，渲染的信息文字未发现低于 12px；纯装饰和 aria-hidden 图形不计入文字检查。

## Responsive matrix: PASS

| Viewport / zoom | Pages | Result |
| --- | --- | --- |
| 1280×720 | Chat, Task Center, Canvas, Settings, Capability, System, Memory, Knowledge, Workflow | PASS |
| 1366×768 | 同上 9 页 | PASS |
| 1600×900 | 同上 9 页 | PASS |
| 1920×1080 | 同上 9 页 | PASS |
| 2560×1440 | 同上 9 页 | PASS |
| 3440×1440 | Chat, Canvas, Settings | PASS |
| 959 / 960 / 1179 / 1180 × 900 | Chat 及 WorkspaceMode 边界 | PASS |
| 125% browser zoom | Chat, Canvas, Settings | PASS |
| 150% browser zoom | Chat, Canvas, Settings | PASS |
| 200% browser zoom | 同三页 smoke；窄屏 Canvas 使用其滚动区到达镜头工具 | PASS |
| Windows DPI | 未操作真实系统显示缩放 | **HUMAN_PENDING** |

Zoom 使用隔离 Edge 配置中的临时扩展调用 `chrome.tabs.setZoom`，并用 `getZoom`、实际 innerWidth / innerHeight / devicePixelRatio 验证。基准浏览器内容区域 1180×900，125% / 150% / 200% 的布局区域分别约 944×720、787×600、590×450。未使用 CSS zoom 或 pinch scaling 替代浏览器缩放，也不把它视为 Windows DPI 验证。[官方 tabs API](https://developer.chrome.com/docs/extensions/reference/api/tabs#method-setZoom)

| 大屏测量 | 1366×768 | 2560×1440 |
| --- | ---: | ---: |
| Chat 阅读列 | 694px | 1040px |
| Canvas 中央宽度 | 737.4px | 1786px |
| Settings 表单 | 1041.4px | 1180px |
| 监控默认卡片每行 | 2 | 3 |
| 能力列表列数 | 1 | 2 |

3440 下 Chat 阅读列仍为 1040px，Settings 表单仍为 1180px；Canvas 继续扩大。CPU/Memory 只有两张真实资源卡，Knowledge 只有两个真实能力分区，未为增加列数填充虚假内容。

## Geometry and interaction gates

- Horizontal Overflow: **0 unexpected pages**。检查 document 和各页主要滚动区域的 scrollWidth/clientWidth。
- 导航、页面标题和主要操作：PASS。安全入口保留独立底部空间。
- Global Voice Pill 与 Chat composer / Settings Save / Canvas toolbar / camera controls：**no overlap**。
- Settings Save：每个 Settings 尺寸/缩放场景先记录原始模型设置页，再切换智能体设置、修改隔离测试名称，并进行启用状态的点击可达性检查。1280×720 另完成一次真实 `PUT /api/settings`，响应成功且页面显示“设置已保存”。原始无修改页面的 disabled 状态不被误报为遮挡。
- Canvas toolbar：可见、点击位置可达、目标尺寸 >=32px。
- Canvas node dimensions：每个 Canvas 场景 offsetWidth/offsetHeight 均为 **240×128**。
- 浏览器 console error / pageerror：最终完整矩阵及两页补充矩阵均为 0。

## Canvas Stability Regression: PASS

运行原有 `canvas-stability.mjs`，没有改写冻结的场景或 Canvas 状态实现。

- 12 个状态快照；Node position、Viewport、Drag、Restore、Locate、Auto Layout 通过。
- 真实 stale revision：1 次 409，重放路径通过。
- 真实本地 workflow output 执行：`succeeded`。
- 初始保存 viewport `{x:340, y:-120, zoom:1.35}` 正确恢复。
- 分组、节点新增/删除、选择及语义更新回归通过。

## Final gate

| Check | Command / evidence | Result |
| --- | --- | --- |
| Frontend tests | `npm.cmd test -- --maxWorkers=2 --minWorkers=1` | **95 files / 468 tests PASS** |
| Architecture | `src/architecture/boundaries.test.ts`，包含于上项 | **5 tests PASS** |
| Production build | `npm.cmd run build` | PASS |
| Real backend core E2E | `npm.cmd run test:e2e` | PASS；3 个真实任务图 |
| Canvas Stability E2E | 同一 core runner 调用原冻结场景 | PASS |
| Responsive matrix | `YILIAN_E2E_UI_MATRIX=final` + `node frontend/e2e/core-paths.mjs` | 61 scenarios PASS |
| Wide-grid follow-up | 再加 `YILIAN_E2E_UI_PAGES=system,capabilities` | 10 focused scenarios PASS |
| Diff whitespace | `git diff --check` | PASS |

生产入口包 **520.70kB**，保留 >500kB warning。本轮未进行 bundle splitting。

Backend changed: **NO**. Rust/Cargo changed: **NO**. Database/schema/migration changed: **NO**. REST/Secret behavior changed: **NO**. v2 touched: **NO**. Canvas state/reconcile/persistence/stale rebase changed: **NO**.

因此没有重跑 Rust 全门禁。后端复用冻结基线的已验证二进制，复制后的 SHA256 为 `AFF6732E1EE3595BA1C6A0BF3B1AFF11280B67241ADB48734594A39482F333C5`。浏览器为 system Edge **153.0.4234.48**。E2E 使用独立临时数据和端口，不修改用户数据库或凭据。

## Evidence index

均位于 ignored target，未提交截图、用户数据或运行日志：

- [Before/after geometry JSON](../../target/ui-foundation/before-after-geometry.json)：24 组可比场景及合并后的 61 场景最终矩阵。
- [Baseline: 24 screenshots + geometry](../../target/ui-baseline/2026-09-26T13-59-42-283Z/geometry.json)。采集在生产样式修改前完成。
- [Complete responsive matrix](../../target/ui-foundation/2026-09-26T14-13-24-749Z/geometry.json)。同目录包含截图、缩放方法、逐场景 enabled-save geometry、真实保存截图、日志、exit code、空 violations 数组。
- [System/Capability final focused matrix](../../target/ui-foundation/2026-09-26T14-15-48-284Z/geometry.json)。这 10 项替代合并矩阵中的对应旧记录。
- [Core / Canvas result](../../target/canvas-e2e/2026-09-26T14-17-43-810Z/result.json) 和 [12 snapshots](../../target/canvas-e2e/2026-09-26T14-17-43-810Z/canvas-snapshots.json)。
- [Frontend test log](../../target/ui-foundation/frontend-tests.log) / [build log](../../target/ui-foundation/frontend-build.log)。

原始 baseline 的 `form` 指向滚动容器，final 的 `form` 指向受限宽度的内层表单，JSON 明确注明这一测量差异；相同控件宽度可比较 `input`，没有把旧滚动区域宽度冒充旧输入框宽度。

## Failure diagnosis and scope review

基线首次采集发现首页因模型未配置而拒绝发送。改用既有 API 保存真实测试用户消息并立即停止；不伪造助手回复或模型成功结果。

完整矩阵首次失败时，保留了截图、DOM geometry、viewport/zoom、浏览器及前后端日志、exit code。诊断出 React Flow attribution 10px、disabled Save 命中检测不适用，以及 200% 下窄屏 Canvas 的多余首行高度。修复后矩阵通过。最终大屏截图复查又调整了监控网格和能力列表，只补跑这两页的 10 项检查。

新增文件：`theme/typography.json`、`e2e/ui-foundation.mjs`、本报告和实施计划。其余修改限于 CSS、主题默认字号、UI primitives、展示类名、现有主题测试及 E2E runner。Task/Workflow 事件处理、任务模型、API 和安全执行链未改动。

安全影响：扩大安全入口点击区，保留 focus-visible、keyboard、aria；不改变审批规则、执行权限或凭据流程。没有使用子智能体。

本轮停止在 Typography / Responsive / Canvas / Frontend / Real-backend gate 完成处。Windows DPI 留待人工验收；下一步等待新的任务书。
