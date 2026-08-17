# v0.9 UI 对齐修复设计

## 背景

原始「忆涟千言 v0.9 UI优化」对话已经确定了唯一视觉基线：Cyrene Ripple / 昔涟·涟漪、浅色月光背景、低强度阴影、分组式 Sidebar，以及保留现有四区工作台。本次修复针对第一阶段落地后出现的视觉偏差，不扩展到 Phase 2 的中部 Composer、角色主页或 Agent 轨迹重构。

## 修复范围

1. 将 Cyrene 核心 Token 对齐到对话定稿：背景、表面、文字、状态色、圆角和阴影使用统一值；保留已有兼容变量和自定义主题映射。
2. 增加 `sidebar-text`、`divider`、`radius-xs`、`radius-xl` 等定稿别名，同时保留现有 `sidebar-fg`、`border-soft` 等兼容名称。
3. 将背景图片的绘制责任收敛到 AppShell，避免 body 与 AppShell 重复叠加背景图层。
4. Sidebar 在默认宽窗口显示可读的分组式导航：Logo、产品名、连接状态、核心/能力/系统分组和粉紫 Active 状态；窄窗口保留现有紧凑导航，不改变路由或工作台抽屉逻辑。

## 不变项

- 不添加未注册路由，不改现有页面业务布局。
- 不改 Agent loop、SSE 事件语义、API、后端或 SQLite。
- 不加入角色图片、语音、复杂粒子或大范围动画。

## 验收

- 主题映射测试覆盖定稿 Token、兼容别名和不同窗口模式所需的导航数据。
- `npm.cmd test`、`npm.cmd run build`、`git diff --check` 通过。
- Vite 预览路由可访问，且生成 CSS 包含新的 Token/Sidebar 样式。
