# Settings UI Polish Design

日期：2026-10-05  
基线：`feat/v1-typed-data-binding-u31 @ 9f935304e394ccbbd10747fa2666f29167f15983`

## 目标

将设置页整理成统一、舒展、适合长期使用的桌面设置中心。页面借鉴 Windows Settings / Fluent 的导航层级和响应式原则：一套布局随窗口宽度重排；宽屏显示更多并列信息，减少无意义空白；窄屏保持完整可达性。

本轮只修改前端设置页结构和视觉。配置字段、API、SecretStore、保存语义、授权 CRUD、主题能力和路由保持原样。不增加新设置功能，不使用装饰性位图，不修改 Canvas、Chat、Voice 运行时、后端、数据库或 v2。

参考：

- Microsoft Learn — [Layout overview for Windows apps](https://learn.microsoft.com/en-us/windows/apps/design/layout/)
- Microsoft Learn — [Responsive design techniques](https://learn.microsoft.com/windows/apps/design/layout/responsive-design)
- Microsoft Learn — [Screen sizes and breakpoints](https://learn.microsoft.com/windows/apps/design/layout/screen-sizes-and-breakpoints-for-responsive-design)

## 信息架构

一级分类由 8 个收敛为 6 个：

1. 模型服务：模型配置、Embedding、连接状态。
2. 语音能力：STT、TTS、试听与连接状态。
3. 智能体：原智能体配置与对话压缩。
4. 安全控制：原权限与安全、资源授权、沙箱。
5. 能力扩展：技能目录与子智能体目录。
6. 外观：明暗模式、背景和重置。

旧深链接 `?section=permissions|sandbox` 映射至 `security`；`compaction` 映射至 `agent`；`skills` 映射至 `extensions`。已有 `model|voice|agent|appearance` 保持。页面不同时显示旧入口和新入口。

保存设置只出现一次，固定在页面标题栏。全局配置仍统一保存；资源授权的创建/删除继续是显式独立操作，不伪装为全局 dirty 状态。模型区不再渲染第二份保存卡片。

## 页面结构

顶部标题区包含“设置”、当前分组说明、保存状态和唯一的保存按钮。保存按钮在未修改时禁用；保存中、已保存、失败和未保存状态保留。

左侧分类导航采用统一 Lucide 线性图标。每项包含标题和一句说明，最小高度 56px；选中项使用淡粉紫实色底、细边框和图标强调，不使用厚重渐变。导航底部显示简短的本地优先与凭据保护提示，不新增操作。

内容区先显示分组标题卡，再显示设置卡片。卡片使用统一圆角、边框、内边距和层级：

- 标题 20–24px。
- 分组标题 16px。
- 正文与控件 14px。
- 辅助文本不低于 12px，常规说明优先 13px。
- 输入控件最小高度 40px，导航项与主要按钮最小高度 44px。

危险操作（清除密钥、删除授权等）保留明确颜色和确认流程。授权创建区默认折叠，当前授权优先显示，避免长表单长期占据页面。权限模式、隔离状态、资源授权和沙箱在同一安全分组内按卡片分区，不重复入口。

## 宽屏和响应式

- 小于 900px：左侧导航改为顶部横向分类带；内容单列；标题区保存操作换行但不遮挡。
- 900–1599px：220–248px 左侧导航，内容单列，最大宽度约 960px。
- 1600px 及以上：导航约 264px；内容区最大宽度约 1280px；智能体、安全和能力扩展中的独立卡片使用双列网格。
- 超宽窗口不把密钥、路径、系统提示词或复杂表单无限拉宽。这些内容跨双列或限定约 780px 的阅读宽度。
- 1280、1366、1920、2560 宽度均不出现重叠、横向页面溢出、过小字号或不可达控件。

## 视觉语言

继续使用忆涟的淡粉紫品牌色、现有主题变量和 Lucide 图标。图标作为统一美术语言：同一尺寸、线宽和圆角容器，不引入不同风格贴图。页面背景只使用现有实色与极轻的品牌光晕，避免重玻璃、重渐变和插画抢占设置内容。

浅色与深色主题复用语义变量；风险、成功、警告颜色不被装饰色覆盖。交互焦点、hover、disabled、selected 状态保持清楚。

## 组件边界

- `settingsNavigation.ts`：六组导航元数据、图标和旧 deep-link 映射。
- `SettingsSectionFrame.tsx`：分组标题、描述、元信息和卡片网格壳，不持有配置状态。
- `SettingsPage.tsx`：保留现有数据加载、保存、清密钥和连接验证；只调整组合与导航。
- 各 section：只调整卡片层级、文字和类名，不复制数据状态或 API 调用。
- `system.css` / `workbench.css`：响应式设置布局与卡片视觉；删除互相覆盖的旧设置页规则，保留一套权威定义。

## 验证边界

用户明确要求“不做测试”。本轮不新增测试，不运行测试套件，不运行 E2E。完成后只运行一次 `npm run build`，用于确认 TypeScript 与生产打包；这不作为交互验收。最终报告明确记录未执行的测试与人工多尺寸验收。

## 明确不做

- 不使用 Figma 写入或新增外部设计资产；现有 token 和真实组件足以直接落地。
- 不增加设置搜索、收藏、快捷入口或重复跳转。
- 不新增后端字段、API 或保存语义。
- 不改主侧栏、Canvas、Chat、系统中心或插件页面。
- 不运行单元测试、集成测试、E2E 或视觉自动化测试。

