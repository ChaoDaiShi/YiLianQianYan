# 忆涟千言 UI 优化与页面升级方案

> **版本：** v1.0（方案稿，未实施）
> **日期：** 2026-10-18
> **基线：** `develop` @ `7df7d97`（`fix(ui): restore theme-adaptive tokens and resolve tooltip-panel interaction conflicts`），工作树无未提交修改
> **主题约束：** **冻结「昔涟 · 涟漪（Cyrene Ripple）」视觉身份**。不新增主题、不改变品牌色相、不改变材质语言；本方案的一切改动都必须在现有昔涟 token 内完成。
> **读者：** 产品负责人、前端实现智能体、验收人
> **状态说明：** 本文是实施前方案，不包含代码实施。文中引用的行号以基线提交为准；实施时以当时的真实源码为准重新核对。

---

## 目录

- [0. 一页摘要（先读这页）](#0-一页摘要先读这页)
- [1. 背景、基线与前两轮的关系](#1-背景基线与前两轮的关系)
- [2. 主题冻结契约](#2-主题冻结契约)
- [3. 现状诊断](#3-现状诊断)
- [4. 全局层升级 G1–G12](#4-全局层升级-g1g12)
- [5. 外壳层升级 S1–S7](#5-外壳层升级-s1s7)
- [6. 公共组件升级 C1–C15](#6-公共组件升级-c1c15)
- [7. 页面升级 P1–P9](#7-页面升级-p1p9)
- [8. 跨页一致性规范](#8-跨页一致性规范)
- [9. 实施计划](#9-实施计划)
- [10. 验收与证据](#10-验收与证据)
- [11. 风险、回滚与开放问题](#11-风险回滚与开放问题)
- [附录 A 冻结 token 全表](#附录-a-冻结-token-全表)
- [附录 B 派生 token 与新增 token 提案](#附录-b-派生-token-与新增-token-提案)
- [附录 C 组件 API 变更清单](#附录-c-组件-api-变更清单)
- [附录 D 页面 → 文件 → 工作包映射](#附录-d-页面--文件--工作包映射)
- [附录 E 快捷键、焦点与键盘规则](#附录-e-快捷键焦点与键盘规则)
- [附录 F 状态与文案规范](#附录-f-状态与文案规范)
- [附录 G 合同测试影响清单](#附录-g-合同测试影响清单)
- [附录 H 可安全执行的样式治理清单（W0 输入）](#附录-h-可安全执行的样式治理清单w0-输入)
- [附录 I 页面验收清单（可直接复制进任务卡）](#附录-i-页面验收清单可直接复制进任务卡)
- [附录 J 页面 × 组件映射（升级后）](#附录-j-页面--组件映射升级后)

---

## 0. 一页摘要（先读这页）

### 0.1 这份方案要解决什么

忆涟千言的前端已经完成两轮工作：

1. **v0.9 主题基础（2026-08）**：昔涟浅/深色、跟随系统、语义 token、共享组件、导航分组、全局背景与首页视觉。
2. **全状态微交互轮（2026-10-16）**：页面骨架、四层表面、全状态、键盘与验收口径，并已按工作包 A–G 落地。

这两轮解决的是「每个页面有没有做到位」。本轮解决的是**跨页面、跨组件、跨状态的结构性问题**：

- 页面外壳有 4 套方言（扁平页头 / 圆角页头 / cloud 页 / setup 页），同一产品看起来像几个不同产品；
- 公共组件缺 14 类能力，导致 35 处原生 `<select>`、7 处手写搜索框、4 处误用 `aria-selected`；
- 暗色模式下仍有硬编码浅色表面，聊天正文、审批卡、进度卡、Toast、命令面板在夜间不可读；
- 导航有 3 套真值（`navGroups.ts` / `productPreferences.ts` / `NavRail` 硬编码 + `CommandPalette` 硬编码），出现「隐藏的页面仍能进入」「搜索归属矛盾」「为通过测试塞入不可见按钮」；
- 信息架构存在堆叠：任务中心首屏三段堆叠、智能体页三段堆叠、监控页工具箱与指标重复、记忆页三处分类入口；
- 流式消息逐 token 解析 Markdown、逐 token smooth 滚动，长会话会卡；
- 安全入口（审批、急停）被藏在 `.sr-only` 里，与 AGENTS.md §14 的方向相反。

### 0.2 本轮五条主线

| 主线 | 内容 | 结果 |
|---|---|---|
| **L1 视觉身份冻结** | 冻结 58 个昔涟语义 token × 明暗两套；清除所有非白名单硬编码色；暗色模式 100% 走 token | 主题风格不变，夜间可用 |
| **L2 页面骨架统一** | 新增 `PageShell`，18 个路由页统一页头、内容宽度、滚动所有权、安全区 | 一个产品，四套方言归零 |
| **L3 组件体系补齐** | 新增 14 个公共组件；升级 10+ 个现有组件；删除 3 类死样式/死代码 | 页面只组合，不复制 |
| **L4 状态与信息层级** | 全状态矩阵、真实状态、审批前置、任务/能力/监控页去堆叠、键盘可达 | 状态不撒谎、操作可发现 |
| **L5 性能与可访问性** | 流式渲染降频、滚动策略、轮询合并、焦点管理、aria、快捷键提示 | 长会话不卡、键盘可用 |

### 0.3 本轮明确不做

- 不新增主题、不恢复多主题系统、不改变 `cyrene-ripple` 预设的任何色值。
- 不替换 React / Vite / Tailwind / Zustand / Tauri 技术栈。
- 不新增 UI 框架、动画库、图表库、虚拟滚动库、日期库。
- 不修改后端、API、SSE 事件语义、Store 持久化结构、审批与安全边界。
- 不重写 Agent 执行循环、任务画布引擎、工作流编辑器数据模型。
- 不新增路由；不删除任何现有页面入口。
- 不为了「看起来高级」引入大面积模糊、粒子、渐变流光、常驻动画。

### 0.4 交付物

1. 本方案文档（本文）。
2. 后续实施时的 10 个工作包（W0–W9），见 [§9](#9-实施计划)。
3. 验收证据目录 `target/ui-upgrade/<run-id>/`（截图、几何、对比度、构建与测试日志）。

### 0.5 量化目标

| 指标 | 当前 | 目标 |
|---|---|---|
| 组件/页面层非 token 硬编码 hex | 约 76 处 | ≤ 12 处（仅装饰渐变白名单） |
| 非 token 色值字面量（含 CSS） | 152 处（hex 77 + rgba 158，扣除 83 处 token 定义） | ≤ 30 处，且全部登记为装饰用途 |
| 未定义 CSS 变量被引用 | 12 个 token / 39 处引用 | 0 处 |
| `base.css` 与 `presets.ts` 同名 token 值漂移 | 17 处 | 0 处（测试强制相等） |
| 页面外壳方言 | 4 套 | 1 套（`PageShell`）；Setup 全屏页保留独立但共用 token |
| 卡面视觉体系 | 5 套 | 2 套（`Panel` 内容卡 / `Section` 结构卡） |
| 原生 `<select>` | 35 处 | 至少 20 处替换为 `Select`，其余登记豁免 |
| 手写搜索框 | 7 处 | 7 处全部替换为 `SearchField` |
| 暗色下不可读的硬编码浅色表面 | ≥ 8 处 | 0 处 |
| 浅色下焦点环/强调文字对比度 | 焦点 2.12:1、accent 文字 2.12:1、faint 2.88:1 | 可交互焦点 ≥ 3:1；正文 ≥ 4.5:1 |
| 可聚焦但不可见的元素 | 3 个 sr-only 按钮 | 0 个 |
| 审批到达路径 | 需展开执行轨 / 抽屉 | 1 次点击，且在会话内可见提示 |
| 流式渲染 | 每 token 全量 Markdown + smooth scroll | rAF/时间片降频；仅在近底时平滑滚动 |
| 未分层 CSS 文件 / `!important` | 16 个 / 56 处 | 全部纳入层或限定作用域；`!important` 减少 ≥ 50% |
| 字面量字号 | 109 处（46% 的 font-size 声明） | 全部收敛到 typography token 或 4 个固定档 |
| 合同测试 | 18 个文件 | 18 个文件全部通过；不削弱任何行为断言 |

---

## 1. 背景、基线与前两轮的关系

### 1.1 已有实现（本轮必须继承，不重复造）

| 能力 | 位置 | 本轮处理 |
|---|---|---|
| 昔涟日/夜主题与跟随系统 | `frontend/src/theme/*`、`frontend/src/styles/base.css` | 冻结并补暗色缺口，不重做 |
| 语义 token 层（58 键） | `frontend/src/theme/presets.ts:212-338` | 冻结值；仅新增派生键 |
| 共享组件（Button/Input/Textarea/Panel/PageHeader/Badge/Modal/Drawer/Tooltip/EmptyState/ErrorState/Skeleton/Spinner） | `frontend/src/components/ui/` | 升级，不替换 |
| CommandPalette / CyreneToast | `frontend/src/components/ui/CommandPalette.tsx`、`CyreneToast.tsx` | token 化 + 接入 + 可访问性 |
| 全局背景与首页场景 | `AppShell.tsx`、`workspace.css`、`WorkbenchHome.tsx` | 保留艺术层，合并内联/硬编码 |
| 导航分组与偏好 | `navGroups.ts`、`productPreferences.ts`、`ModuleSetup.tsx` | 合并成单一注册表 |
| 执行轨迹 / 审批四档风险 | `features/execution/*`、`components/approval/ApprovalCard.tsx` | 修正状态判定 + token 化 |
| 设置六组 / 监控卡 / 日志键盘 | `pages/SettingsPage.tsx`、`SystemPage.tsx`、`LogsPage.tsx` | 结构升级 |
| 任务画布 / 工作流画布 | `features/task-world/*`、`components/workflow/*` | 只做视觉与布局外壳，不动数据模型 |
| 合同测试与 E2E 基建 | `frontend/src/**/*Contract.test.ts`、`frontend/e2e/*` | 复用并扩充 |

### 1.2 与 2026-10-16 计划的关系

- 上一轮的 **A–G 工作包和 01 设计契约**仍有效：页面边距、间距刻度、字体层级、半径、动效合同、全状态矩阵、响应式合同、浮层与键盘次序，本轮全部继承。
- 上一轮遗留、本轮补做的部分：`PageHeader` 双套样式、公共组件缺件、暗色硬编码、导航三套真值、`--shadow-sm` 等变量缺失、`animate-in` 依赖缺失。
- 本轮**不重新讨论**：四层表面模型、品牌色值、字体基线、消息阅读宽度 860px、导航宽度 68/80px、右执行轨默认收起。这些已被合同测试或冻结文档固化。

### 1.3 不可违背的工程约束

- 只在 `develop` 衍生的特性分支工作，建议 `feat/ui-page-upgrade`；不直接改 `main`。
- 每次改动小步提交：`feat(ui-shell): ...`、`refactor(ui-tokens): ...`、`fix(chat-dark): ...` 等。
- 前端改动必须通过 `cd frontend && npm.cmd test && npm.cmd run build`。
- 不删除、不弱化合同测试中的断言；如需修改，必须说明行为变化并同步文档。
- 任何会改变页面类名（如 `capability-page skills-page page-canvas`）的结构改造，先核对 `*Contract.test.ts` 与 `*FreezeContract.test.ts`。

---

## 2. 主题冻结契约

> 这一节是整份方案的「宪法」。任何实现如果与本节冲突，以本节为准。

### 2.1 冻结内容（不可变）

**A. 品牌色相（浅色 / 深色）**

| 语义 | 浅色 | 深色 | 说明 |
|---|---|---|---|
| 主强调（粉） | `#ea91b9` | `#F0A3C6` | 品牌第一识别色，用于选中、焦点、CTA |
| 悬停粉 | `#DF7EAA` | `#F5B3D0` | |
| 辅强调（紫） | `#AD9BE8` | `#B8A7F0` | 水波/涟漪、图标点缀 |
| 辅强调（蓝） | `#99CFEA` | `#8CC9E8` | 信息、运行中 |
| 辅强调（金） | `#EBCF8C` | `#E5C875` | 星芒、提示、审批 |
| 导航深紫 | `#29263A` → `#312C46` | `#1D1929` → `#252035` | **永远深色**，不随主题变浅 |
| 浅色画布 | `#F8F6FD` / `#F2EEFA` | — | |
| 深色画布 | — | `#171421` / `#1D1929` | 深紫黑，不用纯黑 |

**B. 材质语言**

- 半透明玻璃表面（`--surface` 0.42/0.54、`--surface-elevated` 0.58/0.68），浅色上是「薄雾白」，深色上是「深紫玻璃」。
- 低强度、紫色倾向的阴影（`--shadow-card`、`--shadow-float`），不使用黑色硬阴影。
- 圆角刻度固定：6 / 10 / 12 / 16 / 20 px（`--radius-xs/sm/md/lg/xl`）。
- 背景艺术：`/cyrene-home-background.png` 作为 Shell 环境层；`/cyrene-home-character.png` 仅用于首页场景与助手头像。
- 装饰母题：水波弧线、星芒 `✦`、涟漪光环；只点缀，不承载信息。
- 字体：IBM Plex Sans（正文/标题）+ IBM Plex Mono（ID、路径、代码、日志）；字号与行高来自 `theme/typography.json`。

**C. 主题 token 值**

- `frontend/src/theme/presets.ts` 中 `CYRENE_SEMANTIC_TOKENS.light/dark` 的 **58 个键**、`PRESETS["cyrene-ripple"]` 全部字段，**值不得修改**。
- `applyTheme.ts` 的推导逻辑不得改变行为；新增派生变量只能追加。
- 外观页（`AppearanceSettings.tsx`）只保留「跟随系统 / 白天 / 夜间 + 背景图 + 重置」，不新增模式。

**D. 交互气质**

- 动效短、轻、有方向：颜色/边框 140ms，面板 220ms，页面进入 120ms；不做弹簧大位移、不做常驻流光。
- 状态以「颜色 + 文字/图标」双通道表达，不靠颜色单独承载信息。

### 2.2 允许的改动（白名单）

1. **新增派生 token**：只使用上述色相的 `color-mix()` 或固定 alpha，见 [附录 B](#附录-b-派生-token-与新增-token-提案)。禁止出现新的色相（如青色、亮绿、洋红）。
2. **修正使用方式**：把组件里的硬编码 `#353044`、`#1e1b2e`、`#232032`、Tailwind 默认 `slate/blue/green` 换成已有 token 或派生 token。**这是改用法，不是改主题。**
3. **补全暗色视觉**：使用 `CYRENE_SEMANTIC_TOKENS.dark` 中已经存在的值，让暗色下本应存在的玻璃、边框、文字层级生效。
4. **调整组件内叠加层不透明度/阴影/间距**：**不改动附录 A 中任何 token 值**；组件自身非 token 的叠加层透明度可在 ±0.06 内调整，阴影只用 `--shadow-card` / `--shadow-float` 两档，间距在 4/8/12/16/20/24/32 内调整；每处调整需截图对比并记录理由。
5. **结构与组件形态**：新增 PageShell、Tabs、Select、SearchField、ListRow 等，重组页面布局。
6. **图标尺寸与语义**、**文字的层级与行高**、**可访问性属性**、**键盘与焦点行为**。
7. **背景图可读性遮罩**：允许在既有 `--shell-overlay`/`--page-wash` 基础上做静态遮罩，不使用新增色相。

### 2.3 明令禁止

1. 新增可选主题、恢复暖色/中性色/高对比等旧预设的可视入口。
2. 改变 `--accent-primary`、`--accent-purple`、`--accent-blue`、`--accent-gold`、`--sidebar-bg` 的色值或色相。
3. 把粉色用作大面积页面底色；把导航栏改为浅色。
4. 改变状态色语义（success/warning/danger/info/running），或新增一套与现有状态并行的颜色语言。
5. 替换或重新生成品牌图、角色立绘、logo、favicon。
6. 引入 Element/Ant/Radix/shadcn 等 UI 组件库或 Framer Motion 等动画库。
7. 整页 `backdrop-filter`、常驻 CSS 动画、满屏渐变流光、粒子上浮。
8. 使用 Tailwind 默认调色板表达业务状态（`blue-500`、`green-600`、`red-500` 等）。

### 2.4 如何证明「主题风格没变」

主题是否被改变，不靠口头声明，靠以下四类证据：

| 证据 | 方法 | 通过标准 |
|---|---|---|
| **源码证据** | `theme/presets.ts` 的 git diff | 58 键值零变化；`cyrene-ripple` 预设零变化 |
| **静态扫描** | 新增测试 `themeTokenCoverage.test.ts` | TSX/CSS 中的 hex 只出现在白名单（presets、装饰渐变、历史兼容映射） |
| **计算证据** | Node 脚本计算 token 对比度 | 正文 ≥ 4.5:1；大字号/图标 ≥ 3:1；状态色在其 soft 背景上 ≥ 4.5:1 |
| **视觉证据** | 明/暗 × 6 尺寸 × 背景图开关的截图矩阵 | 与基线截图相比：色相/导航/背景艺术/材质不变，只允许结构、间距、层级变化 |

补充判定规则：**同一语义色在浅色和深色下必须仍是「同一个颜色家族」**（例如 `--accent-primary` 浅色 `#ea91b9`、深色 `#F0A3C6`，只允许明度/彩度调整，不允许换成橙色或紫色）。

---

## 3. 现状诊断

> 证据均来自 2026-10-18 基线提交的只读调查；行号以 `frontend/src` 为根。

### 3.1 已具备、不要重做的能力

- 三模式主题、背景图、重置；语义 token 注入；`data-color-scheme` 与 `.dark` 类。
- 公共组件 15 个，含 loading 按钮、错误输入、四种徽章、可缩放 Drawer、焦点生命周期 hook。
- CommandPalette（Ctrl/Cmd+K）、Toast（组件存在）、NavRail 二级「更多」菜单、执行历史模态、审批四档风险样式。
- 设置六组导航 + 未保存吸底栏；监控卡；日志键盘浏览；任务中心主从详情 + 窄窗 Drawer。
- 126 个单元/组件测试文件、18 个合同测试文件、14 个 E2E 脚本（含截图与几何检查）。

### 3.2 结构性问题（按严重度）

| # | 问题 | 关键证据 | 后果 |
|---|---|---|---|
| S1 | **暗色模式硬编码缺口** | `styles/shell.css:172,184,190,202,282,332` 固定深色文字/浅色表面；`ApprovalCard.tsx:44-65` 四档风险 hex；`CyreneToast.tsx:94` 恒深底；`CommandPalette.tsx:293/302/307` 硬编码遮罩与白边；`EmptyState.tsx:15-30` hex 插画；`features/cloud/LifecycleJobsPanel.tsx:164-216` 使用 Tailwind 默认绿/黄/红 | 夜间模式下聊天正文对比仅 **1.43:1**；审批/进度/Toast 不可读 |
| S2 | **token 双源与缺失变量** | `styles/base.css:8-139` 的 `:root` 与 `theme/presets.ts` 两处维护且值不完全一致；`--shadow-sm` 从未定义却被 `shell.css:24`、`cloudExperience.css:28` 引用；`--accent-subtle/--danger-subtle/--success-subtle/--border-hover` 被 `capabilities.css`、`memory-knowledge.css` 引用但未定义 | 主题漂移、样式静默失效、深色回退到 Tailwind 默认色 |
| S3 | **页面骨架方言 4 套** | 扁平页头（`styles/workbench.css:5-14` 覆盖所有 `page-canvas`）；圆角页头（`PageHeader.tsx:14`，实际是死样式）；cloud 页（`cloudExperience.css:1` 用不透明 `--surface-solid` + 13/15/18px 圆角）；setup 全屏（`cloudExperience.css:63`） | 同一应用视觉分裂；页面标题 18–28px 之间跳变 |
| S4 | **导航三套真值** | `layout/navGroups.ts:32-67`、`system/productPreferences.ts:1`、`NavRail.tsx:301-451` 硬编码、`CommandPalette.tsx:97-254` 各自维护；`NavRail.tsx:41` 的核心集合排除 `search`，与 `navGroups.ts:38` 冲突；标签漂移（能力/能力矩阵/能力与工具中心） | 用户隐藏的页面仍可从「更多」进入；`/search` 打开时「更多」错误高亮；改一处漏两处 |
| S5 | **伪装进度的隐藏入口** | `NavRail.tsx:262-267` 为通过合同测试塞入 `.sr-only` 的 3 个可聚焦不可见按钮；`NavRail.tsx:266` 与 `:441` 重复挂载 `ModuleSetup`；`shell.css:633-634` 专为这些按钮写样式 | 键盘用户 Tab 到不可见按钮；安全/审批入口实际不可见；与 AGENTS.md §14 冲突 |
| S6 | **组件缺口与重复实现** | 35 处原生 `<select>`；7 处手写搜索框；4 处非 tab 元素误用 `aria-selected`（`TaskCenterComponents.tsx:24`、`CapabilitiesPage.tsx:288`、`SkillsPage.tsx:231`、`AgentsPage.tsx:162`）；5 套卡片样式；4 种设置行 | 交互不一致、无障碍缺陷、修改成本指数上升 |
| S7 | **信息架构堆叠** | 任务中心首屏同时放 TaskGraph 控制面 + 搜索筛选 + 后台对话 + 任务主从，父级整页滚动且图库/对话无高度上限（`TaskCenterPage.tsx:243`、`shell.css:654`）；图库卡片只显示 TG 编号难以辨认（`:331`）；两套目标输入并存（`:281-315`）；任务详情 decision 不渲染 options，`resolveTaskDecision` 未接入（`TaskDetailPanel.tsx:291-301`），执行/事件两个 tab 内容重复（`:289-330,387-414`）；智能体页三段堆叠（`AgentsPage.tsx:124/227/240`）；监控页工具箱与下方指标重复（`SystemPage.tsx:104`）；记忆页三处分类入口（`MemoryPage.tsx:444/225/258`）；工作空间详情只有任务主从，编辑/归档未接线（`WorkspaceDetailPage.tsx:205-291`） | 首屏主角不明确；用户要滚动才能看到详情；已有操作不可达 |
| S8 | **状态真实性缺陷** | 执行「当前动作」把失败优先于运行中（`features/execution/selectors.ts:3-15`）；「N 个完成」把 rejected/cancelled 计入（`ExecutionSidebar.tsx:36-41`）；工具卡完成态恒为「工具已返回结果」（`toolDisplay.ts:130-139`）；插件 `enabled` 与 `connected` 混用（`PluginsPage.tsx:74-95`）；模型未配置时提示「发送或资源绑定失败」（`ChatInput.tsx:91`）；模板保存失败仍关窗且静默 reload（`WorkflowsPage.tsx:143-150`）；工作流运行失败不渲染 `runError`（`:327-375`）；监控缺数据显示英文 `unavailable`（`MonitoringToolbox.tsx:31`） | 用户看到不真实的进度、失败原因与成功状态 |
| S9 | **无障碍与焦点缺陷** | Button loading 直接 `disabled` 丢焦点（`Button.tsx:41`）；Tooltip 无 `aria-describedby`（`Tooltip.tsx:110-118`）；Toast 无 `aria-live`（`CyreneToast.tsx:86`）；路由加载对读屏静默（`RouteLoadingSurface.tsx:5-7`）；日志方向键即打开抽屉且无焦点框（`LogsPage.tsx:166-176,295`）；审批卡全局 y/n 直接批准（`ApprovalCard.tsx:189-212`）；12 处 `outline:none` 无替代焦点样式；全局焦点环用 accent（浅底 2.12:1）、部分页面用 30% soft（≈1.24:1），低于 3:1；`forced-colors` 模式零支持 | 键盘/读屏用户无法完成核心操作；误批准风险；低视力用户看不见焦点 |
| S10 | **性能与动效失控** | 每 token 全量解析 Markdown（`StreamingText.tsx:9`）+ 每 token smooth 滚动（`ChatView.tsx:451-454`）；消息列表无虚拟化（`MessageList.tsx:86`）；5 套动效时长（100/140/150/220/500ms）；`animate-in/zoom-in-95` 依赖未安装的 `tailwindcss-animate`（`NavRail.tsx:275`、`ExecutionHistory.tsx:177`）；56 处 `!important` | 长会话输入/流式卡顿；动效不一致；样式层叠难以维护 |
| S11 | **样式分层与级联债务** | 22 个主样式文件中 16 个未纳入 `@layer`（`index.css` 注释仍写 4 个）；60 个选择器跨文件重复（`page-header` 出现在 4 个文件、`composer-card` 3 个、`execution-sidebar` 3 个）；助手气泡三方 `!important` 互相覆盖（`shell.css:141`、`workbench.css:54`、`ui-optimization/chat.css:44`），最终实底卡覆盖 workbench「去卡片」意图；`ui-optimization/chat.css:73-84` 固定 250/1fr/360，压掉 `workspace.css:14` 的流体 clamp 与 52px 收起；`ui-optimization/settings-system.css:9-34` 压掉 `settings.css:286-291` 的 1600px 内容宽度；`styles/proofloopGoalView.css` 未进 `index.css`/`stylesheet.ts` 清单（由 `TaskWorldPage.tsx:55` 直接 import，文本测试读不到） | 改一处坏一片；设计意图（流体布局、去卡片）无法生效 |
| S12 | **排版与字体债务** | 109 处字面量 `font-size`（占 46% 声明）；365 处 `text-xs`；73 处 9/10/11px 小于 12px 下限；字重使用 650/750/550，而 Google Fonts 只加载 400/500/600/700（浏览器合成）；`capability.css:67` 标题 `0.84rem`；行高 62/75 为字面量 | 字号层级不可控；合成字重在 Windows 上发虚；小字不可读 |
| S13 | **资源与渲染成本** | 背景 PNG 765KB（1955×1230）、角色图 309KB；19 条 `backdrop-filter` 声明/11 条规则 + 9 处 `backdrop-blur` 类；`workflows.css:51` 存在无限 `box-shadow` 动画；47 条 `transition`、7 处 `transition-all`；`workbench-grid` 叠 3 层渐变；3 个未使用 keyframes；`will-change` 0 处 | 首屏加载慢、滚动掉帧、低端机发热 |

### 3.3 量化基线

| 指标 | 数值 | 说明 |
|---|---|---|
| 路由页 | 18 | 另有 3 个重定向 |
| CSS 文件 / 行数 | 26 / 5,479 | `styles/` 内 22 个（5,039 行）+ feature 局部 4 个（440 行） |
| 样式文件纳入 `@layer` | 5 / 21（`styles/` 内） | 其余未分层，级联不可预期 |
| 单元与组件测试 | 126 文件 | `vitest run` |
| 合同测试 | 18 文件 | 见附录 G |
| E2E 脚本 | 14 | `frontend/e2e/*.mjs` |
| 组件/页面层 hex | ≈ 76 处 | 不含 `presets.ts` 的 133 处 token 定义 |
| 非 token 色值字面量（区间/全量） | TSX `rgba()` 106 处；CSS 非 token 152 处 | 90 个 rgba 集中在 `shell.css`（42）与 `workspace.css`（48） |
| 未定义 CSS 变量 | 12 个 token / 39 处引用 | `--foreground`、`--foreground-muted`、`--border-strong`、`--border-hover`、`--shadow-sm`、`--accent-subtle` 等 |
| `base.css` 与 `presets.ts` 漂移 | 17 个同名 token 值不同 | 首屏 fallback 与运行态不一致 |
| `!important` | 56 处 | shell 19、workflows 9、workspace 8、chat 5 |
| 跨文件重复选择器 | 60 个 | `page-header` 4 文件等 |
| 媒体查询 / 宽度断点 | 51 条 / 16 个 | 560…1600；容器查询 0 |
| 原生 `<select>` | 35 处 | |
| 手写搜索框 | 7 处 | |
| `window.confirm/prompt` | 13 处 | 安全相关流程 |
| `role="status"` / `role="alert"` | 37 / 45 | 部分会被重复播报 |
| 可交互控件 < 44px | ≥ 6 类 | 最小 24px（`workflows.css:157`） |
| 字面量字号 / `text-xs` / <12px | 109 / 365 / 73 | 字重 650/750/550 未加载 |
| z-index 裸值 | 25 处 / 13 个值 | 10 个 z token 中 6 个零引用 |
| 英文 `unavailable` 兜底 | 73 处 | 部分面向用户 |
| 明/暗关键对比度失败 | ≥ 8 处 | 最严重 1.43:1（深色正文） |
| 浅色焦点/强调文字对比度 | 2.12:1 / 2.88:1 / 2.68:1 | 低于 WCAG AA |
| `backdrop-filter` / `transition-all` | 19 声明、11 规则 / 7 处 | 有无限 `box-shadow` 动画 |
| 图片资源 | 背景 765KB、角色 309KB | 均为 PNG |

### 3.4 样式体系专项结论

| 主题 | 结论 |
|---|---|
| **token 真源** | 运行态值来自 `CYRENE_SEMANTIC_TOKENS`（`applyTheme.ts:34-38` 无条件使用），`base.css` 的 `:root` 只是首屏 fallback 且与运行态漂移；`PRESETS` 中另外 4 套旧预设的 `colors` 实际不参与渲染（兼容代码，不是可见主题）；`proofloopGoalView.css` 未进入样式清单 |
| **级联所有权** | `ui-optimization/*` 最后加载且同特异性覆盖主样式，导致设计意图反转（助手气泡实底、侧栏固定宽、设置 1600px 失效）；`@layer` 覆盖率不足是根因 |
| **响应式** | JS 断点（960/1180）与 CSS 断点（899/900/980/1099/1100）不同步；980–1099 任务页隐藏详情但保留两行 grid（空白行）；720 高时工作流 `min-height: 640px` 超过可用高度；1366@125% 落 compact、@150% 落 narrow，但 CSS 在 900–960 仍按桌面排版 |
| **可访问性** | 焦点环对比度不足；12 处 `outline:none` 无替代；`forced-colors` 零支持；`text-faint` 被大量用于正文补充信息（TSX 182 处 + CSS 38 处），实际对比 2.88:1 |
| **性能** | 19 条 blur 声明、无限 `box-shadow` 动画、7 处 `transition-all`、3 层背景渐变叠加、765KB PNG；这些在 800×600 低配窗口上会直接影响滚动与切换体验 |

### 3.5 结论

本轮不应继续「按页加 CSS」；`ui-optimization/*` 已经证明「后加载覆盖前加载」会积累债务。正确顺序是：

1. 先修 **token 真源 + 层 + 断点 + 焦点**（W0）；
2. 再修 **外壳与导航 + 公共组件**（W1/W2）；
3. 最后按页升级（W3–W8），页面只消费公共层，不再新增跨页覆盖。

---

## 4. 全局层升级 G1–G12

> 全局层是「所有页面共同消费」的部分。这一节不实现页面，只定义规范与基础设施。

### G1 主题与 token 单一真源

**目标：** 主题值只在 `theme/presets.ts` 定义一次；`base.css` 的 `:root` 只作为首屏 fallback 且与预设逐值相等；新增派生 token 集中登记。

**做法：**

1. 新增 `frontend/src/theme/tokens.ts`：
   - 导出 `CYRENE_TOKENS`（引用 `CYRENE_SEMANTIC_TOKENS`，不复制值）；
   - 导出 `TOKEN_GROUPS`（背景/表面/文字/边框/强调/状态/侧栏/阴影/代码/动效）供文档、样式审计测试与外观预览使用。
2. 新增派生 token 到 `base.css`（见附录 B），命名规则：`--{类别}-{语义}[-{状态}]`，如 `--risk-high-fg`。
3. 新增测试 `theme/tokenContract.test.ts`：
   - 扫描 `styles/*.css` 与 `components/**/*.tsx`，收集所有 `var(--x)`，断言其要么在 `CYRENE_SEMANTIC_TOKENS`、要么在派生 token 清单中；
   - 扫描 hex/rgba，断言只出现在白名单文件/行（`presets.ts`、`applyTheme.ts` 的 legacy 映射、装饰渐变白名单）。
4. `base.css` 的 `:root` 与预设同步方式：优先改为构建期生成（Vite 插件或 `npm run gen:tokens` 脚本）；若团队不接受构建脚本，则保留手写但由测试强制相等。
5. **补齐 12 个被引用但未定义的变量**（检查结果）：`--foreground`、`--foreground-muted`、`--border-strong`、`--border-hover`、`--shadow-sm`、`--accent-subtle`、`--danger-subtle`、`--success-subtle`、`--warning-subtle`、`--panel-muted`、`--text-tertiary`、`--surface-input`。处理方式二选一：
   - 作为别名指向已有 token（推荐，零视觉变化）；
   - 若确认从未使用，删除引用而不是新增变量。
   同时删除 `ui-optimization/memory-knowledge.css:101/281/476` 中的非昔涟兜底色（`#ec4899`、`#22c55e`、`#ef4444`）。
6. **样式清单补录**：把 `styles/proofloopGoalView.css` 纳入 `index.css` 与 `stylesheet.ts`，删除 `TaskWorldPage.tsx:55` 的直接 import；同时把 `index.css` 注释中的「4 个未分层文件」更新为真实数量。
7. **兼容预设说明**：`presets.ts` 中除 `cyrene-ripple` 外的 4 套预设只作为历史数据兼容存在，不参与渲染；保留但加注释，禁止任何页面引用其颜色（测试断言）。
8. **层策略**：新增 CSS 规则必须写入 `@layer components/utilities/base` 之一；未分层的 `shell.css`、`responsive.css`、`animations.css`、`markdown.css` 在 W0 中评估后逐步收进层，或在文件头标注「unlayered by design + 所有者」。禁止再通过「最后加载的新文件」覆盖旧文件。

**验收：** 未定义 `var()` 数为 0；token 双源测试通过；样式清单与磁盘一致；主题切换无闪烁、无半透明残留。

### G2 表面层级与玻璃规则（L0–L3）

沿用 2026-10-16 的四层模型，但把「哪层允许什么」写成可执行规则：

| 层 | 用途 | 允许 | 禁止 |
|---|---|---|---|
| **L0 画布** | Shell 背景、背景图、环境艺术 | `--bg-app`、`--shell-overlay`、`--page-wash`、静态遮罩 | 页面根节点绘制不透明底色；整页 `backdrop-filter` |
| **L1 结构** | 页头、主导航、左栏、分区外框 | `--surface`、`--divider`、`--border-soft`；≤8px 局部 blur | 多层叠加 blur；给整个页面加玻璃 |
| **L2 内容** | 卡片、列表行、输入、预览 | `--surface-elevated`、`--surface-solid`、`--shadow-card` | 用 `opacity` 实现可读性（会把文字一起变淡） |
| **L3 浮层** | Modal、Drawer、菜单、Tooltip、Toast | `--surface-solid`/`--surface-elevated`、`--shadow-float`、`--backdrop`；blur ≤12px | 遮罩颜色各自硬编码；与被遮内容使用同一色调导致边界不清 |

**暗色补充规则：** 深色下优先用「表面明度差 + 边框」区分层级，不靠黑色阴影；玻璃不透明度整体提高一档（使用已定义的 dark 值），避免背景艺术透视导致文字对比下降。

**已知暗色修复清单（必须逐项闭环）：**

1. `shell.css:171-214` 会话 Markdown：正文/链接/行内码/表头/引用改为 `--text-*`/`--code-*`/`--accent-*`；深色下不再固定 `#353044`。
2. `shell.css:224-260` 进度卡/工具卡：浅色 rgba 背景改为 `--surface-elevated`/`--surface-muted`；`data-status="failed"` 色条目前无 CSS 选择器（`ToolCallCard.tsx:170`），需补齐。
3. `shell.css:297-333` 审批/错误提示：背景与文字成对 token 化；删除 `!important` 覆盖。
4. `workspace.css:392` 的 `mix-blend-mode: multiply` 助手头像在深色下会压成暗块：深色改为 `normal`。
5. `ApprovalCard.tsx:44-65` 四档风险 hex → `RiskBadge` token；`:290/:297` 命令块 → `--code-bg`/`--code-text`。
6. `CyreneToast.tsx:59-94`、`CommandPalette.tsx:104-251/293-307`、`EmptyState.tsx:15-30`、`Modal.tsx:51/70` 全部 token 化。
7. `features/cloud/LifecycleJobsPanel.tsx:142-216` 与 `SyncPrivacyPanel.tsx` 的 Tailwind 默认色 → 语义 token。
8. 所有模式判定统一：优先使用 token（自动适配）；只有必须区分模式时才用 `:root[data-color-scheme="dark"]`；禁止逐组件写两套 hex。

### G3 页面骨架：`PageShell`

**问题：** 现有页面各自 `PageHeader + body`，页头被 `workbench.css` 覆盖成一种样式，`PageHeader` 组件自身的圆角卡片样式变成死代码；每页还要自己管理 `h-full / min-h-0 / overflow-y-auto`，容易出现「双滚动条」或「内容被裁」。

**方案：** 新增 `frontend/src/components/layout/PageShell.tsx`，所有业务页统一使用。

```tsx
type PageShellVariant = "standard" | "workspace";

interface PageShellProps {
  title?: string;
  description?: string;
  status?: ReactNode;        // 状态徽章 / 连接状态
  actions?: ReactNode;       // 主操作，最多 3 个
  variant?: PageShellVariant; // standard: 页头 + 唯一滚动 body
                              // workspace: 页头 + 全高不滚动容器（内部面板各自滚动）
  maxWidth?: "reading" | "form" | "dashboard" | "none";
  children: ReactNode;
}
```

**结构：**

```
┌────────────────────────────────────────────────────────────┐
│ PageShell header（56px，sticky）                            │
│  [标题/返回]  [状态徽章]              [次级操作] [主操作]     │
├────────────────────────────────────────────────────────────┤
│ body（唯一滚动容器，标准 padding）                          │
│   内容宽度：reading 860 / form 1180 / dashboard 1600       │
│                                                            │
└────────────────────────────────────────────────────────────┘
```

**规范：**

- 页头高度 56px（compact 48px），底部 1px `--border-soft` 分隔；标题 20px semibold；描述 13px `--text-secondary`，最多一行。
- 主操作每页最多 1 个 primary 按钮；超过 3 个操作时，其余收进「更多」菜单。
- `variant="workspace"` 用于 Chat / TaskWorld / 工作流编辑器：页头固定，内容区 `flex-1 min-h-0 overflow-hidden`，由内部面板各自滚动。
- 继续输出 `page-canvas` / `page-header` 等兼容类名，避免破坏合同测试与现有 CSS；但页面内不再写页头样式。
- 删除 `workbench.css` 对 `.page-header` 的「字体缩小到 18px」的全局覆盖；标题字号由 PageShell 决定。
- 任务画布的 `StudioHeader.tsx:11` 自制 `<h1>忆涟·Studio</h1>` 与 PageHeader 并存，是本轮唯一「第二页头」：统一为 PageShell 的 `workspace` 变体，品牌字样保留为副标题或页头徽章，不改变画布路由、面板与状态管理。
- 每个页面只允许一个滚动所有者：`standard` 由 PageShell body 滚动；`workspace` 由内部面板滚动。页面内不得再写 `h-full overflow-y-auto` 包裹整个页面（当前 Chat/TaskCenter/Workflows 都有这类重复）。

### G4 排版层级

| 层级 | 字号/行高 | 字重 | 用途 |
|---|---|---|---|
| 页面标题 | 20 / 1.3 | 600 | PageShell 标题 |
| 分区标题 | 16 / 1.4 | 600 | Section header |
| 卡片标题 | 14 / 1.45 | 600 | 列表项/卡标题 |
| 正文 | 14 / 1.55 | 400 | 管理页正文（对话阅读 17 / 1.65） |
| 说明 | 13 / 1.55 | 400 | 描述、辅助说明 |
| 标签/元信息 | 12 / 1.4 | 500 | 时间、计数、Badge |
| 数值 | 24–32 / 1.1 | 600 + `tabular-nums` | 指标卡 |
| 等宽 | 12–13 / 1.5 | 400 | ID、路径、代码、日志 |

**规则：** 正文不得小于 13px；12px 只用于标签与元信息；不支持全站缩字号；标点与中英文混排保持现状。所有「标签: 值」行统一用 `KeyValueRow`（label 13px muted，value 14px primary，值过长换行而非截断）。

补充约束：

- 字重只使用 **400 / 500 / 600 / 700**（`index.html` 只加载这四档 IBM Plex Sans）；禁止 650/750/550，避免浏览器合成导致发虚。
- 109 处字面量 `font-size` 全部收敛到 `typography.json` 的 6 档或 4 个固定对照（12/13/14/16/17/20/22/28）；`capability.css:67` 的 `0.84rem` 标题必须修正。
- 365 处 `text-xs` 中，属于正文补充信息的改为 `text-meta`（13px）或 `text-secondary`（14px）；12px 只保留给标签、时间、计数。
- 行高统一：caption 1.4、body 1.55、reading 1.65；删除 1.45/1.6 等杂值。
- 数字与价格/计数使用 `font-variant-numeric: tabular-nums`；路径、ID、代码使用等宽字体。

### G5 状态语义与风险展示

**状态四组 + 运行中：**

| 语义 | 颜色 token | 使用场景 |
|---|---|---|
| success | `--success` / `--success-soft` / `--success-fg` | 完成、验证通过、已保存 |
| warning | `--warning` / `--warning-soft` / `--warning-fg` | 待确认、部分完成、需要配置 |
| danger | `--danger` / `--danger-soft` / `--danger-fg` | 失败、拒绝、删除 |
| info | `--info` / `--info-soft` / `--info-fg` | 提示、只读、来源信息 |
| running | `--status-running`（已有） | 正在执行、连接中 |

**风险四档（审批 / 工具）：**

| 风险 | 视觉 | 语义 token |
|---|---|---|
| low | 蓝色徽章 + 盾形图标 | `--info-*` |
| medium | 金色/警示徽章 | `--warning-*` |
| high | 粉色危险徽章 | `--danger-*` |
| critical | 危险徽章 + 边框强调 + 二次确认 | `--danger-*` + `--danger-border` |

不允许为风险等级新增独立色相；`ApprovalCard.tsx` 现有四档 hex 改为上述 token 映射（视觉上等价或更克制）。

**状态必须双通道：** 颜色 + 文字（或图标 + 文字）；只靠颜色变化不算完成。

### G6 动效收敛

| 场景 | 时长/曲线 | 禁止 |
|---|---|---|
| 颜色/边框/焦点 | 140ms ease-out | `transition: all` |
| 卡片 hover | 150ms，位移 ≤2px | 表格行、任务行、画布节点位移 |
| 浮层进入/退出 | 进入 180ms `cubic-bezier(.2,.8,.2,1)`、退出 150ms | 退出未播完就卸载 |
| 页面内容出现 | 120ms opacity + ≤4px 位移 | 路由未就绪先显示假数据 |
| 运行态 | opacity 轻变化，周期 ≥1200ms | 快速闪烁、满屏流光 |
| 减少动态效果 | 保留即时状态变化，取消位移/循环 | 关闭动效后加载状态不可见 |

**修复清单：**

1. `styles/animations.css` 的 `--ease-ripple/spring/mist` 目前零引用 → 采用 `--ease-ripple` 作为标准进入曲线，删除未用曲线。
2. 删除所有 `animate-in / fade-in / zoom-in-95`（依赖未安装的 `tailwindcss-animate`），改用 `utilities.css` 中的 `animate-fade-in / animate-scale-in`。
3. `styles/shell.css:544-557` 的全局 `prefers-reduced-motion` 会把所有动画压到 0.01ms，导致 Spinner 静止：改为「保留 opacity 呼吸或显示文字」，确保加载可见。
4. `AppShell.tsx:31,41` 的 300/500ms 背景过渡改为 `--motion-normal`。

### G7 键盘、焦点与浮层次序

**焦点次序（所有页面一致）：** 页头操作 → 主内容区的筛选/搜索 → 列表 → 详情/检查器 → 页脚（如有）。浮层打开后焦点进入浮层，Tab 在浮层内循环，关闭后回到触发元素；触发元素不存在时回到页头标题。

**焦点环规格（修复对比度）：** 双层环——内层 1px `--surface-solid`、外层 2px `--accent-primary`（或 `box-shadow: 0 0 0 2px var(--surface-solid), 0 0 0 4px var(--accent-primary)`），保证在浅色背景上 ≥3:1；`--focus-ring-soft` 只用于装饰性柔光，不得作为唯一焦点提示。删除 12 处裸 `outline:none` 或补等价焦点样式；`cloudExperience.css:56` 的搜索输入必须有可见焦点环。所有可点击的 `div/span` 必须改为按钮或补 `role/tabIndex/keydown`。

**浮层次序：**

1. IME 合成中，Enter/Escape 交给输入法；
2. 焦点在输入框/编辑器内，不触发页面导航与列表上下切换；
3. Esc 只关闭最上层一层；
4. 无模态浮层时，Esc 可收起聊天/画布辅助栏；
5. Esc 不停止生成、不拒绝审批、不丢弃草稿；
6. 浮层关闭后焦点归还。

**全局快捷键（保持现有语义，补可发现性）：** 见 [附录 E](#附录-e-快捷键焦点与键盘规则)。所有快捷键在对应控件上加 `aria-keyshortcuts`；CommandPalette 中显示真实快捷键，不写不存在的键。

**必须删除的行为：** `ApprovalCard.tsx:189-212` 的全局 `y/n` 直接审批。改为「审批卡获得焦点后，在卡内按 Y/N」，或只保留按钮；减少误批。

### G8 响应式与窗口矩阵

**窗口基线：** 800×600（最小）、1024×700、1200×800（默认）、1366×768、1600×900、1920×1080、2560×1440、3440×1440；缩放 100% / 125% / 150% / 200%。

**断点统一（写入文档，代码中通过常量与注释对齐）：**

| 名称 | 宽度 | 行为 |
|---|---|---|
| narrow | < 720px | 单列；辅助栏变 Drawer；页头操作折行 |
| compact | 720–959px | 双列收紧；详情可 Drawer |
| medium | 960–1099px | 聊天 compact；管理页单列 + Drawer |
| wide | 1100–1179px | 管理页双栏；聊天仍 compact |
| full | ≥ 1180px | 聊天三栏；管理页双栏最大宽度 |

**规则：**

- 布局按「内容宽度」而非窗口宽度决策的页面（管理页双栏）使用容器查询或测量容器宽度；聊天继续用窗口宽度（现有 `workspaceLayout.ts` 语义不变）。
- 高度断点 780px 只影响聊天首页与紧凑导航；不隐藏任何导航项。
- `min-width: 800px` 下任何页面不得出现横向滚动条；超宽表格/代码在容器内滚动。
- 125%/150% 缩放下主操作按钮不得被裁切；Drawer 最大宽度不超过 `100vw - 16px`。

**已知响应式缺陷（必须修复）：**

1. 断点漂移：CSS 中 899/900、1099/1100 分别并存，JS 用 960/1180（`workspaceLayout.ts:45-49`、`ChatPage.tsx:36-49` 等 7 处 `window.innerWidth`）。统一为 900/1100 两个管理页断点，960/1180 明确登记为「聊天专用」。
2. 任务页 980–1099：`tasks-workspaces.css:33-39` 隐藏详情但 `shell.css:592-597` 保留两行 grid → 出现空白行；修复为单列或改 Drawer。
3. 高度 720 时工作流 `min-height: 640px` 超过可用高度（`responsive.css:141-144`、`workspace.css:714`）：改为 `min-height: min(640px, 100%)`。
4. 1366@125% ≈ 1093 落 compact，@150% ≈ 911 落 narrow，但 CSS 在 900–960 仍按桌面排版：所有三栏/两栏规则必须与 JS 判定同源。
5. 16 个宽度断点收敛为 5 档（560 / 720 / 900 / 1100 / 1600 超宽），其余只允许高度断点与动效媒体查询；管理页双栏优先使用容器查询（`@container`，原生支持，不引依赖）。

### G9 全状态矩阵

每页必须覆盖其实际能进入的状态，统一按下表实现与验收：

| 状态 | 用户看到 | 可执行动作 | 事实来源 |
|---|---|---|---|
| 首次加载 | 结构骨架（不是转圈全屏） | 可离开 | 请求未完成 |
| 刷新中 | 保留旧内容 + 刷新按钮 busy + 最近刷新时间 | 可继续阅读 | 请求 pending |
| 空数据 | 说明 + 真实创建入口 | 创建/前往 | 请求成功且为空 |
| 筛选空 | 「没有匹配结果」+ 清除筛选 | 清除筛选 | 有数据但过滤后为空 |
| 加载失败 | 错误说明 + 重试 | 手动重试 | API 失败 |
| 部分失败 | 页面可用 + 分区错误条 | 重试该分区 | 并行请求部分失败 |
| 保存中 | 按钮 loading + 保留表单值 | 等待 | mutation 未完成 |
| 保存成功 | 明确「已保存」+ 真实数据同步 | 继续编辑 | API 成功 |
| 保存失败 | 具体原因（可展开原文）+ 重试 | 修改重试 | API 失败 |
| 选中 | 持久边框/底色 + `aria-current` | 打开/关闭详情 | 当前选中 ID |
| 禁用 | 原因可见，键盘不触发 | 可查看原因 | 业务条件 |
| 运行/等待/失败 | 状态文字 + 图标 + 对应动作 | 停止/审批/重试 | 业务状态 |
| 离线/后端不可用 | 顶部提示条 + 页面保留旧数据 | 重试 | health 检查失败 |

### G10 空态、错误与离线的统一表达

- `EmptyState` 增加 `variant`：
  - `default`：插画 + 标题 + 说明 + 主操作；
  - `search`：小图标 + 「没有匹配结果」+ 清除筛选；
  - `unavailable`：说明「需要什么条件才能使用」+ 前往配置；
  - `offline`：后端不可用说明 + 重试。
- 所有插画改为 token 色（`currentColor` + `--accent-*` 透明度），删除 `#ea91b9` 等 hex、`border-white/50` 与常驻 `animate-ping`。
- `ErrorState` 增加 `details?: string` 折叠原始错误、`onRetry?`、`severity`（error/warning）。
- 新增 `OfflineBanner`（全局轻量条），由 health 状态驱动；不阻塞页面。
- `Skeleton` 提供形状变体：`text` / `line` / `card` / `list` / `table`；容器加 `aria-busy="true"`。
- **强制颜色模式**：补 `@media (forced-colors: active)` 兜底——焦点改用 `outline`、状态不只靠背景色、边框使用系统色；Windows 高对比度下不得出现「按钮消失」。
- **减少透明**：`@media (prefers-reduced-transparency: reduce)` 时玻璃表面回退为 `--surface-solid`，颜色家族不变；模糊失效后靠边框维持层级。
- **减少动态效果**：见 G6；加载指示必须有非动画替代。

### G11 图标与插画

- 图标库保持 Lucide；尺寸系统：20（导航）、18（页头/按钮）、16（行内/输入）、14（徽章/元信息）。
- 图标必须与文字一起表达状态；装饰性图标 `aria-hidden="true"`。
- 品牌装饰（水波、星芒、涟漪）只在：Shell 背景、首页场景、空态插画、导航选中态、执行轨迹时间线节点。
- 角色立绘只在：首页场景、助手头像、设置外观预览。不得在列表卡、错误页、按钮里新增立绘。
- 不新增位图资源；如需新装饰，用 CSS/SVG 以现有 token 生成。

### G12 性能预算

| 项目 | 预算/做法 |
|---|---|
| 首屏 | 继续按路由懒加载；单路由 chunk 不显著增大（相对现有 +10% 以内） |
| 流式消息 | Markdown 解析与滚动合并到 rAF，≥120ms 节流；仅近底时平滑滚动；`content-visibility: auto` 用于历史消息 |
| 长列表 | 任务/记忆/日志列表超过 200 行时，列表行使用 `content-visibility: auto` + 固定行高；聊天消息超过 150 条时启用窗口化渲染（不引第三方库，按消息高度估算） |
| 轮询 | 合并同页多接口轮询；页面不可见时暂停；轮询间隔 ≥2s 且用 `useVisibleRefresh` 统一 |
| 模糊 | 只有 L1/L3 允许 `backdrop-filter`，单元素 blur ≤12px；K 个卡片不同时开启 |
| 动画 | 只动 `transform/opacity`；不持续动画 `box-shadow`、`filter`、`width` |
| 构建 | 保留现有 Vite 分包策略；不为性能引入新依赖 |

补充动作：

- **图片**：把 `cyrene-home-background.png`（765KB）与 `cyrene-home-character.png`（309KB）转换为同构图 WebP（目标 ≤200KB / ≤120KB），保留 PNG 作为兼容回退；`<img>` 标注宽高避免 CLS。不改变美术内容。
- **模糊**：19 条 `backdrop-filter` 声明收敛为 3 类场景（页头、浮层、抽屉），删除卡片级 blur 与 9 处散落的 `backdrop-blur` 类；单元素 blur ≤12px。
- **动画**：删除 `workflows.css:51` 的无限 `box-shadow` 动画、3 个未使用 keyframes、7 处 `transition-all`；脉冲类动画只动 `opacity/transform`。
- **滚动条**：5 处隐藏滚动条（`workspace.css:103/189/284/701`）保留，但容器需有可滚动提示（边缘渐隐或 hover 时出现细滚动条）。
- **渲染隔离**：长列表行、历史消息加 `content-visibility: auto` 与 `contain-intrinsic-size`，避免一次性布局。`will-change` 仅在动画期间临时加，不常驻。

---

## 5. 外壳层升级 S1–S7

### S1 导航单一注册表（NavRegistry）

**目标：** 导航的标签、图标、快捷键、分组、模块开关、命令面板描述，全部来自一处。

**新建 `frontend/src/components/layout/navRegistry.ts`：**

```ts
export interface NavEntry {
  id: string;              // 与 preferences.navigation.id 对齐
  to: string;
  label: string;           // 唯一中文标签
  description: string;     // 命令面板副标题
  icon: LucideIcon;
  group: "core" | "capability" | "system";
  primary: boolean;        // 是否显示在 NavRail 主区
  shortcut?: string;       // "Alt+1" 等，一处定义
  moduleId?: string;       // 关联 TRUSTED_MODULES
  keywords?: string[];     // 命令面板搜索
}
```

**派生关系：**

```
navRegistry.ts
  ├─ NavRail：primary 项 + 「更多」分组菜单
  ├─ CommandPalette：全部导航项 + 命令
  ├─ useUiNavigationShortcuts：shortcut 字段
  ├─ ModuleSetup：显示/排序/模块开关
  └─ 测试：navGroups 兼容导出（或直接迁移测试）
```

**规则：**

- `primary` 固定为 6 项：任务、对话、工作流、工作空间、搜索、能力。搜索回归核心组，与 `Alt+3` 一致。
- 「更多」菜单按 `capability` / `system` 分组，全部来自注册表；尊重 `preferences.navigation.visible` 与 `enabled_modules`。被隐藏的项不出现在菜单，也不可通过快捷键进入（快捷键按同一过滤函数计算）。
- 删除 `navGroups.ts` 的重复定义（保留兼容导出，若测试直接引用则更新测试）。
- `CommandPalette` 与 `NavRail` 的标签必须完全一致（测试断言）。

**验收：** 三处联动测试：注册表 → NavRail 菜单项集合；注册表 → CommandPalette 标题；注册表 → 快捷键过滤；`/search` 高亮正确；隐藏页面不可达。

### S2 NavRail 升级

- 尺寸保持 `w-[68px] min-[960px]:w-[80px]`（合同测试冻结，不改）。
- 结构：品牌区（Logo）→ 主区 6 项 → 「更多」触发器 → 底部设置/状态（可选保留）。
- 视觉：选中态继续使用 `--sidebar-active` + 左侧 2px 粉色高亮 + 星芒点缀；hover 用 `--sidebar-bg` 提亮，不用 `text-white` 硬编码。
- 「更多」改为真正的 Popover：
  - `role="dialog"` 改为 `aria-expanded` + `aria-haspopup="menu"`；
  - 打开时焦点进入第一项，↑↓ 切换，Esc 关闭并归还焦点；
  - 点击路由后关闭；点击外部关闭；
  - 不阻断其他全局快捷键（修正 `useUiNavigationShortcuts` 用 `[role='dialog']` 一刀切的问题：改为「有模态浮层时才屏蔽」）。
- 删除 `.sr-only` 假按钮；新增「安全与审批」真实入口（见 S6），不再为测试塞隐藏节点；相应更新合同测试为检查真实入口。
- 矮窗口（<780px）不裁切：主区允许内部滚动（`scrollbar-thin`，隐藏滚动条样式可保留），不再 `overflow:hidden !important`。

### S3 AppShell 与背景层整理

- 合并 `AppShell.tsx:30-50` 的两层内联遮罩为一个受控结构：
  - 层 1：背景色/背景图（来自主题）；
  - 层 2：环境艺术（`shell-ambient` + 可读性 overlay，使用 `--shell-overlay`/`--page-wash`）；
  - 删除内联 `linear-gradient(135deg, rgba(...))` 硬编码，改用派生 token（如 `--shell-readability-gradient`）。
- 背景模式过渡从 300/500ms 降为 `--motion-normal`；避免切主题时整屏重绘。
- 保持 `surfaceContract.test.ts` 的全部断言：`shell-ambient`、`var(--bg-app)`、`var(--shell-overlay)`、`.page-canvas` 透明、`.app-shell` 无 `backdrop-filter`。
- 增加全局 `RouteErrorBoundary`：懒加载 chunk 失败或渲染异常时显示可重试页面，而不是白屏。

### S4 CommandPalette 升级

当前问题：15 处图标 hex、遮罩/白边硬编码、无 listbox 语义、hover 抢键盘选中、无最近使用、无结果提示单一。

**升级：**

1. 颜色全部 token 化：图标使用 `--accent-primary/purple/blue/gold`、`--text-secondary`、`--success`；遮罩用 `--backdrop`；面板用 `--surface-solid`；顶部装饰条用 `--cyrene-gradient-primary`（已存在）。
2. 可访问性：`role="dialog"` + `aria-modal`，内部 `role="listbox"` + `role="option"` + `aria-activedescendant`；输入框 `aria-controls`、`aria-autocomplete="list"`；关闭后焦点归还触发元素。
3. 交互：键盘选中优先于鼠标 hover（hover 只加背景，不改 selectedIndex）；↑↓ 首尾不循环（与列表规范一致）；空结果显示「没有匹配的命令」+ 提示可搜索的关键词。
4. 最近使用：localStorage 记录最近 5 条命令，打开时置顶；不跨用户同步。
5. 分区：导航页面 / 系统与偏好 / 高频动作；每项显示真实快捷键；搜索支持中文与拼音首字母（关键词来自注册表 `keywords`）。
6. 增加「?" 帮助」可选：列出全部快捷键（若不做，则至少在设置-外观中放快捷键说明）。

### S5 Toast 升级与真实接入

当前 `CyreneToast` 是死代码（无调用方），且恒深底、无 `aria-live`、状态色与主题状态色不一致。

**升级规格：**

- 视觉：`--surface-solid` 背景 + 状态色左边框 + 状态图标；浅色/深色都跟随主题；圆角 `--radius-md`；阴影 `--shadow-float`。
- 语义：`aria-live="polite"` 容器；每条 message 有唯一 id；最多同时 3 条；3–5s 自动消失；hover/focus 暂停计时。
- 位置：右下角，底部避让语音 dock（`bottom: calc(16px + var(--voice-dock-height))`）。
- 动作：可选 `action?: { label, onClick }`（如「撤销」「查看」），用于删除/复制等场景。
- **真实接入（本轮至少 5 处）：**
  1. 工作空间路径复制成功；
  2. 设置保存成功 / 失败（失败同时保留页面内错误条）；
  3. 记忆/技能删除后「已删除 · 撤销」（若后端支持恢复则接，不支持则不做撤销，只提示）；
  4. 搜索保存为知识 / 加入任务成功；
  5. 导出/下载成果成功（保留页面内确认）。
- 不接入高频操作（如每次发送消息），避免打扰。

### S6 安全入口可见化（对应 AGENTS.md §14）

**问题：** 审批与「急停」只有隐藏的 sr-only 按钮，正常用户根本找不到。

**方案（分两步，避免一次做太大）：**

1. **必须做（前端可独立完成）：**
   - NavRail 底部增加「安全」入口（盾牌图标，带待审批数量 Badge），点击打开审批中心 Drawer/Modal，复用现有 `listPendingApprovals` 与 `ApprovalCard` 交互；
   - Chat 页存在待审批时，会话内显示内联提示条「有 N 项操作等待确认」+ 一键定位；
   - CommandPalette 增加「查看待审批」「打开安全设置」命令。
2. **需确认后端能力后做：**
   - 全局「紧急停止」：在 Chat/任务画布运行中显示停止入口；点击调用现有 `POST /api/chat/stop`（当前会话）或画布停止接口；如果没有跨页面的全局运行标识，则只做「当前页面的停止」并在文案中写明范围。
   - **禁止**在没有真实取消能力时显示「停止全部」按钮。

### S7 语音 dock 与浮层避让

- `.global-voice-dock` 的 `z-index: 40` 改为 `--z-voice-dock`（在 base.css 中新增，值位于导航与浮层之间），避免与设置吸底栏、Toast 重叠。
- 设置吸底保存栏、日志详情 Drawer、Toast 三者与语音展开面板的避让关系写成规则（自下而上：语音 pill < Toast < 吸底栏 < Drawer < Modal）。
- 语音 pill 使用 `aria-live="polite"` 报告状态变化；展开面板有明确关闭按钮与 Esc 行为（沿用现有逻辑，不改语音运行时）。

---

## 6. 公共组件升级 C1–C15

> 原则：页面只组合公共组件，不复制视觉实现；新组件必须带 props 文档、可访问性行为与至少一个合同测试。

### C1 Button（升级）

| 现状问题 | 改动 |
|---|---|
| 未设默认 `type="button"` | 默认 `type="button"`，表单提交需显式 `type="submit"` |
| loading 时 `disabled` 导致焦点丢失 | 改为 `aria-disabled="true"` + 阻止点击；保留焦点；`aria-busy` |
| loading 文案替换可访问名 | 保留原标签文本（视觉上淡出），叠加「处理中」作为补充；或使用 `aria-label` 拼接 |
| 尺寸被 `shell.css` 反向覆盖 | 删除 `shell.css:607-610` 覆盖，高度收敛为 32/36/40/44 四档（sm/md/lg + icon） |
| danger 用 opacity | 改为 token 色 hover + 边框，与其他档一致 |
| 缺图标按钮变体 | 新增 `iconOnly` 尺寸规范（固定 32/36px 方形，必须传 `aria-label`） |

**API：** `ButtonProps extends ButtonHTMLAttributes`，新增 `loading?: boolean`（已有）、`loadingLabel?: string`（已有）、`iconOnly?: boolean`。

### C2 Input / Textarea / Field（升级）

- 抽出 `FormField`：label、hint、error、required 标记、`aria-describedby`、`aria-invalid` 统一处理；Input/Textarea/Select 复用它。
- 修复 `Textarea.tsx:19` 用 label 文本拼 id 导致重复 id 的缺陷，改用 `useId()`。
- error 提示不再逐字符 `role="alert"` 重播：输入过程中使用 `aria-live="polite"` 或仅在 blur/submit 时播报。
- 内阴影改为 token（`--input-inset-shadow` 派生 token）。
- 新增 `revealable` 密码输入（模型/语音/搜索密钥共用），不改变“不显示已保存密钥”的安全语义。
- 原生 `<select>` 迁移到 C4。

### C3 Panel / Section（升级 + 新增）

- `Panel`：定位为 **L2 内容卡**；统一圆角 `--radius-lg`、边框 `--border-soft`、背景 `--surface`/`--surface-solid` 二选一、内边距 16/20；`density` 保留；删除无调用方的 `interactive` prop（或补齐键盘语义后再保留）。
- 新增 `Section`：**L1 结构分区**，带标题、描述、右侧操作、可折叠（`<details>` 语义或 `aria-expanded`），用于替代 `SystemSection`、`u2-disclosure`、页内手写分组。目标：全站卡片从 5 套收敛为 2 套。

### C4 Select（新增）

- 样式化原生 `<select>`：统一高度 40px、圆角 `--radius-md`、边框/焦点/禁用态、选项不可自定义时保持原生（不做自定义下拉，避免引入浮层复杂度）。
- Props：`label?`、`hint?`、`error?`、`options: {value,label,disabled?}[]`、`size?: "sm"|"md"`。
- 替换优先级：设置页、能力筛选、日志级别、账号区域、搜索区域（≥20 处）；无法替换的登记原因。

### C5 SearchField（新增）

- 结构：搜索图标 + `<input type="search">` + 清空按钮 + 可选快捷键提示（如 `/`）。
- Props：`value`、`onChange`、`onClear`、`placeholder`、`autoFocusOnSlash?: boolean`（只在页面内启用）、`aria-label`。
- 替换 7 处：会话侧栏、能力、技能、记忆、日志、任务中心、搜索页。

### C6 Tabs / SegmentedControl（新增）

- 同一个组件两种形态：`variant="underline"`（页面级切换，如工作流模板/运行时）与 `variant="segment"`（视图切换，如能力矩阵/卡片、监控网格/自由）。
- 语义：`role="tablist"` + `role="tab"` + `aria-selected` + `aria-controls`；方向键切换、Home/End、焦点与面板关联。
- 替换：`AccountCenterPage` 手写 tablist、`CapabilitiesPage` 视图切换、`WorkflowsPage` 模板/运行时、筛选分组（需评估）。
- 修正现有误用 `aria-selected` 的非 tab 元素（改为 `aria-current` 或正确 role）。

### C7 Badge / StatusBadge（升级）

- 统一 `tone`：default / success / warning / danger / info / accent + `running`（可用 pulse 点但周期 ≥1200ms）。
- 新增 `RiskBadge`（low/medium/high/critical，内部映射到 C 系列语义，不暴露 hex）。
- 新增 `StatusBadge` 组合：图标 + 文案 + tone；用于任务、执行、审批、插件、能力、验证。
- 删除重复实现 `CapabilityStatusBadge`（或改为薄封装复用 StatusBadge），避免两套状态渲染。

### C8 PageHeader（合并进 PageShell）

- `PageHeader` 保留为兼容导出（内部调用 `PageShell.Header`），不再单独维护样式；删除 `mx-4 mt-4` 等死样式。
- `title` 支持 `ReactNode`（可放徽章/图标）；`description` 支持一行省略；`actions` 自动折行。
- 合同测试若引用 `PageHeader` 字符串，保留组件名与导出。

### C9 Modal（升级）

- 遮罩统一 `--backdrop`（删除 `#1e1b2e/45`）；顶部渐变条改用 `--cyrene-gradient-primary`。
- 退出动画：进入 180ms、退出 150ms；在动画结束后卸载（需要内部状态管理，不能 `open=false` 立刻返回 null）。
- 尺寸：sm 480 / md 640 / lg 860 / xl 1040；`max-height: calc(100dvh - 32px)`；内容区独立滚动，footer sticky。
- 焦点：沿用 `useDialogFocusLifecycle`；关闭按钮 `aria-label="关闭{title}"` 与 Drawer 一致。
- 新增 `variant="danger"`：危险确认时顶部条/主按钮用 danger，不改变安全性。

### C10 Drawer（升级）

- `z-50` 改为 `--z-rail-drawer`（已有 token）；遮罩统一 `--backdrop`。
- 宽度断点集中：`width` clamp 到 `min(设定值, 100vw - 16px)`；窄窗（<720px）100% 宽。
- 拖宽手柄：键盘左右键 16px 步进（现有逻辑保留）；增加 `aria-valuetext`。
- Drawer 内容区只承载一个滚动容器；不允许内部再套整页滚动。

### C11 Tooltip（升级）

- 触发器与浮层建立 `aria-describedby` 关联；浮层加唯一 id。
- 位置计算加入 scroll/resize 监听（`ResizeObserver` + `scroll` capture），避免滚动后错位。
- 触屏：长按 500ms 显示；离开/滚动隐藏。
- 边界魔数集中为常量；浮层宽 ≤260px、可换行。

### C12 EmptyState / ErrorState / Skeleton / Spinner（升级）

- `EmptyState`：四种 variant（默认/搜索/未满足条件/离线）；插画 token 化；删除常驻 ping；操作区最多 2 个按钮。
- `ErrorState`：severity、details、retry；静态面板用 `role="status"`，表单提交失败用 `role="alert"`。
- `Skeleton`：形状变体 + 容器 `aria-busy`；路由加载使用结构骨架（贴合 PageShell）。
- `Spinner`：`prefers-reduced-motion` 下保留可见替代（静态圆环 + 文案）；多个 Spinner 不重复 `role="status"`，由父容器统一播报。

### C13 ConfirmDialog（新增）

替代 13 处 `window.confirm/prompt`（保留后端安全边界不变）：

- Props：`open`、`title`、`description`、`confirmLabel`、`cancelLabel`、`tone: "default" | "danger"`、`requireText?: string`（删除类操作要求输入确认词）、`onConfirm`、`onCancel`。
- 键盘：Enter 确认（除非 requireText 未填）、Esc 取消；焦点默认在取消按钮（危险操作）。
- 文案必须描述真实后果（「删除后无法恢复」等），不能只写「确定吗？」。

### C14 ListRow（新增）

统一任务行、能力行、记忆行、日志行（可选）的选中/悬停/键盘行为：

- Props：`selected`、`onSelect`、`leading`（图标/头像）、`title`、`meta`、`trailing`（状态徽章/时间）、`statusBar?`（左侧色条）、`onActivate?`（Enter）。
- 语义：外层 `role="option"` + 容器 `role="listbox"`（或使用 `aria-current` 的按钮列表，二选一并全站统一）。
- 修正 `aria-selected` 误用：TaskRow 等改为 listbox/option 或 `aria-current`.

### C15 FilterBar / Inspector / CodeBlock / KeyValueRow / StatTile / ProgressBar（新增，6 件）

- `FilterBar`：搜索 + 筛选 chips/下拉 + 计数 + 刷新；统一任务、能力、记忆、日志、搜索页的筛选区；移动端自动折行。
- `Inspector`：右侧详情外壳（头部/元信息/操作/滚动内容/空态），统一 TaskDetail、能力详情、执行详情、记忆详情、日志详情；不改变各领域的数据来源。
- `CodeBlock`：语言标签、复制按钮、可选行号、最大高度内滚动；用于 Markdown 代码、技能正文、命令预览、日志详情。
- `KeyValueRow`：统一 `label: value` 行。
- `StatTile`：指标数值 + 单位 + 说明 + 可选进度/环形（SVG，无新依赖）；用于系统监控、能力统计、设置用量。
- `ProgressBar`：`role="progressbar"` + `aria-valuenow/min/max`；支持 bar/ring 两种。

---

## 7. 页面升级 P1–P9

> 每页包含：现状问题（证据）→ 升级目标 → 页面结构 → 升级点 → 验收。  
> 组件落位见 [附录 J](#附录-j-页面--组件映射升级后)；所有页面统一使用 `PageShell`，页面内不再单独定义页头样式与整页背景。

### P1 对话工作台（Chat）

**现状证据：** 审批藏在执行轨（`ChatPage.tsx:231`、`CurrentActionCard.tsx:34-43`）；「当前动作」判定把失败置于运行中之前（`selectors.ts:3-15`）；消息 Markdown 硬编码深色文字（`shell.css:172`）导致夜间 1.43:1；三层 `!important` 气泡覆盖（`shell.css:141-146`、`workbench.css:54`、`chat.css:45`）；三处阅读宽度不一（`workspace.css:370,455-464`、`chat.css:12,73-96`）；逐 token 解析与滚动（`StreamingText.tsx:9`、`ChatView.tsx:451-454`）；运行中禁用输入框且错误文案不符（`ChatInput.tsx:130,91`）。

**升级目标：** 把「聊天页」升级为「智能体工作台」：先看到状态与待办，再看到对话；执行轨迹可读、可定位、不喧宾夺主；输入舱状态清晰；长会话稳定。

**页面结构：**

```
full（≥1180）
┌──────────┬───────────────────────────────────────┬──────────────┐
│ 会话栏    │ 会话身份条（标题/来源/状态/更多）        │ 执行轨        │
│ 250px    ├───────────────────────────────────────┤ 默认收起 52px │
│ 搜索/新建 │ 待审批提示条（如有）                   │ 展开 360px    │
│ 会话列表  │ 消息流（用户/助手/工具轨迹/审批卡）      │ 当前动作      │
│          │ 回到底部按钮                           │ 时间线        │
│          ├───────────────────────────────────────┤ 失败筛选      │
│          │ 输入舱（附件/语音/发送/停止）           │              │
└──────────┴───────────────────────────────────────┴──────────────┘

compact（960–1180）：左栏 + 中央；执行轨改为右侧 Drawer。
narrow（<960）：中央独占；会话/执行分别通过 Drawer 打开。
首页（/chat）：同一输入舱居中 + 角色场景 + 快捷动作 + 最近任务/工作区。
```

**升级点：**

1. **会话身份条（ChatHeader 重做）**
   - 左：会话标题（`deriveTaskDisplayTitle`）+ 来源徽章（任务/工作区/普通对话）+ 连接状态点；
   - 右：打开当前任务、执行轨开关、更多菜单；
   - 运行中显示已用时长与当前工具名；失联时显示「连接已断开，点击重连」。
2. **审批前置**
   - 会话内存在待审批时，在消息流顶部固定一条提示：「有 N 项操作等待确认」+「查看」；
   - 审批卡在消息流中与对应工具卡相邻显示（同一 `tool_call_id` 关联），执行轨中仍保留；
   - 删除全局 y/n 直批，改为卡内快捷键。
3. **执行轨（ExecutionSidebar 升级）**
   - 修正「当前动作」选择：`running` / `awaiting_approval` 优先于历史失败；
   - 计数口径拆开：完成 / 被拒绝 / 已取消 / 失败，不再混算；
   - 每条记录：工具名、动作摘要（使用已有 `formatToolActionSummary`）、风险徽章、验证状态、耗时；
   - 点击记录：滚动到消息流中的对应工具卡并短暂高亮；「定位」是显式动作，不自动移动；
   - 详情从整屏遮罩改为右栏内展开或统一 Drawer（保留焦点管理）。
4. **消息呈现**
   - 建立唯一气泡规则：用户消息为 `--accent-soft` 卡（16/6 圆角）；助手消息为无卡片正文（保留头像与元信息），工具轨迹以紧凑卡展示；
   - 删除 `shell.css:141-146`、`workbench.css:54`、`chat.css:45` 的重复背景覆盖，只保留一份；
   - Markdown 行高统一 1.65；代码块用 `CodeBlock`（语言 + 复制）；表格横向滚动 + 边缘阴影；链接显示外链图标；
   - 消息操作条：复制、朗读、查看执行、引用到输入框；无 hover 设备常显。
5. **输入舱状态机**
   - `idle / sending / running / uploading / error` 五态：运行中不锁死输入框（允许输入草稿），发送按钮变「停止」，提示「当前任务运行中，Enter 可排队/停止后可发送」——具体行为按现有 API 能力，不新增后端队列；
   - 附件：拖拽高亮、上传中进度/失败重试、超过 8 个/25MiB 的说明；
   - 模型未配置时显示「先配置模型」+ 入口，而不是「发送或资源绑定失败」；
   - 快捷键提示不截断，窄窗折叠为图标。
6. **首页（WorkbenchHome）**
   - 保留角色、问候与快捷动作；
   - 增加「继续最近任务」「最近工作区」两行（来自现有 `listTasks`/`listWorkspaces`，为空则隐藏）；
   - 快捷动作的 prompt 保持现状（不新增能力承诺）；
   - 首页与对话页共用同一输入舱组件与状态。
7. **性能**
   - 流式 Markdown：合并解析到 rAF，≥120ms 节流；
   - 滚动：仅当用户接近底部时平滑跟随，否则显示「回到底部」按钮；
   - 超过 150 条消息启用窗口化渲染（自研，按估算高度 + 测量校正）。

**验收：**

- 待审批在会话内 1 次点击可达；无待审批时不打扰；
- 暗色下正文/代码/审批/错误/进度卡对比度达标；
- 消息阅读宽度全场景一致（860px）；
- 输入五态均有可见反馈；
- 200 条消息流式测试无长任务 >100ms（开发工具 Performance 面板抽检）；
- 窄窗 800×600 无横向滚动、Drawer 焦点归还正确。

**不做：** 不新增「重新生成」「多点分支对话」「消息编辑重发」等需要后端支持的能力。

---

### P2 任务中心（TaskCenter）

**现状证据：** 首屏三段堆叠（`TaskCenterPage.tsx:244-540`：TaskGraph 控制面、搜索筛选、后台对话、任务主从），父级整页滚动且图库/后台对话无高度上限（`:243` + `shell.css:654`），任务列表被挤出首屏；图库卡片只显示 TG 编号难以辨认（`:331`）；两套目标输入并存（`:281-315`）；筛选仅 5 类但与真实状态枚举（draft/ready/waiting_approval/waiting_user/blocked/running/completed/failed/cancelled）不完全对应，「运行中」混装 5 种状态、草稿不可筛（`taskPresentation.ts:12`），且没有计数；任务详情的 decision 不渲染 options、`resolveTaskDecision` 未接线（`TaskDetailPanel.tsx:291-301`），执行与事件两个 tab 内容重复（`:289-330,387-414`）；窄窗靠 `window.innerWidth` 判断（`:508`）而 CSS 断点是 1099（`tasks-workspaces.css:33`），存在 1px 不一致与 980–1099 空白行。

**升级目标：** 首屏先回答「我现在有什么任务、哪个需要我」，把「规划新任务」收进明确动作，不抢占列表。

**页面结构：**

```
┌──────────────────────────────────────────────────────────────┐
│ PageShell：任务中心                    [刷新] [新建任务 ▾]     │
├──────────────────────────────────────────────────────────────┤
│ 优化摘要条（可选）：运行中 2 · 待审批 1 · 失败 1  [查看待审批]   │
├──────────────────────────────────────────────────────────────┤
│ FilterBar：搜索 / 状态 chips（带计数）/ 来源 / 排序 / 刷新      │
├───────────────────────────────┬──────────────────────────────┤
│ 主列表（Tabs：任务 | 会话 | 任务图）│ 详情 Inspector（右）        │
│ ListRow：状态条/标题/工作区/时间  │ 概览 / 执行 / 成果 / 相关记忆 │
│ 选中态 + ↑↓ + Enter            │ 空态「选择一个任务」          │
└───────────────────────────────┴──────────────────────────────┘
新建任务 → Drawer/Modal（Tabs：快速规划 | 证行目标编译 | 空白画布）
```

**升级点：**

1. **首屏去堆叠**：TaskGraph 控制面（目标输入、Proofloop 审核、画布卡）收进「新建任务」Drawer；首屏只保留优化摘要条 + 列表/详情。
2. **筛选与计数**：状态 chips 使用真实枚举分组（运行中/待审批/等待用户/已完成/失败/已取消/草稿），显示每组计数；搜索命中标题、描述、工作区、来源；「无匹配」显示清除筛选。
3. **来源区分**：任务 / 会话 / 任务图三类用图标 + 徽章区分，不再把后台对话混进任务列表的同一视觉层级。
4. **TaskRow → ListRow**：左侧状态色条、标题一行、元信息一行（工作区 · 时间 · 来源）、右侧状态徽章；选中持久化；键盘 ↑↓/Enter；修复 `aria-selected`。
5. **详情 Inspector**：统一头部（标题 + 状态 + 操作）、Tab（概览/时间线/成果/相关记忆，仅保留真实存在的数据源）、滚动内容；操作按钮 loading/错误内联；`onChanged` 刷新列表。**决策等待状态必须在详情顶部渲染 options 并接入现有 `resolveTaskDecision`**，不能只显示「等待决策」文字；执行与事件两个 tab 合并为「时间线」一个（事件作为时间线的技术详情）。
6. **窄窗**：Drawer 详情；断点统一（JS 与 CSS 都使用 1100）；关闭 Drawer 后焦点回到列表项。
7. **轮询**：仅存在运行中/待审批时轮询；页面不可见暂停；刷新按钮显示最近刷新时间；避免整页骨架闪烁（保留旧数据）。
8. **任务图卡片**：显示任务标题（`deriveTaskDisplayTitle`）+ 节点/边数 + 状态徽章，而不是只显示 TG 编号；点击进入画布。
9. **单滚动所有者**：页头固定，任务列表自己滚动；图库与后台对话改为有高度上限的 `Section`（可折叠），不再让父级整页滚动把主列表挤出首屏。

**验收：** 1200×800 首屏可同时看到 ≥3 条任务与详情；筛选计数与真实数据一致；决策 options 可提交且结果反映到状态；执行/事件不再重复展示；空态/加载/错误/无权限四态齐全；窄窗操作闭环；父级无第二滚动条。

---

### P3 工作空间（Workspaces / WorkspaceDetail）

**现状证据：** 列表卡片信息稀疏（名称/描述/路径），缺少真实活跃任务数（`WorkspacesPage.tsx:139-204`）；路径复制按钮嵌在整卡链接内部（嵌套交互，点击/键盘冲突），且复制无反馈；创建弹窗打开时不重置表单；详情页只有任务主从，`updateWorkspace/deleteWorkspace` 未接编辑/归档（`WorkspaceDetailPage.tsx:205-291`）；不新增无端点页签（本轮不做 Knowledge/Subagents 假页签）。

**升级目标：** 让「工作空间」成为任务的组织容器：能一眼看到每个空间的活跃任务与最近活动。

**页面结构：**

```
列表页
┌ PageShell：工作空间                          [+ 新建工作空间] ┐
│ FilterBar：搜索 / 排序（最近活动/名称）/ 刷新                  │
│ 卡片网格（minmax(300px,1fr)）                                │
│ ┌─────────────┐ ┌─────────────┐ ┌─────────────┐             │
│ │ 📁 名称      │ │             │ │             │             │
│ │ 路径（复制） │ │             │ │             │             │
│ │ 活跃任务 2   │ │             │ │             │             │
│ │ 最近更新     │ │             │ │             │             │
│ └─────────────┘ └─────────────┘ └─────────────┘             │
└──────────────────────────────────────────────────────────────┘

详情页
┌ 返回 / 工作空间名 · 路径（复制）              [新建任务] [更多] ┐
│ 左：概览（描述、任务数、创建时间） 右：任务列表（真实数据）      │
│ 空态：还没有任务，创建第一个                                   │
└──────────────────────────────────────────────────────────────┘
```

**升级点：** 卡片补齐真实字段（活跃任务数、最近更新时间）；把复制按钮移出整卡 Link，作为独立图标按钮（`aria-label` + Toast 反馈）；创建表单 Modal 打开时重置字段（校验、loading、错误）；详情页保留「概览 + 任务列表」两个真实区域，若现有 API 已提供编辑/归档能力则接线，否则不显示入口；不新增 Knowledge/Subagents 等无端点页签；删除不可用操作。

**验收：** 无假计数；创建/复制/进入详情闭环；空态与错误态齐全。

---

### P4 工作流（Workflows）

**现状证据：** 模板/运行时切换是页头手写按钮（`WorkflowsPage.tsx:41-58`），无 tab 语义；页面 body 统一 `overflow-y-auto p-6`（`:62`），编辑器与运行检视被塞进同一个滚动区；模板保存失败仍关闭弹窗并静默 reload，修改丢失（`:143-150`）；运行失败不渲染 `runError`（`:327-375`）；模板网格固定双列无断点，操作按钮仅 hover 显形（`:193,219`）；编辑器 zoom 类名与样式不匹配、fit 固定 600（`WorkflowGraphEditor.tsx:223,296` + `workflows.css:142`）；空画布缺引导 CTA（`WorkflowGraphEditor.tsx:324,365`），运行脉冲/状态徽标 CSS 已写但未接线（`workflows.css:50,87`）；`WorkflowRunInspector.tsx:167-183` 的 ID 截断不可复制、缺少图名与耗时；表单 textarea 手写，与 Input 不一致（`:289,295`）。

**升级目标：** 编辑器回归「全高画布工作台」，模板/运行列表保持轻量；运行态节点与检视器清晰可定位。

**页面结构：**

```
列表模式
┌ PageShell：工作流        [模板工作流 | 运行工作流]  [新建]  ┐
│ 模板：卡片网格（名称/描述/节点链/激活/内置/操作）              │
│ 运行：运行列表（状态/时间/耗时）+ 最近运行                     │
├──────────────────────────────────────────────────────────────┤
编辑器模式（variant="workspace"）
┌ 顶栏：返回 / 名称 / 保存状态 / 运行 / 关闭                     ┐
│ 左：节点库（分组+搜索）│ 中：画布（grid/缩放/连接）│ 右：Inspector │
│                        │ 底部缩放/适配/定位          │ 节点配置     │
└──────────────────────────────────────────────────────────────┘
运行检视：右栏或 Drawer（概览/节点/日志；点击节点显式定位）
```

**升级点：**

1. 模板/运行时用 `Tabs`；运行列表与最近运行合并去重。
2. 编辑器使用 `variant="workspace"` 全高布局，不再嵌套页面滚动。
3. 模板卡：主操作常显、次级操作 hover 出现；保存失败保持弹窗并显示原因，删除失败保持列表并提示；只有成功才关闭/刷新。删除用 `ConfirmDialog`。
4. 节点状态使用 token（运行中蓝、成功绿、失败红、等待金），不新增色值；接线已存在但未使用的运行脉冲/状态徽标样式；节点被选中/运行时有清晰描边与光晕（使用 `--accent-*` 透明度）。
5. Inspector 统一用 `Inspector` 组件（标题/描述/字段分组/动作区），表单控件统一 Input/Textarea/Select。
6. 运行检视：节点点击 → 高亮 + 可选「定位节点」按钮；错误节点优先展示错误摘要与重试；运行失败的就地错误条置顶；ID 可复制并显示图名/耗时。
7. 空画布：引导卡片「添加第一个节点」，不自动创建假节点；修复 zoom 类名不匹配与 fit 固定 600，按容器尺寸计算。

**验收：** 1440×900 与 1200×800 下画布可用、无双重滚动；节点颜色符合语义 token；运行/停止/定位闭环；窄窗（<1100）右栏折叠为 Drawer。

---

### P5 能力四页（Agents / Skills / Plugins / Capabilities）

**现状证据：** 智能体页三段堆叠、卡片 7 层信息、无搜索筛选、`?agent=` 无消费者；技能页正文裸 `<pre>`、4 个复制按钮、打开详情无竞态守卫、编辑关闭丢草稿、删除无撤销；插件页单页长滚动、状态只映射 4 种、真实字段未用、每服务器 6 按钮平铺；能力页平表无分组、表头不在滚动容器内、空态矛盾、每行两个冗余入口。

**升级目标：** 四页共用同一「列表 + 检查器」范式；状态诚实；阅读/编辑体验达到产品级。

**统一结构（新增 `CapabilityLayout` 内部模式）：**

```
┌ PageShell：能力名                              [刷新] [主操作] ┐
│ FilterBar：SearchField / 类型 / 状态 / 来源 / 视图切换          │
├───────────────────────────────┬──────────────────────────────┤
│ 列表（ListRow 或分组矩阵）      │ Inspector（右，选中后显示）     │
│ 选中高亮 + 键盘 ↑↓             │ 标题/状态/元信息/内容/操作      │
│ 空态/加载/错误分区显示          │ 空态「选择一项查看详情」        │
└───────────────────────────────┴──────────────────────────────┘
```

**各页要点：**

- **Agents**：卡片压缩为 3 行（名称+状态、角色描述一行、模型/来源一行）；详情常驻右侧；补搜索；来源深链 `?agent=` 落地；`enabled` 只显示一次；团队信息保留但并入详情，不占首页三段。
- **Skills**：正文阅读器（标题层级、代码块、可折叠目录）；只保留 1 个复制入口；编辑 Modal 关闭前草稿确认；删除 `ConfirmDialog` + 成功 Toast；筛选「托管/可编辑/只读」；`openSkill` 加请求序号防竞态。
- **Plugins**：状态机补齐（未启用/启用未连接/连接中/已就绪/错误），显示真实字段（资源模板数、最近刷新、运行时就绪）；每服务器操作收进「主操作 + 更多」；页面分区（内置工具 / MCP 服务 / 子智能体）用 `Section`，长页不丢页头；加载/错误不替换整页页头。
- **Capabilities**：按 provider/kind 分组折叠矩阵，组内计数；表头 sticky 且与滚动容器同层；风险用 `RiskBadge`；删除每行重复「查看详情/管理来源」（整行可点，操作进 Inspector）；托管导入折叠进 Drawer。
- 四个页面共用：空态、错误、加载、刷新状态与键盘行为。

**验收：** 四页同一布局节奏；状态文案真实；合同测试文案保留（修改需同步更新测试并说明）；窄窗 Drawer 详情。

---

### P6 记忆中心 / 知识库（Memory / Knowledge）

**现状证据：** 记忆页分类入口三处（概览气泡、chips、列表计数）；搜索逐键请求且整列骨架闪烁、无竞态守卫；详情标题固定「记忆详情」；编辑只改正文；删除失败提示出现在另一条记录；行操作默认透明（触屏不可达）。知识库页诚实但重复三遍同一说明、880px 窄栏大量留白、全页唯一动作是跳转。

**升级目标：** 记忆中心成为可检索、可维护的「事实库」；知识库改为清晰的「能力地图」，不虚构能力。

**页面结构：**

```
记忆中心
┌ PageShell：记忆中心                    [维护] [新建记忆] ┐
│ 概览条：分类气泡（1 处入口）+ 搜索 + 来源/分类筛选          │
├───────────────────────────────┬──────────────────────────┤
│ 记忆列表（ListRow，分组/时间）  │ Inspector：内容/分类/来源   │
│ 加载更多                        │ 编辑（inline）/删除（确认） │
└───────────────────────────────┴──────────────────────────┘

知识库
┌ PageShell：知识库                                        ┐
│ 状态条：当前版本支持什么、不支持什么（一句话）              │
│ 两栏卡片：① 当前可用：长期记忆（动作 → 前往记忆中心）        │
│           ② 尚未接入：文档导入/索引（状态徽章 + 未来规划）   │
└──────────────────────────────────────────────────────────┘
```

**升级点：**

1. 分类入口只保留「概览条」一处，chips 作为筛选与概览联动；列表计数不再充当入口。
2. 搜索 250ms 去抖 + 保留旧列表 + 请求序号守卫；加载中只显示轻量指示，不清空列表。
3. 详情 Inspector：标题用内容摘要；分类/来源可编辑（若接口支持，否则只读并说明）；删除确认 + Toast；错误显示在对应记录/详情内。
4. 行操作常显（无 hover 依赖），键盘可达。
5. 知识库：删除重复说明，保留「已就绪 / 尚未接入」双分区与徽章；增加「它会怎样影响对话与检索」的说明段；页面宽度与其它页统一（dashboard maxWidth）。

**验收：** 搜索无闪烁；编辑/删除反馈闭环；知识库无假数据、无重复文案；暗色可用。

---

### P7 设置（Settings）

**现状证据：** 导航点击不回写 `?section=`，深链/后退失效；页头与吸底栏两个同名「保存设置」；保存失败只有统一文案且不播报；`SandboxSettings.tsx:32` 回车被 `filter(Boolean)` 吞掉；`PermissionSettings.tsx:44` 逗号同样被吞；多处 label 无 `htmlFor`；外观即时生效与「保存/放弃」语义冲突且上传无边校验；无离开守卫；模型档案即时落库与页面保存两套语义。

**升级目标：** 设置页成为唯一的配置入口：可深链、可保存、可离开保护；危险操作统一确认。

**页面结构：**

```
┌ PageShell：设置                    [状态徽章] [保存设置]  ┐
├──────────────┬──────────────────────────────────────────┤
│ 左导航 200px  │ 右内容（max-width 960，唯一滚动容器）       │
│ 模型服务      │ 分组标题 + 说明                            │
│ 语音能力      │ 卡片（setting rows）                       │
│ 智能体        │ ...                                        │
│ 安全控制      │ 未保存变更吸底栏（唯一保存入口）             │
│ 能力扩展      │                                            │
│ 外观          │                                            │
└──────────────┴──────────────────────────────────────────┘
```

**升级点：**

1. 导航点击写入 `?section=`；读取时解析；切换分组不丢失脏值提示；返回/前进可用。
2. 保存语义收敛：吸底栏为唯一保存入口；页头保留状态徽章与「保存设置」按钮但两者共享同一状态与禁用条件（或只留吸底栏，需产品确认）。
3. 保存失败：显示具体原因（保留后端 error 原文可展开）+ `role="alert"` + 重试；成功后 Toast + 徽章更新。
4. 表单：所有 label 关联 `htmlFor`；回车/逗号能正确新增路径与条目；错误提示内联。
5. 外观：三个模式卡使用真实 Cyrene 缩略图（替换 Tailwind slate 示意色）；背景图上传校验类型/大小并显示错误；「重置默认」用 ConfirmDialog；theme 变更即应用（保持现状），但在离开/放弃时明确「外观即时生效」。
6. 新增「工作台与导航」卡片（把 `ModuleSetup` 从导航二级菜单迁移至此），保留模块开关与导航显隐/排序；不新增第七分组。
7. 离开守卫：存在未保存修改时，路由切换弹出确认（使用 `useBlocker` 或自定义确认）；不拦截浏览器关闭。
8. 密钥：保留「替换/清除」语义；密码框加显示切换；清除用 `ConfirmDialog`。

**验收：** 深链与后退；保存成功/失败/重试；脏值离开确认；所有表单键盘可达；外观缩略图与实际主题一致。

---

### P8 系统监控 / 运行日志（System / Logs）

**现状证据：** 监控页工具箱置顶且与下方资源/服务重复；本体是无样式编辑器；缺数据时出现英文 `unavailable`；自动刷新也置 loading、图标常驻自旋；`swap_per_core` 等真实字段未用；环形图 `aria-hidden`。日志页方向键即打开抽屉、焦点不可见、无 `aria-selected`；暂停时「刷新」无效；轮询不判可见性、500 行无虚拟化；筛选项仅来自已加载数据；清空无确认；窄窗布局被覆盖失效。

**升级目标：** 监控页是「当前状态仪表盘」，日志页是「可检索的运行控制台」。

**页面结构：**

```
系统监控
┌ PageShell：系统监控        [最近刷新 14:32:10] [刷新] [查看日志] ┐
│ 顶部状态条：Backend 正常 · DB 正常 · 版本 · 策略版本              │
│ 资源卡：CPU / 内存（含 swap）/ 磁盘 / GPU（真实字段）              │
│ 服务与安全：健康、待审批、进程隔离、诊断（真实数据，未知显示「未采集」）│
│ 详细信息：主机/OS/内核/运行时间（KeyValueRow）                     │
│ 自定义仪表盘：网格/自由布局（保留 preferences.monitor，编辑态才显示）│
└──────────────────────────────────────────────────────────────┘

运行日志
┌ PageShell：运行日志   [自动刷新中/已暂停] [级别 chips] [搜索] [暂停] ┐
│ 控制台（--code-bg，等宽，行号，时间列，级别色）                     │
│ 行选中 → 右侧 Inspector/Drawer 显示完整消息与元数据                 │
│ 暂停时：显示「已暂停，恢复后加载最新」；不在暂停时假刷新             │
└──────────────────────────────────────────────────────────────┘
```

**升级点：**

1. 监控：删除重复数据层，合并为一个 dashboard；`MonitoringToolbox` 改为「自定义仪表盘」编辑模式（保留模块开关/布局偏好），默认查看态不再显示编辑器控件。
2. 真实字段优先：使用 swap、per_core、process_count、isolation 状态；未采集显示「未采集」，不显示英文占位。
3. 刷新：自动刷新不显示全屏 loading，只更新数值与「最近刷新」；图标不再常驻自旋。
4. 指标卡带 `role="progressbar"` 或数值文本（不依赖环形图）。
5. 日志：方向键只改变选中，Enter 打开详情；焦点框可见；`aria-selected` 改为 listbox/option 或 `aria-current`。
6. 暂停语义：暂停即停止轮询并显示明确状态；恢复时拉取最新；「刷新」在暂停时也应显式拉取一次（或明确禁用并说明）。
7. 清空日志使用 `ConfirmDialog`（API 为 drain，操作不可逆需说明）。
8. 窄窗单列、消息换行不截断；1000 行以上使用窗口化或分批渲染。

**验收：** 无英文占位；刷新状态真实；日志键盘闭环；暗色代码区可读；窄窗无横向滚动。

---

### P9 账号中心 / 搜索 / 初始化（Account / Search / Setup）

**现状证据：** 账号页使用 `cloud-page` 不透明背景与独立 252px 侧栏（与设置页不一致）；原生 `prompt/confirm`；错误条在 main 末尾、全页共享 busy；游客态登录卡与 Profile 卡重复；会话列表空无空态。搜索页 0 结果仍渲染空区、动作无 loading、自动创建工作空间。初始化页 readiness=null 导致状态闪变、清空 `api_key_env`、完成度按索引推断。

**升级目标：** 抹掉最后一组成套视觉方言，让账号/搜索与主壳一致，同时保持本地优先、诚实能力说明。

**升级点：**

1. **账号中心**：改用 `PageShell`（透明画布 + L2 卡）；左侧分区导航复用设置的 200px 规格与样式；同步/隐私/凭据操作统一 `ConfirmDialog`；错误条放到相关卡片内；空态补齐；Jobs 状态颜色 token 化（删除 Tailwind 默认色）。
2. **搜索**：命令栏用 `SearchField` + `Select`；提交 loading；结果卡统一（来源域名高亮、摘要三行、动作按钮 1 主 + 更多）；0 结果用 `EmptyState variant="search"`；配置折叠为 `Section`；「保存为知识」与「加入任务」加 loading 与 Toast；不自动创建「搜索研究」工作区（改为让用户选择目标空间或明确提示）。
3. **初始化**：保留独立全屏（不挂 AppShell）但统一 token、字体、圆角、间距；步骤进度条；readiness 未加载时显示骨架而非「需要配置」；完成页按真实 readiness 计算（不按索引推断）；表单一律不复用会清空 `api_key_env` 的写法。
4. **通用**：删除该组页面里的 Tailwind 默认色与硬编码阴影；全部使用 PageShell 的页头规格。

**验收：** 三页在明暗下与主壳一致；无原生 prompt；无假刷新/假状态；空态齐全。

---

## 8. 跨页一致性规范

### 8.1 结构一致性

| 项 | 规范 |
|---|---|
| 页面外壳 | 统一 `PageShell`；`variant="workspace"` 仅 Chat/TaskWorld/Workflows 编辑器 |
| 页头 | 标题 20px / 描述 13px 一行 / 右侧最多 3 操作 / 状态徽章在标题旁 |
| 内容宽度 | reading 860（对话阅读）、form 1180（设置/表单）、dashboard 1600（任务/能力/记忆/监控/日志）、none（画布） |
| 滚动 | 每页只有一个滚动容器；`variant="workspace"` 的滚动在面板内部 |
| 卡片 | L2 用 `Panel`；L1 分组用 `Section`；浮层用 Modal/Drawer；不再新增第 6 套卡面 |
| 间距 | 页面 16/20/24；卡片内 16/20；元素间 8/12；列表行 12 |
| 圆角 | 6/10/12/16/20 四档；按钮/输入 10/12；卡片 16；浮层 16–20；不使用 `rounded-xl/2xl` 混写 |
| 图标 | 20/18/16/14 四档；状态图标与文字同色 |

### 8.2 文案一致性

| 场景 | 统一文案 |
|---|---|
| 加载 | 「正在加载…」（骨架屏可不带文案）、按钮「加载中…」 |
| 空数据 | 「还没有{对象}」+ 一句说明 + 主操作 |
| 无匹配 | 「没有匹配的结果」+「清除筛选」 |
| 错误 | 「{对象}暂时无法加载」+ 原因 + 「重试」 |
| 保存 | 「保存设置」/「保存中…」/「已保存」；失败「保存失败：{原因}」 |
| 删除 | 「删除{对象}」+ 后果说明 + 危险按钮 |
| 运行中 | 「运行中」；停止按钮「停止」 |
| 待审批 | 「等待确认」/「批准」/「拒绝」 |
| 未采集 | 「未采集」（禁用英文 `unavailable` 面向用户） |
| 时间 | `formatTaskDate` / `formatLogTimestamp` 统一；相对时间只在 24h 内使用 |
| 数字 | 百分比 1 位小数、GB 1 位小数、ms 整数；数字列 `tabular-nums` |

### 8.3 按钮层级

- 每屏最多 1 个 primary；同一组操作最多 1 个 danger。
- 危险操作必须二次确认（`ConfirmDialog`），文案描述真实后果。
- 图标按钮必须有 `aria-label` 与 Tooltip（Tooltip 不能是唯一可访问名）。
- loading 按钮保持宽度、保留原可访问名、不夺取焦点。

### 8.4 焦点与键盘

- 统一焦点环：`:focus-visible` 2px `--focus-ring` + 2px offset；删除重复的 `focus-ring-token` 自定义。
- 列表统一 `↑/↓` 换选、`Enter` 激活、`Home/End` 首尾（不循环）。
- 浮层统一 Esc 关最上层、Tab 内循环、关闭归还焦点。
- 快捷键统一由注册表生成，UI 上显示真实键位，并加 `aria-keyshortcuts`。

### 8.5 视觉一致性检查项（实现自检）

- 页面内没有第二个页头；
- 没有 `text-white` / `text-black` 直接写在组件上（导航深紫底内除外，且用 `--sidebar-*`）；
- 没有新增 hex；
- 暗色下所有卡片、输入、浮层边界可见；
- 状态色只出现在语义位置；
- 悬浮操作在无 hover 设备上不可见即为缺陷；
- 长单词/路径/ID 不撑破容器（`overflow-wrap:anywhere` 或省略 + Tooltip）。

---

## 9. 实施计划

### 9.1 工作包与所有权

> 与 2026-10-16 相同：工作包可以并行，但文件所有权必须互斥；公共入口由集成负责人单独接线。

| 包 | 范围 | 可改文件（核心） | 新增文件 | 依赖 |
|---|---|---|---|---|
| **W0 主题与规范基座** | token 单源、12 个缺失变量、派生 token、字号/行高/字重、动效/z-index/断点收敛、CSS 分层与清单、图片与 blur 治理（附录 H 全部 14 项）、扫描测试 | `theme/presets.ts`（只增不改值）、`theme/applyTheme.ts`（追加）、`styles/base.css`、`styles/utilities.css`、`styles/animations.css`、`styles/responsive.css`、`styles/stylesheet.ts`、`index.css`、`frontend/public/*`（WebP） | `theme/tokens.ts`、`theme/tokenContract.test.ts`、`themeTokenCoverage.test.ts` | 无 |
| **W1 外壳与导航** | PageShell、AppShell、NavRegistry、NavRail、CommandPalette、快捷键、路由加载/错误、Toast、安全入口、语音 dock | `components/layout/*`、`AppShell.tsx`、`RouteLoadingSurface.tsx`、`components/ui/CommandPalette.tsx`、`CyreneToast.tsx` | `components/layout/PageShell.tsx`、`navRegistry.ts`、`RouteErrorBoundary.tsx`、`components/ui/SafetyEntry.tsx` | W0 |
| **W2 组件库** | 13 个新组件（第 14 个 PageShell 由 W1 交付）+ 现有 15 个升级 + 组件测试 | `components/ui/*`、`styles/ui-optimization/foundation.css` | `Section/FormField/Select/SearchField/Tabs/FilterBar/ListRow/Inspector/StatTile/ProgressBar/ConfirmDialog/CodeBlock/KeyValueRow.tsx` 及测试 | W0 |
| **W3 对话工作台** | Chat 页、消息、输入舱、执行轨、审批、Markdown | `pages/ChatPage.tsx`、`components/chat/*`、`features/execution/*`、`components/approval/ApprovalCard.tsx`、`styles/shell.css`（chat 段）、`styles/ui-optimization/chat.css` | 可能拆出 `chat/messageActions.tsx` 等 | W1、W2 |
| **W4 任务与工作空间** | 任务中心、空间列表/详情、任务画布外壳 | `pages/TaskCenterPage.tsx`、`WorkspacesPage.tsx`、`WorkspaceDetailPage.tsx`、`components/tasks/*`、`features/tasks/*`、`features/task-world/*`（外壳与呈现层） | `features/tasks/TaskListPanel.tsx` 等 | W1、W2 |
| **W5 工作流** | 列表、编辑器外壳、运行检视 | `pages/WorkflowsPage.tsx`、`components/workflow/*`、`styles/ui-optimization/workflows.css`、`styles/responsive.css`（workflow 段） | — | W1、W2 |
| **W6 能力/记忆/知识** | 四能力页、记忆、知识库 | `pages/AgentsPage.tsx`、`SkillsPage.tsx`、`PluginsPage.tsx`、`CapabilitiesPage.tsx`、`MemoryPage.tsx`、`KnowledgePage.tsx`、`components/capabilities/*`、相关 features、`styles/capability.css`、`ui-optimization/capabilities.css`、`memory-knowledge.css` | `features/capabilities/CapabilityLayout.tsx` 等 | W1、W2 |
| **W7 设置/监控/日志** | 六组设置、监控仪表盘、日志控制台 | `pages/SettingsPage.tsx`、`SystemPage.tsx`、`LogsPage.tsx`、`features/settings/*`、`features/llm/*`、`features/security/*`、`components/system/*`、`styles/settings.css`、`system.css`、`ui-optimization/settings-system.css` | 监控仪表盘组件 | W1、W2 |
| **W8 账号/搜索/初始化** | 三页视觉方言统一 + 交互修正 | `pages/AccountCenterPage.tsx`、`SearchPage.tsx`、`SetupWizardPage.tsx`、`features/cloud/*`、`styles/cloudExperience.css` | — | W1、W2 |
| **W9 验收与集成** | 集成接线、E2E、截图矩阵、文档、最终构建 | 只读业务源码；可改测试与 E2E | `frontend/e2e/ui-upgrade.mjs`、`target/ui-upgrade/` 证据、实施记录文档 | 全部 |

**公共冲突文件（只能由对应包或集成负责人改）：** `styles/base.css`、`styles/stylesheet.ts`、`index.css`、`tailwind.config.js`、`theme/*`、`AppShell.tsx`、`components/ui/index.ts`、`package.json`。页面包不得自行修改这些文件；需要时向 W0/W1/W2 负责人提出。

### 9.2 阶段与退出条件

| 阶段 | 内容 | 退出条件 |
|---|---|---|
| **P0 契约固定** | W0 + W1 设计评审 + 基线截图（明/暗 × 6 尺寸） | token 扫描测试通过；基线截图入库；合同测试基线全绿 |
| **P1 基础与公共层** | W0、W1、W2 完成并集成 | `npm test`、`npm build` 通过；组件 demo/契约测试通过 |
| **P2 页面并行** | W3–W8 并行推进 | 每包独立通过自己的页面验收与局部截图 |
| **P3 集成** | 接线、跨页检查、回归修复 | 全量测试、构建、E2E、截图矩阵通过 |
| **P4 证据归档** | 文档、已知限制、回滚说明 | 交付清单完整，未验证项明确标注 |

关键约定：

- W3–W8 可以在 W2 的新组件尚未全部完成时先用旧组件完成布局与状态，**只有使用新组件的接线步骤依赖 W2 完成**。
- 同一文件不允许两个包同时改；跨包需求通过任务卡协调。
- 集成负责人统一处理 `index.css`/`stylesheet.ts`/公开导出，避免多条线各自加样式。

依赖关系（实线为硬依赖，虚线为可并行前置）：

```text
W0 ──┬──→ W1 ──┬──→ W3 对话
     │         ├──→ W4 任务/空间
     └──→ W2 ──┼──→ W5 工作流
               ├──→ W6 能力/记忆/知识
               ├──→ W7 设置/监控/日志
               └──→ W8 账号/搜索/初始化
                          │
        所有页面包 + W1/W2 ─┴──→ W9 集成验收
```

注意：W1 的 NavRegistry 与 PageShell 是页面包的硬前置；W3–W8 之间的样式文件互不重叠，可真正并行。若只能单线推进，推荐顺序 W0 → W1 → W2 → W3 → W6 → W4 → W7 → W5 → W8 → W9（先修最影响观感与暗色的部分）。

### 9.3 分支与提交

- 分支：`feat/ui-page-upgrade`（从 `develop` 切出）；若并行，使用独立 worktree，禁止同一检出多会话切分支。
- 提交粒度（示例）：
  - `refactor(theme): extract token registry and freeze cyrene palette`
  - `feat(ui): add PageShell and route error boundary`
  - `refactor(nav): derive navrail and command palette from one registry`
  - `fix(chat): make markdown and approval surfaces theme-adaptive`
  - `feat(ui): add Select, SearchField and Tabs`
  - `refactor(tasks): split task center into list and inspector`
  - `test(ui): add token coverage and component contract tests`
- 不提交构建产物、不提交 `target/` 证据目录以外的临时文件；不修改无关代码。

### 9.4 规模估计（相对量级，不是工时承诺）

| 包 | 规模 | 风险 |
|---|---|---|
| W0 | S | 低（测试保护） |
| W1 | L | 中高（导航三处联动、合同测试） |
| W2 | L | 中（组件多但独立） |
| W3 | L | 高（暗色与性能，触及核心体验） |
| W4 | M | 中（信息架构变化） |
| W5 | M | 中（画布外壳） |
| W6 | L | 中高（四页结构统一 + 合同测试） |
| W7 | M | 中（保存语义、日志键盘） |
| W8 | S–M | 低中 |
| W9 | M | 中（证据与回归） |

---

## 10. 验收与证据

### 10.1 自动化命令

```powershell
cd frontend
npm.cmd test
npm.cmd run build

# 浏览器/桌面 E2E（需按仓库现有方式启动后端与前端；不在此处新建第二套启动脚本）
node e2e/core-paths.mjs
$env:YILIAN_E2E_UI_MATRIX = "final"
node e2e/core-paths.mjs
```

### 10.2 测试要求

| 类型 | 要求 |
|---|---|
| 单元/组件测试 | 新增/修改的组件必须有测试：状态覆盖（hover/focus/disabled/loading/error）、a11y 属性、键盘行为 |
| token 扫描测试 | 零未定义变量；非白名单 hex/非昔涟色为零；`base.css` 与 `presets.ts` 逐值相等 |
| 合同测试 | 18 个文件全部通过；行为变化必须更新断言并说明原因，禁止删除断言 |
| E2E | 覆盖核心路径：聊天发送/停止/审批、任务详情与决策提交、工作空间创建、工作流运行、能力页、记忆编辑、设置保存、日志键盘、账号/搜索空态 |
| 视觉 | 明/暗 × 1200×800、1366×768、1600×900、1920×1080、2560×1440；缩放 100/125/150%；背景图开/关 |
| 主题冻结 | token diff 为空；静态扫描零违规；截图色相无偏移；对比度脚本通过（正文 ≥4.5:1、焦点 ≥3:1） |
| 响应式回归 | 800×600、1024×700、1366@125%、1366@150%；重点回归任务页 980–1099 空白行、工作流 720 高度裁切 |
| 强制颜色/减少透明 | Windows 高对比度模式抽查关键页；`prefers-reduced-transparency` 下玻璃回退后可读 |
| 性能 | 200 条消息流式发送不卡顿；1000 行日志滚动流畅；页面切换无白屏；背景/角色图体积下降；CSS 中 blur 声明收敛 |
| 可访问性 | 全页面 Tab 走查；无不可见的可聚焦元素；Esc/Tab/方向键符合附录 E；无 `aria-selected` 误用 |

### 10.3 截图与几何证据

- 实施前先按现有 `YILIAN_E2E_UI_MATRIX=baseline` 采集 `target/ui-baseline/`，作为「主题未变」的对照证据。
- 证据目录：`target/ui-upgrade/<run-id>/`（与现有 `target/ui-foundation` 习惯一致）。
- 每页每尺寸至少一张截图；关键页（Chat/Tasks/Capabilities/Memory/System/Logs/Settings）另存交互态截图（空/错/加载/选中/审批/暗色）。
- 使用现有 `e2e/ui-foundation.mjs` 的 `collectGeometry` 思路检查：页头高度、操作按钮是否可见、是否横向溢出、双滚动条、遮挡关系、主操作命中区域。
- DOM 抽查：无 `.sr-only` 可聚焦元素；无 `aria-selected` 误用；无重复 id；无 `outline:none` 后无焦点样式。
- 与基线截图对比只允许「结构、间距、层级」差异；色相明显偏移即视为违反主题冻结。

### 10.4 逐包验收门槛

- **G0**：W0 通过 token 扫描；`npm test` 绿。
- **G1**：W1 完成 PageShell/NavRegistry；18 页无独立页头；导航三处联动测试绿；sr-only 可聚焦元素为 0。
- **G2**：W2 组件全部有测试且导出；页面不再出现原生 select/搜索框的重复样式（未被替换的登记）。
- **G3**：W3–W8 每包页面验收清单（见各页验收）+ 局部截图；暗色无不可读表面。
- **G4**：W9 全量测试、构建、E2E、截图矩阵、几何检查全部通过；未验证项明确标注 `NOT TESTED`。

### 10.5 完成定义（Definition of Done）

1. 主题冻结证据齐全（token diff 为空、扫描零违规、截图色相无偏移）。
2. 18 个页面全部使用 PageShell，且每页有加载/空/错误/选中状态。
3. 新增 14 个组件全部被至少一个页面消费，或有明确保留理由。
4. 18 个合同测试与 126+ 单元测试通过，生产构建通过。
5. 关键路径 E2E 通过，截图矩阵归档。
6. 文档更新：README/文档中涉及 UI 的描述与实现一致；本轮实施记录写入 `docs/v1.2/`（或届时约定目录）。
7. 已知限制与未验证项写明（尤其是 Tauri 桌面端、DPI 缩放、语音、真实后端数据）。
8. 附录 H 的 14 项样式治理完成并记录前后指标（未定义 token 0、非昔涟色 0、`!important` 减少 ≥50%、字面量字号显著下降、图片体积下降）。
9. 主题冻结三份证据（源码 diff、静态扫描、截图/对比度）齐全，且没有任何未登记的 token 值变更。

---

## 11. 风险、回滚与开放问题

### 11.1 风险

| 风险 | 影响 | 缓解 |
|---|---|---|
| 导航重构触碰合同测试与用户偏好数据 | 中 | W1 单独提交；测试覆盖三个联动；`productPreferences` 数据格式不变 |
| 页面结构升级导致回归 | 高 | 每页独立提交 + 局部截图；先结构后样式；保持路由与 API 不变 |
| 暗色修复引入浅色模式偏差 | 中 | 双模式截图对比；只在 token 内替换 |
| 流式性能优化造成消息显示异常 | 中高 | 不改变 reducer/SSE 语义；只改渲染节流；保留回退开关（常量） |
| 新增组件与既有页面样式冲突 | 中 | 组件替代逐页进行；全局样式只由 W0/W1/W2 改；每次构建验证 |
| 合同测试大面积失效 | 中 | 先读测试再改结构；行为变化显式更新断言并记录 |
| 设计过度（加太多新组件） | 中 | 14 个组件必须有消费方；无消费方的组件不合并 |

### 11.2 回滚策略

- 每个工作包独立提交，可按包 revert。
- 页面重构保留原页面组件文件直到新结构通过验收（先新增、后切换、再删除旧实现）。
- 主题冻结相关的 W0 先落地且不改视觉；若后续页面改动出问题，回滚页面包即可，不影响主题。
- 出现严重回归时优先回滚最近一个工作包的合并提交，不在坏基线上继续叠加。

### 11.3 开放问题（需产品/负责人确认）

1. **任务中心信息架构**：是否接受「任务/会话/任务图」三 Tab 主列表 + 右侧详情？还是保留 TaskGraph 控制面在首屏？
2. **保存入口**：设置页保留页头「保存设置」+ 吸底栏两个入口（共享状态），还是只保留吸底栏？
3. **安全入口形态**：NavRail 底部加「安全」入口是否可接受？全局急停需要后端提供跨页面运行标识，是否本轮只做「当前会话停止」？
4. **中性色可读性**：浅色下 `--text-faint`（`#9690A1`）对背景对比 2.88:1，低于正文标准。建议不修改 token，仅限制其用于非必要标签；若允许微调中性色，需要单独批准并记录。
5. **搜索页自动建工作区**：当前「加入任务」会自动创建「搜索研究」工作区，是否改为让用户选择空间？
6. **日志暂停时的刷新语义**：暂停状态下点击刷新是否应拉取一次新日志？
7. **Tauri 桌面验收**：本轮是否必须在 Tauri WebView 内截图验收，还是允许浏览器等价验收并标注限制？

---

## 附录 A 冻结 token 全表

> 来源：`frontend/src/theme/presets.ts` 的 `CYRENE_SEMANTIC_TOKENS`。**58 个键，light/dark 全部冻结。**

### A.1 画布与表面

| token | light | dark |
|---|---|---|
| `--bg-app` | `#F8F6FD` | `#171421` |
| `--bg-soft` / `--bg-subtle` | `#F2EEFA` | `#1D1929` |
| `--surface` | `rgba(255,255,255,0.42)` | `rgba(37,31,53,0.54)` |
| `--surface-muted` | `rgba(252,249,255,0.24)` | `rgba(45,38,63,0.42)` |
| `--surface-elevated` | `rgba(255,255,255,0.58)` | `rgba(49,41,68,0.68)` |
| `--surface-solid` | `#FFFDFF` | `#252032` |
| `--surface-hover` | `rgba(249,243,252,0.62)` | `rgba(67,55,88,0.72)` |
| `--titlebar-bg` | `rgba(255,255,255,0.32)` | `rgba(29,25,41,0.72)` |

### A.2 文字

| token | light | dark |
|---|---|---|
| `--text-primary` | `#292536` | `#F3EFF8` |
| `--text-secondary` | `#696276` | `#C7BED2` |
| `--text-faint` | `#9690A1` | `#91879E` |
| `--text-disabled` | `#BDB8C5` | `#6E6678` |

### A.3 强调色

| token | light | dark |
|---|---|---|
| `--accent-primary` | `#ea91b9` | `#F0A3C6` |
| `--accent-primary-hover` | `#DF7EAA` | `#F5B3D0` |
| `--accent-contrast` | `#292536` | `#241D2C` |
| `--accent-soft` | `#F9DCE9` | `rgba(240,163,198,0.18)` |
| `--accent-purple` | `#AD9BE8` | `#B8A7F0` |
| `--accent-blue` | `#99CFEA` | `#8CC9E8` |
| `--accent-gold` | `#EBCF8C` | `#E5C875` |
| `--accent-border` | `rgba(234,145,185,0.26)` | `rgba(240,163,198,0.30)` |
| `--accent-soft-fg` | `#67435A` | `#F3C4D9` |
| `--focus-ring-soft` | `rgba(234,145,185,0.30)` | `rgba(240,163,198,0.32)` |

### A.4 边框与状态

| token | light | dark |
|---|---|---|
| `--divider` | `rgba(91,76,125,0.11)` | `rgba(220,210,235,0.12)` |
| `--border-soft` | `rgba(86,72,117,0.12)` | `rgba(220,210,235,0.14)` |
| `--success-soft` / `-border` / `-fg` | `rgba(115,185,154,0.14)` / `rgba(115,185,154,0.34)` / `#315D49` | `rgba(116,192,157,0.16)` / `rgba(116,192,157,0.36)` / `#A8E0C5` |
| `--warning-soft` / `-border` / `-fg` | `rgba(217,184,102,0.16)` / `rgba(217,184,102,0.36)` / `#72571D` | `rgba(226,191,103,0.17)` / `rgba(226,191,103,0.38)` / `#F2D993` |
| `--danger-soft` / `-border` / `-fg` | `rgba(223,121,149,0.14)` / `rgba(223,121,149,0.34)` / `#873F59` | `rgba(232,126,157,0.17)` / `rgba(232,126,157,0.38)` / `#F4B0C3` |
| `--info-soft` / `-border` / `-fg` | `rgba(132,188,220,0.15)` / `rgba(132,188,220,0.34)` / `#37677E` | `rgba(132,197,229,0.16)` / `rgba(132,197,229,0.36)` / `#B5DDF1` |

> 注：上表中的 `-border` / `-fg` 是完整 token 名的缩写，例如 `--success-soft` / `--success-border` / `--success-fg` 三个键；warning、danger、info 同理。

### A.5 侧栏（永远深色）

| token | light | dark |
|---|---|---|
| `--sidebar-bg` | `#29263A` | `#1D1929` |
| `--sidebar-bg-2` | `#312C46` | `#252035` |
| `--sidebar-text` / `--sidebar-fg` | `#EEEAF8` | `#F0EBF7` |
| `--sidebar-muted` | `#AAA3BA` | `#AAA1B7` |
| `--sidebar-active` | `rgba(234,145,185,0.17)` | `rgba(240,163,198,0.18)` |

### A.6 昔涟专有

| token | light | dark |
|---|---|---|
| `--cyrene-glow-pink` | `rgba(234,145,185,0.22)` | `rgba(240,163,198,0.18)` |
| `--cyrene-glow-purple` | `rgba(173,155,232,0.18)` | `rgba(184,167,240,0.16)` |
| `--cyrene-glow-blue` | `rgba(153,207,234,0.14)` | `rgba(140,201,232,0.12)` |
| `--cyrene-glass-border` | `rgba(255,255,255,0.52)` | `rgba(239,232,248,0.15)` |
| `--cyrene-surface-tint` | `rgba(255,255,255,0.34)` | `rgba(60,50,80,0.30)` |
| `--cyrene-night` | `#29263A` | `#171421` |
| `--cyrene-water` | `rgba(153,207,234,0.18)` | `rgba(140,201,232,0.14)` |
| `--cyrene-ripple` | `rgba(173,155,232,0.18)` | `rgba(184,167,240,0.16)` |

### A.7 阴影、代码、遮罩、环境

| token | light | dark |
|---|---|---|
| `--shadow-card` | `0 4px 16px rgba(54,45,79,0.05)` | `0 5px 18px rgba(5,4,10,0.20)` |
| `--shadow-float` | `0 12px 36px rgba(48,38,76,0.11)` | `0 14px 40px rgba(5,4,10,0.32)` |
| `--code-bg` | `#252132` | `#11101A` |
| `--code-text` | `#F4F0F8` | `#EEEAF5` |
| `--shell-overlay` | `rgba(248,246,253,0.12)` | `rgba(17,14,27,0.66)` |
| `--page-wash` | `rgba(248,246,253,0.18)` | `rgba(23,20,33,0.28)` |
| `--shell-art-opacity` | `0.72` | `0.26` |

### A.8 共享刻度（两模式相同）

`--radius-xs 6px`、`--radius-sm 10px`、`--radius-md 12px`、`--radius-lg 16px`、`--radius-xl 20px`、`--motion-fast 140ms`、`--motion-normal 220ms`。

---

## 附录 B 派生 token 与新增 token 提案

> 只允许使用附录 A 中已有色相的 alpha/`color-mix`；不得引入新色相。

| token | 值（示意） | 用途 |
|---|---|---|
| `--shadow-sm` | `0 2px 8px rgba(54,45,79,0.06)` | 修复被引用但未定义的阴影 |
| `--input-inset-shadow` | `inset 0 1px 2px rgba(41,38,58,0.04)`（dark 调低） | 输入框内阴影 token 化 |
| `--overlay-scrim` | = `--backdrop` 语义别名 | 浮层遮罩统一 |
| `--risk-low-fg/-soft/-border` | 映射 `--info-*` | 审批/工具低风险 |
| `--risk-medium-fg/-soft/-border` | 映射 `--warning-*` | 中风险 |
| `--risk-high-fg/-soft/-border` | 映射 `--danger-*` | 高风险 |
| `--risk-critical-fg/-soft/-border` | 同 high，但 border 加强 | 严重风险 |
| `--z-voice-dock` | `35` | 语音 dock 层级 |
| `--z-savebar` | `55` | 设置吸底栏 |
| `--shell-readability-gradient` | 由 `--page-wash`/`--shell-overlay` 组合 | 背景图可读性遮罩 |
| `--code-header-bg` | `color-mix(code-bg, white 6%)` | 代码块标题栏 |
| `--row-selected-bg` | `color-mix(--accent-soft, transparent)` | 列表选中态 |
| `--page-padding-inline/block` | 已存在，保留 | 页面边距 |
| `--foreground` / `--foreground-muted` | 别名 → `--text-primary` / `--text-secondary` | 消除 20 处未定义引用（各 9/11 处） |
| `--border-strong` / `--border-hover` | 别名 → `--divider` / `--accent-border` | 消除 8 处未定义引用 |
| `--accent-subtle` / `--danger-subtle` / `--success-subtle` / `--warning-subtle` | 别名 → 对应 `--*-soft` | 删除 `#ec4899/#22c55e/#ef4444` 兜底 |
| `--text-tertiary` | 别名 → `--text-secondary` | 统一补充信息层级 |
| `--surface-input` | 别名 → `--input-bg` | 输入类控件统一 |
| `--panel-muted` | 别名 → `--surface-muted` | 兼容旧引用 |

**登记规则：** 所有派生 token 集中在 `theme/tokens.ts` 的 `DERIVED_TOKENS` 中声明，并有测试防止未登记变量被使用。

---

## 附录 C 组件 API 变更清单

| 组件 | 类型 | 关键变更 |
|---|---|---|
| PageShell | 新增 | `title/description/status/actions/variant/maxWidth` |
| Section | 新增 | `title/description/actions/collapsible/defaultOpen` |
| FormField | 新增 | `label/hint/error/required/children` |
| Select | 新增 | `options/label/hint/error/size` |
| SearchField | 新增 | `value/onChange/onClear/slashShortcut/aria-label` |
| Tabs | 新增 | `items/value/onChange/variant/aria-label` |
| FilterBar | 新增 | `search/filters/counts/actions` |
| ListRow | 新增 | `selected/onSelect/leading/title/meta/trailing/statusBar/onActivate` |
| Inspector | 新增 | `title/meta/actions/tabs/empty/children` |
| StatTile | 新增 | `label/value/unit/detail/progress/ring/status` |
| ProgressBar | 新增 | `value/max/variant/label` |
| ConfirmDialog | 新增 | `open/title/description/tone/requireText/confirmLabel/onConfirm/onCancel` |
| CodeBlock | 新增 | `code/language/copyable/lineNumbers/maxHeight` |
| KeyValueRow | 新增 | `label/value/mono/wrap` |
| Button | 升级 | 默认 `type="button"`；`aria-disabled` loading；`iconOnly`；尺寸收敛 |
| Input / Textarea | 升级 | 基于 FormField；修复 Textarea id；密码显示切换 |
| Panel | 升级 | 统一 L2 表面；删除死 `interactive` 或补齐键盘语义 |
| PageHeader | 升级/兼容 | 内部委托 PageShell.Header；`title` 支持 ReactNode |
| Badge / StatusBadge | 升级 | 统一 tone + `RiskBadge` + `StatusBadge` |
| Modal | 升级 | `--backdrop`、退出动画、size xl、`variant="danger"`、footer sticky |
| Drawer | 升级 | z token、宽度断点、遮罩统一、`aria-valuetext` |
| Tooltip | 升级 | `aria-describedby`、滚动重算、触屏长按 |
| EmptyState | 升级 | `variant` 四种；token 插画 |
| ErrorState | 升级 | `severity/details/onRetry` |
| Skeleton | 升级 | 形状变体 + `aria-busy` |
| Spinner | 升级 | reduced-motion 可见替代 |
| Toast | 升级 | token、`aria-live`、最多 3 条、动作、避让、真实接入 |
| CommandPalette | 升级 | token、listbox 语义、最近使用、焦点归还、hover 不抢选中 |
| NavRail | 升级 | 注册表驱动、真实安全入口、矮窗可滚动、删除 sr-only |
| ModuleSetup | 迁移 | 从 NavRail 迁至设置页「工作台与导航」卡片 |
| ApprovalCard | 升级 | 风险 token 化、卡内确认、删除全局 y/n |

---

## 附录 D 页面 → 文件 → 工作包映射

| 页面 | 路由 | 主文件 | 工作包 |
|---|---|---|---|
| 对话工作台 | `/chat`、`/chat/:id` | `pages/ChatPage.tsx`、`components/chat/*`、`features/execution/*` | W3 |
| 任务中心 | `/tasks` | `pages/TaskCenterPage.tsx`、`components/tasks/TaskDetailPanel.tsx`、`features/tasks/*` | W4 |
| 任务画布 | `/task-world/:graphId` | `features/task-world/TaskWorldPage.tsx` 及 canvas/inspector/goal-view | W4 |
| 工作流 | `/workflows` | `pages/WorkflowsPage.tsx`、`components/workflow/*` | W5 |
| 工作空间 | `/workspaces`、`/workspaces/:id` | `pages/WorkspacesPage.tsx`、`WorkspaceDetailPage.tsx` | W4 |
| 智能体 | `/agents` | `pages/AgentsPage.tsx`、`features/capabilities/*` | W6 |
| 技能 | `/skills` | `pages/SkillsPage.tsx`、`features/skills/*` | W6 |
| 插件 | `/plugins` | `pages/PluginsPage.tsx`、`features/mcp/*` | W6 |
| 能力矩阵 | `/capabilities` | `pages/CapabilitiesPage.tsx`、`components/capabilities/*` | W6 |
| 记忆中心 | `/memory` | `pages/MemoryPage.tsx`、`features/memory/*` | W6 |
| 知识库 | `/knowledge` | `pages/KnowledgePage.tsx` | W6 |
| 设置 | `/settings` | `pages/SettingsPage.tsx`、`features/settings/*` | W7 |
| 系统监控 | `/system` | `pages/SystemPage.tsx`、`components/system/*` | W7 |
| 运行日志 | `/logs` | `pages/LogsPage.tsx` | W7 |
| 账号中心 | `/account` | `pages/AccountCenterPage.tsx`、`features/cloud/*` | W8 |
| AI 搜索 | `/search` | `pages/SearchPage.tsx`、`features/cloud/cloudPresentation.ts` | W8 |
| 初始化 | `/setup` | `pages/SetupWizardPage.tsx` | W8 |
| 全局外壳 | 所有页面 | `components/layout/*`、`AppShell.tsx`、`components/ui/*` | W1/W2 |

---

## 附录 E 快捷键、焦点与键盘规则

### E.1 全局快捷键（保持现有语义）

| 快捷键 | 范围 | 动作 | 备注 |
|---|---|---|---|
| `Ctrl/Cmd + K` | 全局 | 打开命令面板 | 已有 |
| `Ctrl/Cmd + ,` | 全局 | 打开设置 | 已有 |
| `Alt + 1` | 全局 | 任务中心 | |
| `Alt + 2` | 全局 | 对话 | |
| `Alt + 3` | 全局 | 搜索 | |
| `Alt + 4` | 全局 | 工作流 | |
| `Alt + 5` | 全局 | 工作空间 | |
| `Alt + 6` | 全局 | 能力矩阵 | |
| `Ctrl/Cmd + B` | 聊天页 | 切换会话侧栏（窄窗） | 已有 |
| `Ctrl/Cmd + J` | 聊天页 | 切换执行轨（full）/抽屉（compact/narrow） | 已有 |
| `/` | 有搜索框的管理页 | 聚焦当前页搜索框（非输入态） | 统一启用 |
| `Esc` | 全局 | 关闭最上层浮层/辅助栏 | 一次一层 |
| `↑ / ↓` | 列表获得焦点 | 换选，不循环 | |
| `Home / End` | 列表 | 首/尾 | |
| `Enter / Space` | 焦点行 | 激活 | |
| `Ctrl/Cmd + S` | 设置页 | 保存 | 新增（页面内） |
| `?` | 全局（非输入态） | 快捷键帮助 | 可选 |

实现要求：快捷键配置来自 NavRegistry；UI 提示真实键位；控件加 `aria-keyshortcuts`；输入法合成中不触发；有模态浮层时不触发页面快捷键。

### E.2 浮层层级与 Esc

`Voice pill < Toast < 设置吸底栏 < Drawer < Modal < CommandPalette`（同一时刻只允许一个模态浮层）。

### E.3 焦点归还

- Modal/Drawer/CommandPalette 关闭后回到触发按钮；触发按钮已卸载时回到页面标题。
- 列表详情 Drawer 关闭后回到对应列表行。
- 删除确认取消后回到删除按钮。

---

## 附录 F 状态与文案规范

### F.1 任务状态

| 枚举 | 中文 | tone |
|---|---|---|
| draft | 草稿 | default |
| ready | 待开始 | info |
| running | 运行中 | running |
| waiting_approval | 等待确认 | warning |
| waiting_user | 等待用户 | warning |
| blocked | 受阻 | danger |
| completed | 已完成 | success |
| failed | 失败 | danger |
| cancelled | 已取消 | default |

### F.2 执行状态

| 枚举 | 中文 | tone |
|---|---|---|
| running | 执行中 | running |
| awaiting_approval | 等待确认 | warning |
| succeeded | 成功 | success |
| failed | 失败 | danger |
| rejected | 已拒绝 | danger |
| cancelled / interrupted | 已取消 / 已中断 | default |
| verification failed | 验证未通过 | danger |
| verification skipped/not_requested | 未验证 | default |

### F.3 插件/MCP 状态

| 状态 | 中文 | 说明 |
|---|---|---|
| disabled | 已停用 | 未启用 |
| enabled_disconnected | 已启用 · 未连接 | 启用但连接失败/未建立 |
| connecting | 连接中 | |
| ready | 已就绪 | 可调用工具 |
| error | 连接异常 | 显示真实错误 |

### F.4 通用动作文案

`保存 / 保存中… / 已保存`、`创建 / 创建中…`、`删除 / 删除中…`、`重试 / 重新加载`、`停止 / 停止中…`、`批准 / 拒绝`、`前往设置 / 前往记忆中心`。

---

## 附录 G 合同测试影响清单

> 修改页面结构前先读这些测试；任何文案/类名/行为变化都要同步更新断言并说明原因。

| 合同测试 | 约束 |
|---|---|
| `theme/surfaceContract.test.ts` | AppShell 持有画布；所有业务页含 `page-canvas`；页面根透明；无整页 blur |
| `pages/systemCenterFreezeContract.test.ts` | system/logs/settings 路由；NavRail `min-[960px]:w-[80px]`；设置页类名 |
| `pages/systemSettingsContract.test.ts` | 设置页结构、深链、密钥语义 |
| `pages/systemMonitorContract.test.ts` | 监控页真实字段与状态 |
| `pages/systemLogsContract.test.ts` | 日志页键盘、Drawer、aria |
| `pages/capabilitiesCapabilityContract.test.ts` | 禁用「执行能力」「切换启用」等虚构文案 |
| `pages/capabilityFreezeContract.test.ts` | 能力页类名（如 `capability-page skills-page page-canvas`） |
| `pages/agentsCapabilityContract.test.ts` | 禁用「删除 Agent」「在线」「健康」等 |
| `pages/skillsCapabilityContract.test.ts` | 技能页类名与 PageHeader |
| `pages/pluginsCapabilityContract.test.ts` | 禁用「Marketplace」「API Key」等 |
| `pages/memoryKnowledgeContract.test.ts` | 「没有独立知识库数据源」「尚未接入」「没有匹配的记忆记录」等文案 |
| `pages/workflowEditorContract.test.ts` | 工作流编辑器接口与行为 |
| `pages/routeLoadingContract.test.ts` | 路由加载骨架 |
| `components/capabilities/capabilityComponentsContract.test.ts` | 空值隐藏规则 |
| `components/system/systemComponentsContract.test.ts` | 系统组件渲染契约 |
| `components/chat/resourceIngestContract.test.ts` | 资源附件行为 |
| `components/chat/voiceRefreshContract.test.ts` | 语音刷新行为 |
| `components/layout/shellRailsVisual.test.ts` / `navGroups.test.ts` | 导航分组与可见性 |

**更新原则：**

1. 行为变化 → 更新断言，并在提交信息/PR 中说明；
2. 纯视觉重构 → 保留原有断言所需的类名/文案，若必须调整，同时增加等价的新断言；
3. 禁止为了绿灯删除断言、放宽断言或把真实行为改成占位；
4. 合同测试之外，新增 `themeTokenCoverage.test.ts`、`pageShellContract.test.ts`、`componentA11yContract.test.ts` 三个测试守护本轮规范。

---

*本文是实施前方案。任何与现有源码冲突之处，以实施时的真实源码、合同测试与冻结文档为准；发现冲突应在对应任务卡中记录，不得凭旧文档修改代码。*

---

## 附录 H 可安全执行的样式治理清单（W0 输入）

> 以下动作都**不改变昔涟配色**，只消除双源、死变量、层叠债务与性能浪费；按顺序执行，每步可独立验证。

| # | 动作 | 证据 | 验收 |
|---|---|---|---|
| H1 | 由 `CYRENE_SEMANTIC_TOKENS` 生成 `base.css` 的 `:root`，对齐首屏 fallback 与运行态 | `base.css:8-139` 与 `presets.ts:212-338` 17 处漂移 | 新测试断言两边逐值相等；首屏无颜色闪变 |
| H2 | 为 12 个缺失变量建别名或删除引用 | `--foreground(9)`、`--foreground-muted(11)`、`--border-strong(5)`、`--border-hover(3)`、`--shadow-sm(3)`、`--accent-subtle(2)` 等 | 未定义 `var()` 扫描为 0 |
| H3 | 删除非昔涟兜底色 | `memory-knowledge.css:101/281/476` 的 `#ec4899/#22c55e/#ef4444` | 全 CSS 无 Tailwind 默认色 |
| H4 | `proofloopGoalView.css` 纳入 `index.css` 与 `stylesheet.ts`，去掉 `TaskWorldPage.tsx:55` 直接 import | 磁盘 22 个文件、清单 21 条 | 清单与磁盘一致；测试可读取该文件 |
| H5 | 用现有 dark token 补 Markdown/工具卡/审批/错误条，深色下 `multiply` 改 `normal` | `shell.css:171-347`、`workspace.css:392` | 深色对比度 ≥4.5:1；截图通过 |
| H6 | 焦点改为双层环，soft 仅作装饰；补搜索输入焦点 | `base.css:161`、`system.css:133`、`cloudExperience.css:56` | 焦点态对比 ≥3:1；12 处 `outline:none` 清除 |
| H7 | 增加 `forced-colors` 与 `prefers-reduced-transparency` 兜底 | 当前 0 条 | Windows 高对比度下按钮/焦点可见 |
| H8 | z-index：新增 `--z-overlay` 与局部负层 token，替换 25 处裸值 | `Drawer.tsx:108`、`NavRail.tsx:275`、`ChatHeader.tsx:108-112`、`workspace.css:386-433` 等 | 裸 z-index 为 0；6 个未用 token 标注或删除 |
| H9 | 字号/行高/字重收敛：10/11px→12px 档，13/14/18/22→token，删除 650/750/550 | 109 处字面量、365 处 `text-xs`、73 处 <12px | 字面量 font-size 显著下降；字体只加载四档 |
| H10 | 断点合并（899→900、1099→1100），清任务页空白行，工作流高度改 `min()` | `responsive.css`、`shell.css:592`、`tasks-workspaces.css:33` | 800/1024/1200/1366/1920 与 125%/150% 无错位 |
| H11 | 确认气泡唯一所有者后，删除三方 `!important` 覆盖 | `shell.css:141`、`workbench.css:54`、`ui-optimization/chat.css:44` | 助手气泡只有一处背景声明；`!important` 减少 ≥50% |
| H12 | 删除死代码：3 个 keyframes、7 处 `transition-all`、无限 `box-shadow` 动画、未使用 token | `workflows.css:51`、`animations.css`、z 命名空间 | 构建 CSS 体积下降；无无效动画 |
| H13 | PNG → WebP（同图），图片加尺寸；背景 blur 收敛 | 765KB/309KB；19 条 `backdrop-filter` | 首屏资源体积显著下降；视觉一致 |
| H14 | 合并 16 个宽度断点为 5 档；需要分栏的地方用 `@container` | 51 条媒体查询 | 断点清单集中在文档与常量 |

---

## 附录 I 页面验收清单（可直接复制进任务卡）

| 页面 | 必须通过 |
|---|---|
| Chat | 待审批 1 击可达；当前动作显示运行中；暗色正文/代码可读；输入五态；860px 阅读宽；200 条消息流式不卡 |
| Tasks | 首屏 ≥3 条任务 + 详情；筛选计数与真实状态一致；决策 options 可提交；时间线不重复；Drawer 闭环 |
| Workspaces | 卡片真实计数；复制与 Toast；创建/校验/错误；详情真实任务 |
| Workflows | Tabs 语义；全高编辑器；保存/运行失败不静默；空画布 CTA；节点状态 token；fit 正确 |
| Agents | 列表+右详情；搜索；卡片 3 行；来源深链；无假删除/假在线 |
| Skills | 阅读器结构；单复制入口；草稿守卫；删除确认；竞态修复 |
| Plugins | 状态机五态；真实字段；启用≠连接；操作收拢；加载/错误不吞页头 |
| Capabilities | 分组矩阵 + sticky 表头；风险 Badge；空态一致；无冗余入口 |
| Memory | 单分类入口；搜索去抖无闪烁；详情/编辑/删除闭环；触屏操作常显 |
| Knowledge | 无重复说明；双分区徽章；无假数据；动作可达 |
| Settings | 深链回写；保存成功/失败/重试；离开守卫；外观缩略图真实；模块与导航入口可用 |
| System | 单一 dashboard；真实字段；刷新状态真实；无英文占位；窄窗单列 |
| Logs | 方向键不弹抽屉；焦点可见；暂停语义真实；清空确认；窄窗布局 |
| Account | PageShell 一致；无原生 prompt；空态；错误就地显示 |
| Search | 加载/空/错误；动作 loading；不擅自创建工作区；配置折叠 |
| Setup | 状态不闪变；真实完成条件；Form 控件一致；独立页 token 统一 |

---

## 附录 J 页面 × 组件映射（升级后）

> 用于检查「页面只组合、不复制」；`●` 主要使用，`○` 局部/可选。未列出的页面内组件（如画布、日志行、监控卡）保留页面自有实现，但样式必须走 token。

| 页面 | 新增公共组件 | 升级后的公共组件 | 页面自有保留 |
|---|---|---|---|
| Chat | `PageShell(workspace)` ●、`Section` ○、`Inspector` ●、`ConfirmDialog` ○ | `Button`、`EmptyState`、`ErrorState`、`Tooltip`、`Drawer`、`StatusBadge`、`Toast`、`CodeBlock` | ChatHeader、消息流、执行轨迹、输入舱 |
| TaskCenter | `PageShell` ●、`FilterBar` ●、`ListRow` ●、`Tabs` ●、`Inspector` ●、`StatTile` ○、`ConfirmDialog` ○ | `Button`、`Badge`、`EmptyState`、`ErrorState`、`Skeleton`、`Drawer`、`Modal` | TaskDetailPanel 内容、TaskRow 视觉 |
| Workspaces | `PageShell` ●、`FilterBar` ●、`StatTile` ●、`ConfirmDialog` ○、`FormField` ● | `Button`、`Input`、`Modal`、`EmptyState`、`ErrorState`、`Toast` | 空间卡片 |
| WorkspaceDetail | `PageShell` ●、`Inspector` ●、`ListRow` ●、`Section` ● | `Button`、`EmptyState`、`ErrorState` | 概览区 |
| Workflows | `PageShell(workspace)` ●、`Tabs` ●、`Section` ●、`Inspector` ●、`ConfirmDialog` ●、`FormField` ○ | `Button`、`Badge`、`EmptyState`、`Modal`、`Input`、`Textarea` | 画布、节点库、运行时间线 |
| Agents | `PageShell` ●、`FilterBar` ●、`ListRow` ●、`Inspector` ●、`SearchField` ●、`StatusBadge` ● | `Button`、`Badge`、`EmptyState`、`Modal`、`Input` | 团队信息区 |
| Skills | 同 Agents + `CodeBlock` ● | `Button`、`Modal`、`Textarea`、`Badge`、`ConfirmDialog`、`Toast` | 阅读器布局 |
| Plugins | `PageShell` ●、`FilterBar` ●、`Section` ●、`ListRow` ●、`Inspector` ●、`StatusBadge` ●、`Select` ● | `Button`、`Badge`、`EmptyState`、`ErrorState`、`Modal`、`Input` | MCP 工具/资源列表 |
| Capabilities | `PageShell` ●、`FilterBar` ●、`Tabs` ●、`Inspector` ●、`StatTile` ○、`RiskBadge` ● | `Button`、`Badge`、`EmptyState`、`Skeleton`、`Modal` | 分组矩阵 |
| Memory | `PageShell` ●、`FilterBar` ●、`ListRow` ●、`Inspector` ●、`ConfirmDialog` ●、`Toast` ○ | `Button`、`Badge`、`EmptyState`、`ErrorState`、`Modal`、`Textarea` | 概览气泡 |
| Knowledge | `PageShell` ●、`Section` ●、`StatTile` ○ | `Button`、`Badge` | 说明区 |
| Settings | `PageShell` ●、`Section` ●、`FormField` ●、`Select` ●、`ConfirmDialog` ●、`Toast` ●、`KeyValueRow` ○ | `Button`、`Input`、`Textarea`、`Badge`、`Modal` | 设置吸底栏、分组导航 |
| System | `PageShell` ●、`Section` ●、`StatTile` ●、`ProgressBar` ●、`KeyValueRow` ●、`Tabs` ○ | `Button`、`EmptyState`、`ErrorState`、`Skeleton` | 监控仪表盘布局 |
| Logs | `PageShell` ●、`FilterBar` ●、`Select` ●、`Inspector` ●、`Drawer` ●、`CodeBlock` ●、`ConfirmDialog` ● | `Button`、`Badge`、`EmptyState`、`ErrorState`、`Skeleton` | 日志行/控制台 |
| Account | `PageShell` ●、`Section` ●、`Tabs` ●、`ConfirmDialog` ●、`Toast` ○ | `Button`、`Input`、`Badge`、`EmptyState`、`Skeleton` | 云端契约卡片 |
| Search | `PageShell` ●、`SearchField` ●、`Select` ●、`Section` ●、`ListRow` ○ | `Button`、`Badge`、`EmptyState`、`Input` | 结果卡片 |
| Setup | 不挂 PageShell（保持独立全屏） | `Button`、`Input`、`FormField`、`Select`、`Badge` | 步骤导航、完成页 |
| TaskWorld | `PageShell(workspace)` ●、`Inspector` ●、`StatusBadge` ●、`ConfirmDialog` ● | `Button`、`EmptyState`、`ErrorState`、`Skeleton` | 画布、节点、Proofloop 面板 |
