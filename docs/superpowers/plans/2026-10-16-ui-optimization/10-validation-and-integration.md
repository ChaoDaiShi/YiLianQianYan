# 10 工作包 Q/I：验证、候选集成与交付

## 1. 验收状态的含义

| 状态 | 允许使用的条件 |
|---|---|
| PASS | 针对本候选执行了指定步骤，结果符合要求，有命令/截图/交互记录 |
| FAIL | 实际执行后不符合要求；保留原始失败证据 |
| NOT TESTED | 没有执行、环境不具备或未拿到对应数据；不计入PASS |
| BLOCKED | 当前用例被具体依赖阻塞，写出依赖和负责人；不代表整个工作包停止 |
| N/A | 此页面确实没有该操作/状态，写明原因；不能用来跳过必需功能 |

本文的所有产品用例初始状态为 `NOT TESTED`。文档写好、源码看起来正确、旧报告通过，都不能改变这一状态。

## 2. Q1 测试数据与场景准备（P0即可开始）

优先沿用 `frontend/e2e/core-paths.mjs` 启动的隔离后端和测试工作目录。创建的数据只存在测试目录；不连接用户当前运行实例完成增改删。

| 数据 | 最小集合 | 主要用途 |
|---|---|---|
| 会话 | 空会话、长对话、代码/表格、一个可停止的测试流 | B布局、滚动、输入 |
| 任务 | draft/ready/running/waiting_approval/waiting_user/blocked/completed/failed/cancelled各一例 | C状态与操作 |
| 工作空间 | 无root_path、长中文路径、active_tasks=0、未返回active_tasks | C真实字段 |
| 工作流 | 五种节点类型，两个不同图的运行，成功/失败/等待审批 | D展示与关联 |
| 能力 | 全部状态、不同provider、长名称、空列表 | E过滤与详情 |
| 技能 | editable=true/false，短正文/长正文 | E读取编辑 |
| MCP | 专用本地测试服务就绪、禁用、测试ok=false、资源/Prompt | E真实接线 |
| 记忆 | 长短正文、现有各分类来源、保存失败一次 | F列表编辑 |
| 设置 | 未配置/已保存未测试/测试成功/失败/dirty | G状态 |
| 系统日志 | CPU/内存/磁盘样例，INFO/WARN/ERROR/DEBUG，长正文 | G指标与日志 |

有真实API可创建的使用API创建；难以稳定产生的UI状态可用Playwright请求夹具明确注入。夹具只验证呈现与交互，不冒充后端已经执行成功。例如“任务被取消”需要真实API回读证据，“取消按钮loading样式”可用受控延迟响应验证。

固定时刻示例使用 `2026-10-16T02:00:00Z`；名称使用 `ui-opt-*`；文件放测试workspace。状态样例不包含真实用户对话、文件内容、API Key或访问令牌。

## 3. Q2 与现有E2E接线

目前 `ui-foundation.mjs` 导出函数，不能直接 `node ui-foundation.mjs` 当作已运行测试。`npm.cmd run test:e2e` 执行的是core-paths，它会启动独立后端和Vite开发服务。

拟实施的接线合同：

1. Q新增 `frontend/e2e/ui-optimization/index.mjs`，导出 `runUiOptimization(context)`。
2. context固定包含 `page, backendPort, frontendPort, controlToken, evidenceDir, workspace, zoomWorker`；沿用runner已创建对象。token只在进程内使用，不写报告。
3. B–G各自模块导出 `checkChat(context)`、`checkTasks(context)`、`checkWorkflows(context)`、`checkCapabilities(context)`、`checkMemory(context)`、`checkSystem(context)`；Q总入口顺序调用，以免同一个page竞争导航。
4. 返回值为 `{ cases: [{ id, status, evidence, source }], summary }`；source只能是`real_backend`、`ui_fixture`、`browser_geometry`、`desktop`之一。
5. I在core-paths添加**拟新增**环境开关 `YILIAN_E2E_UI_OPTIMIZATION=1`。该分支在普通场景之前调用总入口；其他场景开关不得同时开启。
6. I将此模式证据目录定为仓库 `target/ui-optimization/<run-id>/`，复用现有进程清理，不新增第二套服务启动脚本。
7. 此模式需要浏览器真实缩放，复用现有uiStage的扩展与zoomWorker创建流程；不能只把deviceScaleFactor改成1.5后声称做了页面150%缩放。
8. 该模式最终应检查生产bundle：在隔离端口、测试token确定后，以这些非生产测试配置构建到隔离目录，再用Vite preview提供该目录。不要把开发服务页面标成生产包验收；旧模式继续原行为。
9. 正常业务请求不拦截，只有明确登记的失败/延迟/缺少数据用例启用夹具，结束后解除。故意失败请求的console/network错误按具体URL与次数记录，不一口气忽略全部错误。

