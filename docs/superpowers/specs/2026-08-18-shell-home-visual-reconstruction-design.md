# Phase R1A — Shell + Home Visual Reconstruction Design

## Scope

本设计用于 YiLianQianYan v0.9 Phase R1A 的 Shell/Home 视觉重构。目标是在真实 Tauri 窗口中建立以 Workbench 为视觉主体的桌面应用构图，同时保留现有业务行为、数据来源和安全边界。

本轮允许修改的视觉区域：

- AppShell
- NavRail
- ConversationSidebar
- WorkbenchHome
- ChatHeader 的视觉承载
- ExecutionSidebar 的视觉布局
- SkillsPage 的展示布局（本轮用户明确授权恢复左右分栏）

本轮不修改 Backend、API、SSE、Store 业务模型、Agent Runtime、Memory、Workflow、MCP、Security、Approval、Database，也不进入 R1B、Phase 5C 或其他 Feature Page 的重新设计。

## Design Direction

采用 60% 现代桌面生产力、40% Cyrene 角色氛围的首页构图；系统和能力类页面保持克制。页面层级使用 App Background → Region Surface → Interactive Surface → Hover/Selected 四级体系，减少重复白卡、明显边框和大面积阴影。

首页中心轴固定为：

```text
Greeting
  ↓
Character
  ↓
Status
  ↓
Composer
  ↓
Quick Actions
```

角色不重新生成。当前可用 `frontend/public/favicon.png` 为 256×256、完全不透明的白底 PNG；本轮通过更大显示尺寸、柔和 ambient 背景和轻量混合处理降低白底突兀感，并在交付报告中保留 Asset Gap 说明。

## Shell Layout

在 full workspace 中采用以下视觉基准：

- NavRail：约 76–80px，保持 icon above label 的紧凑 Dock 结构。
- ConversationSidebar：约 248–260px，作为 Context Rail，降低饱和粉色和卡片权重。
- Workbench：`minmax(0, 1fr)`，始终是最大的区域和视觉中心。
- ExecutionSidebar：expanded 约 288–300px，collapsed 约 48px；Idle 状态使用低对比辅助区域。

分栏使用背景色阶、间距和极低对比边界表达，不使用明显的“灰线墙”。低宽度下沿用既有 workspace mode 和 drawer 行为，只调整视觉尺寸和表面样式。

## NavRail

NavRail 保留所有现有导航项、路由和 backend health polling。视觉调整包括：

- 顶部只保留 Q 版角色 Logo，不显示大型品牌文字。
- 导航项保持 icon above label，缩短垂直占用并避免标签被不必要地挤压。
- Active 使用浅粉紫半透明面、提高图标和文字亮度，并保留极小星光标记。
- 分组只使用间距和 subtle divider，不重新渲染“核心/能力/系统”标题。
- 底部只显示绿色/黄色/红色状态点，状态文字继续通过 Tooltip 提供。
- 不使用强 Glow、大阴影或大边框。

## ConversationSidebar

ConversationSidebar 保留新建、搜索、选择、删除、空态和轮询行为，但改为无滚动条的紧凑 Context Rail：

- 外层和任务列表不显示滚动条；常规首页内容应在视口内完成布局。
- 列表使用紧凑 row，标题优先，命令/细节作为次级信息。
- Selected 使用柔和 accent surface，不使用重边框。
- 新建任务按钮改为中性浅色按钮，粉色只保留在图标或微小 accent。
- 仅在任务数量超出可用高度时允许内容裁切或由页面级行为处理，不引入可见滚动轨道。

## WorkbenchHome and Composer

WorkbenchHome 使用稳定的中心列和不等距垂直节奏：

- Greeting 是首页 Header，但视觉权重低于角色。
- Character 以首页 Hero 视觉尺寸展示，保持在 Composer 之上并避免遮挡。
- Status pill 更轻、更紧凑，保留实时状态语义。
- Composer 保持视觉中部，宽度约 640–700px，不移动到底部。
- Composer 采用月光白 command surface、极轻边界和 focus ring，不增加不存在的 ReAct/Context/Model 控件。
- Quick Actions 保留四项真实动作，改为统一的紧凑 2×2 网格；标题 baseline 对齐，描述控制在 1–2 行，hover 不缩放。
- `Enter`、`Shift+Enter`、`Send`、`Stop` 的行为完全不变。

低高度优先压缩垂直间距，其次调整角色尺寸，最后才调整文字；不以整体缩小替代布局重构。

## ExecutionSidebar

保留当前动作、历史记录、审批按钮、折叠和关闭行为。Idle 时：

- 区域背景与主背景接近。
- Header 保留明确标题和统计，但正文使用低对比度。
- 不形成大 Empty Card 或明显的固定面板墙。
- Expanded/collapsed 视觉尺寸与 Shell grid 对齐。

## SkillsPage

本轮仅对 SkillsPage 恢复左右分栏视觉，不改变真实能力：

- 左侧为固定宽度的技能列表/搜索 Context Rail。
- 右侧为可滚动的技能详情和真实 `SKILL.md` 内容。
- 保留 `listSkills`、`loadSkill`、加载态、错误态、空态、搜索、选择和重试行为。
- 左侧列表滚动条保持隐藏或不显示轨道，详情内容允许独立滚动。
- 不新增“启用/版本/安装/测试技能”等未经 API 支持的元数据或操作。

## Files and Boundaries

预期仅修改：

- `frontend/src/components/layout/AppShell.tsx`
- `frontend/src/components/layout/NavRail.tsx`
- `frontend/src/pages/ChatPage.tsx`
- `frontend/src/components/chat/ConversationSidebar.tsx`
- `frontend/src/components/chat/WorkbenchHome.tsx`
- `frontend/src/components/chat/ChatInput.tsx`（仅视觉类名或布局承载，如有必要）
- `frontend/src/features/execution/ExecutionSidebar.tsx`
- `frontend/src/pages/SkillsPage.tsx`
- `frontend/src/index.css`
- 必要的视觉契约测试文件

不修改 API、Store、数据类型、运行时、后端和数据库文件。

## Validation

每轮视觉修改都必须：

1. 启动真实 Tauri。
2. 在 1366×768 和 1200×800 实际抓图。
3. 检查第一视觉焦点、Workbench 主体性、角色存在感、Composer 可发现性、左右侧栏抢戏程度、Card/Border/Pink 密度和 Desktop App 感。
4. 至少完成 Round 1 与 Round 2 截图复核；若仍明显接近 Generic Admin Dashboard，则继续 Round 3。

最终至少运行：

```text
npm.cmd test -- --run
npm.cmd run build
git diff --check
```

后端保持不变，但保留基线验证记录。最终报告必须明确 UI Behavior Changes、Backend Changes、API Changes、Store Semantic Changes 和 Security Changes 均为 NONE，除非验证发现实际回归。
