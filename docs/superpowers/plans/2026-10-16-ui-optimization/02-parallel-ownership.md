# 02 并行阶段、依赖与文件所有权

## 1. 人员角色

I是集成负责人，A–G是代码工作线，Q是验收负责人。这是职责名称，不要求创建九个智能体；同一人可以先后承担多项职责。未经用户明确要求不得启动子智能体。

阶段P描述验收时间点，工作包A–G描述可独立写的代码。不能把“先完成所有组件，再做所有页面，再测试”称为并行方案。

## 2. 实际依赖图

```text
                       P0 / G0 固定基线和01契约
                       ├─ A1变量 → A2控件 → A3浮层 → GA
                       ├─ B1布局 → B2消息 → B3输入 ─────┐
                       ├─ C1任务 → C3空间 ─────────────┤
                       ├─ D1工作台 → D2节点 ───────────┤
                       ├─ E1/E2/E3/E4能力四页 ──────────┤ 这些工作同时开展
                       ├─ F1记忆 → F3知识说明 ──────────┤
                       ├─ G1设置 → G3监控 → G4日志 ─────┤
                       └─ Q1状态样例 → Q2测试准备 ──────┘

 GA + 各包局部完成 → 各自公共参数接线 → GB/GC/GD/GE/GF/GG
 任一包过门槛即可进入候选，不需要等待其他包才能检查
 全部必需包 + A4导航 → I候选集成 → Q完整矩阵 → GI → GQ交付
```

例：E可先改插件卡结构并保留旧Button的disabled参数。GA通过后再把保存按钮接入loading参数。不得提前使用一个还没有实现的参数让主干无法编译。

## 3. 阶段操作表

| 阶段/负责人 | 输入 | 具体动作 | 产出 | 退出检查 |
|---|---|---|---|---|
| P0 / I | 当前工作树、00、01 | 核对基线；分文件；核对CSS顺序；登记允许分支；固定接口 | 基线记录、所有权、共享变更单 | 每个文件只一个写入者；每个新参数有默认行为 |
| P0 / Q | 当前可运行产品 | 记录浅深色、空/加载/错误状态获取方式与基线图 | before截图、测试数据说明 | 不以构造成功状态替代运行结果 |
| P1 / A | 01契约 | 按A1–A3实现公共基础 | 可兼容旧调用的组件 | GA：构建与组件交互通过 |
| P1 / B–G | G0、现有组件 | 各自页面布局、样式、局部状态 | 可单独运行的页面增量 | 不修改他人文件、不依赖未提供字段 |
| P2 / B–G | GA及各自布局 | 接入新参数，完成真实动作与状态 | 各包交付记录 | 对应GB–GG用例通过 |
| P3 / I+Q | 工作包增量与证据 | 在单一候选集成，查级联冲突和全局回归 | candidate SHA/差异、总报告 | GI：页面与跨页结果符合10 |
| P4 / I | GI和桌面验证结果 | 清楚标注范围与未测项；按授权准备交付 | 结果索引 | 无未说明的必需失败项 |

## 4. 独占文件表

下表中的目录归属包含其现有测试，但精确文件例外优先于目录规则。未列出的文件默认只读；确需修改走第6节。

| 所有者 | 允许修改的现有文件/目录 | 拟新增的独占文件 |
|---|---|---|
| I | `frontend/src/index.css`；`frontend/src/styles/stylesheet.ts`；`frontend/src/styles/workbench.css`；`frontend/e2e/core-paths.mjs`；必要的路由入口只做接线 | 本轮实施记录与证据索引 |
| A | `frontend/src/components/ui/`；`frontend/src/theme/`；`frontend/src/components/layout/AppShell.tsx`、`NavRail.tsx`、`ContextRail.tsx`、`navGroups.ts`、`RouteLoadingSurface.tsx`；`frontend/src/styles/base.css`、`utilities.css`、`animations.css`、`contextRail.css`；与这些文件对应的layout测试 | `frontend/src/styles/ui-optimization/foundation.css`；`frontend/src/components/layout/useUiNavigationShortcuts.ts` |
| B | `frontend/src/pages/ChatPage.tsx`；`frontend/src/components/chat/`；`frontend/src/features/execution/ExecutionSidebar.tsx`、`ExecutionHistory.tsx`、`CurrentActionCard.tsx`；`frontend/src/components/approval/ApprovalCard.tsx`；`frontend/src/components/layout/workspaceLayout.ts`及其测试 | `frontend/src/styles/ui-optimization/chat.css`；`frontend/e2e/ui-optimization/b-chat.mjs` |
| C | `frontend/src/pages/TaskCenterPage.tsx`、`WorkspacesPage.tsx`、`WorkspaceDetailPage.tsx`；`frontend/src/components/tasks/TaskDetailPanel.tsx`；`frontend/src/features/tasks/`；`frontend/src/features/task-world/TaskWorldPage.tsx`、`StudioHeader.tsx`；仅为UI改动的对应测试 | `frontend/src/styles/ui-optimization/tasks-workspaces.css`；`frontend/e2e/ui-optimization/c-tasks.mjs` |
| D | `frontend/src/pages/WorkflowsPage.tsx`；`frontend/src/components/workflow/`；`frontend/src/pages/workflowEditorContract.test.ts` | `frontend/src/styles/ui-optimization/workflows.css`；`frontend/e2e/ui-optimization/d-workflows.mjs` |
| E | `frontend/src/pages/AgentsPage.tsx`、`SkillsPage.tsx`、`PluginsPage.tsx`、`CapabilitiesPage.tsx`；`frontend/src/components/capabilities/`；`frontend/src/features/capabilities/`；四页对应Contract测试 | `frontend/src/styles/ui-optimization/capabilities.css`；`frontend/e2e/ui-optimization/e-capabilities.mjs` |
| F | `frontend/src/pages/MemoryPage.tsx`、`KnowledgePage.tsx`；`frontend/src/pages/memoryKnowledgeContract.test.ts` | `frontend/src/styles/ui-optimization/memory-knowledge.css`；`frontend/e2e/ui-optimization/f-memory.mjs` |
| G | `frontend/src/pages/SettingsPage.tsx`、`SystemPage.tsx`、`LogsPage.tsx`；`frontend/src/features/settings/`；`frontend/src/features/llm/ModelManagerPanel.tsx`；`frontend/src/features/security/GrantEditor.tsx`；`frontend/src/components/system/SystemMetricCard.tsx`、`SystemSection.tsx`、`SystemStatusBadge.tsx`；这三页对应Contract测试 | `frontend/src/styles/ui-optimization/settings-system.css`；`frontend/e2e/ui-optimization/g-system.mjs` |
| Q | `frontend/e2e/ui-foundation.mjs`；本轮新增总验收模块及报告；所有业务源码只读 | `frontend/e2e/ui-optimization/fixtures.mjs`、`index.mjs`、`shared.mjs`；`target/ui-optimization/`下证据 |