第8步仅作用于新UI模式。命令参数来自当前安装的Vite CLI，I实施时先读本地 `node frontend/node_modules/vite/bin/vite.js --help` 确认build/preview参数，再接线；不存在参数就改成同版本实际支持的调用，不能留下无效命令。

## 4. 运行命令与前置条件

以下命令是**后续代码实施后的验收命令，本次文档任务不执行**。

```powershell
Set-Location 'F:\项目开发\忆涟千言\YiLianQianYan\frontend'
npm.cmd run test
npm.cmd run build
```

预期：测试退出0，`tsc && vite build`退出0。已有node_modules就不重复npm install；缺少依赖才按现有lockfile安装，不升级全栈。

后端测试runner需要当前基线编出的二进制。先检查：

```powershell
Set-Location 'F:\项目开发\忆涟千言\YiLianQianYan'
Test-Path target/debug/yilian-server.exe
```

仅在二进制缺失或不能确认匹配当前基线时构建现有后端：

```powershell
cargo build -p yilian-backend --bin yilian-server
```

这不表示授权改后端。若实施确实改了Rust代码，已超出当前UI范围，应先交代原因并重新确定范围；随后遵守仓库cargo fmt/fmt --check/check/test要求。

已有UI基线模式（有runner及二进制时可用）：

```powershell
Set-Location 'F:\项目开发\忆涟千言\YiLianQianYan\frontend'
$uiPreviousMatrix = $env:YILIAN_E2E_UI_MATRIX
try {
  $env:YILIAN_E2E_UI_MATRIX = 'baseline'
  npm.cmd run test:e2e
} finally {
  $env:YILIAN_E2E_UI_MATRIX = $uiPreviousMatrix
}
```

新UI模式（**Q2/I接线完成后才存在**）：

```powershell
Set-Location 'F:\项目开发\忆涟千言\YiLianQianYan\frontend'
$uiPreviousMode = $env:YILIAN_E2E_UI_OPTIMIZATION
try {
  $env:YILIAN_E2E_UI_OPTIMIZATION = '1'
  npm.cmd run test:e2e
} finally {
  $env:YILIAN_E2E_UI_OPTIMIZATION = $uiPreviousMode
}
```

在干净测试shell里运行单一场景；不要同时设置UI_MATRIX、STUDIO、PROOFLOOP等选择开关。端口默认前端1420/后端9420；端口占用时用runner已支持的 `YILIAN_E2E_FRONTEND_PORT/BACKEND_PORT` 指定空闲端口，不终止用户的现用服务。

新临时文件优先放F盘任务测试目录，可在测试进程中指定TEMP/TMP并在结束后恢复；不要改系统级环境变量。浏览器优先复用系统Edge/Chrome，不安装无关全局工具。

## 5. 视觉与窗口矩阵

