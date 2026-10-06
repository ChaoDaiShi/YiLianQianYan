# Context Rail Layout Design

日期：2026-10-06

基线：`feat/v1-settings-ui-polish @ 0ba7a7dddb9814b8ddbd88ca8f7f08ed36ba6353`

## 目标

把 Chat 和 Task Canvas 重构为中心内容优先的桌面工作区。右侧辅助内容默认收起为窄轨道，用户显式选择后才展开一个上下文面板。布局参考 ChatGPT 桌面端的可切换侧栏、Side Chat 与完整/分屏视图原则，以及 Notion 的主内容加按需侧栏模式。

官方可确认的参考：

- ChatGPT Learn [Commands](https://learn.chatgpt.com/docs/reference/commands)：桌面端支持 Toggle sidebar、Open side chat 等命令。
- ChatGPT Learn [Browser](https://learn.chatgpt.com/docs/browser)：内置浏览器提供 full view 与 split view 的显式切换。
- ChatGPT Learn [Quickstart](https://learn.chatgpt.com/docs/quickstart)：桌面端以侧栏组织项目、聊天和工作区。

官方文档没有规定本项目所需的右栏宽度或样式。本设计只采用“主内容优先、辅助区域可切换、并列/完整视图显式控制”的原则。

## 范围

本轮修改 Chat 和 Task Canvas 两个工作区。设置页、系统页、能力页、数据库、后端、API、执行状态、审批和 Canvas 几何数据保持。

全局左侧 NavRail 保持。Chat 的会话列表在 full/compact 模式保持原职责；窄屏继续使用左侧 Drawer。其他业务页面不新增右栏。

## 共享 ContextRail

新增纯展示组件 `ContextRail`：

- 宽度 52px，纵向排列 44px 按钮。
- 每项包含 Lucide 图标、Tooltip、`aria-controls`、`aria-expanded` 和可选状态徽标。
- 同时只有一个 item active。
- 不持有业务状态，不读取 API；父页面传入 items 和动作。
- Chat 作为 grid 第三列使用；Canvas 作为绝对定位的 Studio 辅助轨道使用。

轨道和展开面板使用同一组按钮语义。某个断点内不同时显示第二套相同入口。

## Chat

在 full 模式：

- `executionCollapsed` 初始值改为 `true`。
- 默认列为 conversation sidebar + centered chat + 52px ContextRail。
- 轨道显示“执行轨迹”，可显示待审批数量和执行状态点。
- 点击后展开约 360px 停靠式 ExecutionSidebar；面板内收起按钮回到轨道。
- 主对话区不因空白执行面板长期损失宽度。
- Escape 在 full 模式且执行面板展开时将其收起；不影响输入框自身快捷键处理。

compact/narrow 模式继续使用现有右侧 Drawer，由 ChatView 的单一执行入口打开。full 模式不显示 Drawer 入口，因此没有重复功能。

消息列保持居中与有界阅读宽度；超宽窗口只增加外围留白，不把正文无限拉长。

## Task Canvas

Canvas 已有 `activePanel: "trail" | "inspector" | "tools" | null`，继续作为唯一状态源：

- 默认 `null`，右侧只显示 ContextRail。
- 轨道提供节点属性、执行轨迹和更多工具三个入口。
- 展开面板停靠在轨道左侧，三者一次只开一个。
- StudioHeader 移除这三个重复按钮，只保留返回、图身份、保存状态和发布占位。
- 执行轨迹从左侧移到右侧上下文区，统一用户空间模型。
- Escape 关闭当前面板并恢复到触发按钮。
- 普通选中节点只更新 selection/focus，不自动打开 inspector，不改变镜头。
- “从能力添加”完成后可继续显式打开 inspector，因为该操作本身进入配置流程。

轨道位于画布右侧，底部保留现有浮动工具栏安全区。面板宽度保持 360–420px；小窗口面板占用可用宽度但不覆盖整个画布。

## 视觉

采用 Notion 风格的克制结构：无厚重玻璃，无大面积渐变；轨道以实色表面、细边框和统一 18px 图标构成。active 使用淡粉紫底与左侧/内侧强调。状态徽标使用现有 success/warning/danger 语义色。

ContextRail 与现有 NavRail 的图标语言一致，但层级更轻。辅助面板标题固定，内容独立滚动。

## 不变式

- Chat SSE、审批、执行历史和语音状态不变。
- Canvas 选择节点不移动镜头、不自动打开面板。
- Locate/Fit/100%/Auto Layout 仍只由显式操作触发。
- Canvas 节点位置、240×128 几何、保存队列、stale revision 语义不变。
- 后端、Cargo、数据库、REST、Secret、v2、依赖清单不变。

## 验证边界

延续用户上一轮要求：不新增或运行单元测试、集成测试、E2E、视觉自动化。本轮最后只运行一次 `npm run build`。通过源码审计确认默认收起、唯一面板状态、重复入口删除和保护路径无改动。未做运行时尺寸或焦点验收会在报告中明确保留。

