# Context Rail Layout Report

日期：2026-10-06

分支：`feat/v1-context-rail-layout`

基线：`feat/v1-settings-ui-polish @ 0ba7a7dddb9814b8ddbd88ca8f7f08ed36ba6353`

## 结果

Chat 与 Task Canvas 已改成中心内容优先的桌面工作区。右侧辅助内容默认收起为 52px 的统一 ContextRail，用户显式点击后才展开一个上下文面板。共享轨道只负责图标、Tooltip、状态点、徽标和展开语义，业务状态继续由各页面维护。

本轮没有引入贴图、重玻璃、重渐变或第二套入口。图标统一使用现有 Lucide 体系，轨道按钮为 40px，常规文字继续沿用现有字体 token。

## Chat 布局

- full 模式默认显示居中的对话区与收起的执行轨道。
- 执行轨道显示当前连接/执行状态；当前会话存在待审批项时显示数量徽标。
- 点击轨道显式展开 320–360px 的执行面板；面板内收起按钮和受控 Escape 返回轨道。
- compact/narrow 模式继续使用原右侧 Drawer，没有新增重复的执行入口。
- 消息正文最大阅读宽度从 860px 调整到 900px，超宽桌面增加留白，不把正文无限拉长。

## Task Canvas 布局

- `activePanel` 仍是唯一面板状态，初始值保持 `null`。
- 节点属性、执行轨迹和画布工具集中到右侧 ContextRail；StudioHeader 删除三组重复按钮。
- 执行轨迹从左侧移到统一的右侧上下文区。轨迹、属性和工具面板分别使用 320px、360px、420px 宽度，并停靠在轨道左侧。
- 轨道可显示已执行节点数量；节点属性入口使用选中节点的执行状态色。
- 720px 以下将轨道缩为 48px，并为面板、通知和底部画布工具保留独立安全区。

普通节点选择仍只调用 `setFocusedNodeId`，不会打开 Inspector，也不会移动镜头。Locate、Fit All、100% 和 Auto Layout 仍由既有显式按钮触发。“从能力添加”后打开 Inspector 的原配置流程保留。

## 主要变更文件

- `frontend/src/components/layout/ContextRail.tsx`：共享无状态右侧上下文轨道。
- `frontend/src/styles/contextRail.css`：轨道尺寸、状态、焦点和窄屏规则。
- `frontend/src/components/ui/Tooltip.tsx`：增加左右定位能力，右侧轨道使用左向 Tooltip。
- `frontend/src/pages/ChatPage.tsx`：执行上下文默认收起及显式展开逻辑。
- `frontend/src/features/execution/ExecutionSidebar.tsx`：只保留展开面板职责。
- `frontend/src/styles/workspace.css`、`frontend/src/styles/shell.css`：中心内容与执行列布局收敛。
- `frontend/src/features/task-world/TaskWorldPage.tsx`：画布右侧轨道及单一面板映射。
- `frontend/src/features/task-world/StudioHeader.tsx`：移除重复面板入口。
- `frontend/src/features/task-world/canvas/studio.css`：右侧停靠层级和响应式安全区。

## 范围与安全

源码审计确认 Backend、Cargo、数据库、schema、migration、REST、Secret、v2、依赖清单和包锁文件改动均为 0。Canvas 节点几何、视口恢复、保存队列、stale view revision、SSE、审批与执行协议未修改。

## 验证

根据用户明确要求，本轮没有新增或运行单元测试、集成测试、E2E、视觉自动化或人工多尺寸验收。

只运行一次前端生产构建：

```text
npm.cmd run build
exit 0
tsc && vite build
✓ 2157 modules transformed
✓ built in 7.88s
```

构建保留既有主 chunk `index-CCi9KLFG.js` 为 523.01kB、超过 500kB 的提示。本轮没有新增依赖或进行无关拆包。构建日志和源码审计证据保存在 ignored `target/context-rail-layout/`。

## 明确保留的限制

- 未运行测试，因此不声明任何测试套件通过。
- 未进行浏览器或真实桌面 1280/1366/1920/2560 截图验收；响应式结论来自 CSS 规则与生产构建，不表述为实机视觉验收。
- 构建完成后只把 Inspector 轨道按钮的可访问名称从动态节点标题收敛为固定“节点属性”，以保持稳定语义；为遵守只执行一次构建的约束，没有为这一处纯文案调整再次构建。
- 现有 `canvas-studio.mjs` 仍按旧名称“更多”寻找画布工具入口；本轮遵守“不做测试”要求，未修改或执行该 E2E，恢复测试门禁时需把选择器同步为“画布工具”。
- 主 bundle 的既有体积提示仍存在。
- 本分支保留本地，未推送、未合并、未发布。