| 组 | 页面 | 条件 | 通过条件 |
|---|---|---|---|
| 全页基线 | 原稿14页+任务画布/搜索/账号/初始化，共18种页面 | 1366×768，浅/深各一次 | 首屏层级清楚，无页面级横向溢出，主操作可见 |
| 最小常用窗口 | 聊天、任务、工作流、插件、设置、日志 | 1280×720 | 详情/保存/关闭/停止/批准不被遮挡 |
| 宽屏 | 聊天、任务画布、工作流、能力、设置 | 1920×1080与2560×1440 | 主阅读区有界，画布实际利用空间，侧栏不过宽 |
| 聊天断点 | 聊天 | 959、960、1179、1180宽，高900 | mode与布局精确匹配，入口不重复 |
| 真实浏览器缩放 | 聊天、任务详情、设置、日志 | 125%、150%、200% | 文字完整、主要操作可触达，缩放方法被记录 |
| 系统模式 | 聊天、设置 | theme=system，模拟light/dark | 跟随系统且不改用户选项 |
| 背景图 | 聊天、设置、审批浮层 | 明亮/暗色高细节背景 | 正文、表单、风险文案清楚 |
| 减少动效 | 对话输出、Drawer、Modal、运行状态 | prefers-reduced-motion=reduce | 无循环位移/缩放，状态文字仍在 |
| Windows桌面 | 聊天、设置、浮层 | 实际Tauri窗口与100%/125%/150%DPI | 标题拖拽、焦点、缩放、原生文件操作有实际证据 |

18种页面：chat、tasks、workspaces、workspace-detail、workflows、agents、skills、plugins、capabilities、memory、knowledge、system、logs、settings、task-world、search、account、setup。chat空态/有消息是同一页面的不同状态。

账号/初始化/搜索只回归已有功能；不因公共UI检查去注册真实账号、切换用户身份或修改现用配置。别名 `/sync`、`/privacy` 验证重定向到账号对应section。

## 6. 用例索引与最短操作

各编号的细节要求见对应分册；本表供验收者逐个勾选，不能仅写“页面测试通过”。