例外：B只读 `components/chat/toolDisplay.ts`、`taskTitle.ts` 及其测试，因为它们被多个页面消费；C/E只读同一文件。需要改展示命名时由I单独指派一个所有者，暂停其他包接线后处理。

A不拥有 `layout/workspaceLayout.ts`；B不拥有全局ContextRail。C不拥有TaskWorld的业务hooks、模型、API、Schema表单或运行引擎。G对GrantEditor只改排版和现有选项说明，不改授权模型。F不改memoryStore和记忆API。

layout测试的精确例外：`navGroups.test.ts`、`shellRailsVisual.test.ts`归A；`workspaceLayout.test.ts`归B；`taskCenterRouting.test.ts`归C。`frontend/src/pages/routeLoadingContract.test.ts`归A。其他页面Contract测试归所在页面负责人；跨多页的 `capabilityFreezeContract.test.ts`归E，`systemCenterFreezeContract.test.ts`归G，出现另一包行为变更时由所有者接收申请再修改。源码检查引用了别的页面，不代表测试文件可以多人同时写。

固定只读：`frontend/src/api/`、`frontend/src/core/api/`、`frontend/src/types/`、后端、SQLite迁移、Tauri权限和配置、语音runtime、`package.json`及锁文件。本计划没有需要更改这些文件的必需任务。

## 5. CSS如何真正并行

1. P0由I在代码实施阶段创建七个独立样式文件的空文件，并在 `index.css` 末尾按 foundation、chat、tasks-workspaces、workflows、capabilities、memory-knowledge、settings-system 顺序登记；这一步不是本次文档任务的操作。
2. I同步 `stylesheet.ts` 的实际导入清单，保留原CSS相对顺序，不大规模搬迁旧声明。登记前后构建，查看生成CSS变化符合新增入口而非旧样式丢失。
3. A–G只写分配到的CSS。页面根节点增加对应范围类，例如B的 `.ui-opt-chat`；每个媒体查询也带范围。
4. 若旧 `workbench.css` 的更高优先级或 `!important` 阻止新样式，提交“旧选择器→目标选择器→截图”给I。I只移除本次已被接管的旧声明。
5. Q用生产构建后的computed style验证最终值。源码里写了36px，不等于实际按钮就是36px。

不把全部工作串到I：I只处理共享入口和明确冲突；页面结构与绝大部分CSS由各包自己完成。

## 6. 共享变更申请

复制以下字段，填写具体值；这是一项工程交接，不是要求用户逐项批准：

```text
申请方：B
需要修改：frontend/src/components/ui/Drawer.tsx
原因：右侧执行详情需要宽度调整，但当前公共参数尚未实现。
消费位置：frontend/src/pages/ChatPage.tsx
使用合同：width / resizable / onWidthChange，遵守01。
最小修改：仅实现已约定参数，不改变onClose或焦点回调。
接线依赖：A3通过后B5接入。
验收：B-02和A-05；1366窗口与200%缩放。
处理所有者：A
```

A确认后实现，B等待该参数这一步，继续做消息/输入等独立任务。禁止B自己顺手修改Drawer。

## 7. 推荐调度

| 同时可工作的执行者数量 | 分配 |
|---|---|
| 1 | 先P0/A公共最小增量；随后按B→C→D→E→F→G执行；仍保留包边界与独立验收 |
| 2 | 一人I+A+Q；另一人按页面包推进，接口完成后接入 |
| 4 | I+A；B+C；D+E；F+G+Q准备。每人顺序处理自己组合，不能在两个会话重复持有一个包 |
| 8或更多 | A–G各一人，I/Q按资源配置；各包P1同起，P2按依赖完成顺序接线 |

这些组合是排期建议，不授权自动创建执行者。若只有一个人，使用同样文档即可逐包实施。

## 8. 每包交付物

- 修改文件与新增文件精确列表。
- 对应任务ID勾选记录和未完成项。
- 真实数据来源、已接入动作、未支持动作。
- 构建命令、退出码、相关行为检查结果。
- 至少浅/深两张页面图，以及本包最关键交互的前后状态证据。
- 交接依赖：是否消费GA新增参数、是否还欠I接线。
- 不自行commit/push/merge；有明确授权才进行，并且只暂存本包文件。