| ID | 操作 → 观察 |
|---|---|
| A-01 | 切浅/深+背景图 → L1/L2/L3和正文可区分 |
| A-02 | 连点保存并延迟响应 → 一次请求、按钮宽度差≤1px |
| A-03 | Tab经过控件并制造表单错误 → 焦点可见、错误与输入关联 |
| A-04 | 打开两层浮层按Esc → 仅顶层关闭、焦点返回 |
| A-05 | 鼠标/键盘调整Drawer → 范围正确，窄窗不越界 |
| A-06 | 聚焦窗口边缘图标 → Tooltip完整且位置正确 |
| A-07 | 开启减少动效 → 状态可读，位移/循环停用 |
| A-08 | 点击全部导航及快捷键 → 正确路由、当前项唯一 |
| A-09 | 720高窗口滚导航 → 底部设置/账号仍可到达 |
| A-10 | 加载、空、失败 → 页面标题稳定、状态文案不同 |
| B-01 | 改到四个边界宽度 → full/compact/narrow正确 |
| B-02 | 展开/收起两栏 → 状态与焦点正确，滚动保留 |
| B-03 | 渲染长文/代码/表格 → 消息可读、仅内容内部横滚 |
| B-04 | 键盘复制/删除指定会话 → 只作用当前记录 |
| B-05 | 中文合成、Shift+Enter、双击发送 → 无误发/重复请求 |
| B-06 | 让附件或发送失败 → 文本与附件保留 |
| B-07 | 展开语音控件 → 发送/停止仍可点击 |
| B-08 | 流式时上翻再回最新 → 不抢滚，显式恢复跟随 |
| B-09 | 展开工具与执行详情 → 标识对应同一条事件 |
| B-10 | 批准/拒绝一次 → resolving到真实决定，未新增授权范围 |
| B-11 | 执行成功但验证失败 → 明确显示验证失败 |
| C-01 | 从任务中心走各入口 → 目标/计划/任务图仍可用 |
| C-02 | 组合过滤任务/会话 → 计数与来源不混淆 |
| C-03 | 开关详情并调整窗口 → 筛选、选中、滚动保留 |
| C-04 | 切详情三页签 → 原资源/成果/候选/决定入口齐全 |
| C-05 | 重试或取消 → 只发一次原API，结果真实回读 |
| C-06 | 快速选任务A再B → B标题下没有A产出 |
| C-07 | 查看四种空间数据样例 → 不伪造路径或数量 |
| C-08 | 空间创建失败/成功 → 草稿保留/真实刷新 |
| C-09 | 复制省略显示的路径 → 剪贴板为完整原值 |
| C-10 | 在指定空间创建任务 → 返回workspace_id一致 |
| C-11 | 选节点与点击定位 → 只有显式定位移镜头 |
| C-12 | 切目标/专家视图并保存 → 原图定义保持 |
| D-01 | 模板/runtime/新建/编辑/返回 → 入口完整 |
| D-02 | 小窗口和200%缩放 → 保存/运行/返回可用 |
| D-03 | 五类节点编辑保存 → 语义与返回图正确 |
| D-04 | 缩放/适应后保存 → 业务图不混入视图缩放 |
| D-05 | hover日志/等待轮询 → 镜头保持 |
| D-06 | 点击定位并切换run → 对应图与节点准确 |
| D-07 | 审批/取消 → 原流程保持、状态准确 |
| D-08 | 运行失败 → 错误可读，编辑草稿保留 |
| E-01 | 长角色名/不同能力数量 → 不溢出、数量准确 |
| E-02 | 创建角色再重载 → ID真实、团队入口保留 |
| E-03 | 打开只读与可编辑技能 → 操作权限与editable一致 |
| E-04 | 长正文保存失败 → 内容保留、复制无行号 |
| E-05 | 保存技能再读取 → 内容真实持久化 |
| E-06 | enabled=true但非ready → 没有已连接绿灯 |
| E-07 | 测试接口返回ok=false → 明确失败 |
| E-08 | 打开资源/Prompt → 原预览结果可读 |
| E-09 | 配置秘密输入 → 报告/截图不含值 |
| E-10 | 组合三组过滤与搜索 → 枚举完整、结果正确 |
| E-11 | 列表/卡片切换 → 数据、过滤、选择一致 |
| E-12 | 打开不可用能力管理入口 → 不自动执行工具 |
| F-01 | 长短记忆与URL → 正文完整可读 |
| F-02 | 分类/来源/搜索再清除 → 结果恢复 |
| F-03 | 增改失败 → 原输入保留 |
| F-04 | 保存后重进 → 展示实际内容 |
| F-05 | 删除失败 → 记录仍在 |
| F-06 | 查看知识库说明 → 无伪索引/假数量 |
| F-07 | 前往记忆再返回 → 路由与状态正常 |
| G-01 | 六组/旧section深链 → 路由映射完整 |
| G-02 | 模式/背景切换重载 → 预览与界面一致 |
| G-03 | 修改/放弃/保存成功失败 → dirty准确、草稿不丢 |
| G-04 | 保留/替换/清除测试密钥 → 原语义保持 |
| G-05 | provider测试成功/失败 → 真实结果，未保存不冒充已保存 |
| G-06 | 展开语音与保存栏 → 两个操作区不重叠 |
| G-07 | 系统指标样例 → 字段和单位准确、无假网络/GPU利用率 |
| G-08 | 加载后请求失败 → 上次数据和当前错误可区分 |
| G-09 | 日志级别/来源/搜索 → 可见结果准确 |
| G-10 | 暂停/继续 → 网络请求停止/恢复 |
| G-11 | 上下键检视/复制 → 当前行正确、全文完整 |
| G-12 | 清空/上滚/回最新 → 真实清空范围、滚动不抢夺 |

共72项功能用例。窗口矩阵是额外覆盖条件，不等于每项都要穷举全部组合；每页先做浅深标准窗口，关键交互再覆盖小窗和缩放。

## 7. 可直接实现的浏览器断言示例

以下代码使用当前已有playwright-core、node:assert，不引入新测试框架。具体页面导航和隔离数据由Q1准备。

```js
import assert from "node:assert/strict";

export async function assertNoPageOverflow(page) {
  const sizes = await page.evaluate(() => ({
    content: document.documentElement.scrollWidth,
    viewport: document.documentElement.clientWidth,
  }));
  assert.ok(sizes.content <= sizes.viewport + 1,
    `page overflow: ${sizes.content} > ${sizes.viewport}`);
}

export async function assertButtonStableWidth(page, button, trigger) {
  const before = await button.boundingBox();
  assert.ok(before, "button must be visible");
  await trigger();
  const element = await button.elementHandle();
  assert.ok(element, "button must remain mounted");
  await page.waitForFunction(
    node => node.getAttribute("aria-busy") === "true",
    element,
    { timeout: 5000 },
  );
  await element.dispose();
  const after = await button.boundingBox();
  assert.ok(after, "busy button must remain visible");
  assert.ok(Math.abs(before.width - after.width) <= 1,
    "loading changed button width");
}
```

调用此断言时，夹具需让该请求保持pending直到宽度检查结束，trigger只完成点击而不等待请求完成。busy等待限定到传入button本身；宽度稳定不是唯一判定，还需通过网络记录确认请求只发一次。

DOM没有横向溢出不代表所有内部区块都可见；还要检查主要按钮的boundingBox与实际点击命中，尤其语音悬浮控件遮挡。

## 8. 性能与可访问性检查

- 同一硬件、相同数据比较优化前后，不报无测量依据的“提升30%”。
- 流式文本不对每个token重播进场动画；浏览器Performance中检查连续输入/流式期间是否出现新增的可复现长任务。
- 500条当前日志、200条任务/能力的列表可滚动，暂停/搜索仍能响应；先用真实交互记录判断，不默认引入虚拟列表库。
- 页面初始化不新增重复API请求；公共UI组件不能自己读取业务数据。
- tab顺序与视觉阅读顺序一致；所有主要动作可用键盘完成；图标按钮有名称，错误文本与控件关联。
- 正文以4.5:1、较大文字/组件边界以3:1作为本轮可读性验收目标；透明背景需按合成后的实际像素检查，不能只比较token色值。这里是项目目标，不宣称通过某项无障碍认证。

## 9. 证据文件格式

拟输出结构：

```text
target/ui-optimization/<run-id>/
  environment.json
  cases.json
  summary.md
  before/
  after/
  interactions/
  network-redacted.json
  console-redacted.json
  geometry.json
  desktop/                         只有实际桌面验证后才写入
```

`<run-id>`是实际时间戳，不是让执行者保留原样的占位路径；例如 `2026-10-16T10-30-00Z`。截图名称使用 `B-08-chat-dark-1366x768-after.png`。

单条记录必须包含：id、候选SHA、工作树差异说明、状态、数据来源、操作、预期、实际、证据相对路径。示例是格式说明，**不是本轮PASS**：

```json
{
  "id": "B-08",
  "status": "NOT TESTED",
  "source": "ui_fixture",
  "steps": ["打开隔离会话", "开始测试流", "向上滚动", "继续输出", "点击回到最新"],
  "expected": "上翻后不抢滚，显式点击后恢复跟随",
  "actual": "本次仅编写文档，尚未执行",
  "evidence": []
}
```

基线与优化后须使用同一数据、模式、尺寸、缩放和滚动位置；每组图片附一句说明“改善了什么”。截图只是视觉证据，不能独立证明发送、保存、取消或文件打开。

## 10. I集成步骤与失败处理

- [ ] 查看各包修改范围，核对是否有未授权的API、后端、依赖、主题体系改动。
- [ ] 先集成A公共参数及CSS入口，再按B–G已完成顺序接入；集成到候选不等于获准merge受保护分支。
- [ ] 使用git diff检查CSS声明和原入口保留情况；检查同义按钮重复、多个global keydown和浮层层级。
- [ ] 每包运行自己的相关检查与一次生产构建；候选合齐后再执行全量测试和Q矩阵，不对未变化包反复跑全部测试。
- [ ] 失败时保存日志/截图/候选标识，记录操作、预期、实际，交给文件所有者窄修。
- [ ] 修复后先重跑原失败用例，再跑直接受影响范围；不要先改断言接受错误行为。
- [ ] Tauri窗口检查与浏览器结果分列；无真实Windows证据时桌面项保持NOT TESTED。
- [ ] 交付报告列完成包、失败/未测项、命令结果、截图、功能变化与后续建议。

UI代码本身不需发布到生产才能供审阅。本计划没有自动提交、推送、合并或安装新发行包的授权。
