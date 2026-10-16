# 05 工作包 C：任务、工作空间与任务画布兼容

**目标：** 用户能从目标/任务列表找到当前状态、执行详情和真实产出，在工作空间内继续工作。

**输入：** TaskCenterPage、TaskDetailPanel、WorkspacesPage、WorkspaceDetailPage、现有TaskGraph入口。**输出：** GC与三个管理页、一个画布兼容结果。

**可改：** 02中C的文件。拟新增 `frontend/src/styles/ui-optimization/tasks-workspaces.css`、`frontend/e2e/ui-optimization/c-tasks.mjs`。

**依赖：** G0后可布局；Drawer/loading新参数接入依赖GA。C内部C1/C2共享TaskDetailPanel，由同一负责人维护，不能再拆给两个同时编辑的会话。

## C1 任务中心分区、筛选和选中

- [ ] 保留顶部目标输入、计划方式、ProofLoop审核与“新建任务画布”，不因重新布置任务列表删去这些能力。
- [ ] 页面分成“目标与计划”“任务画布”“执行记录”可辨认区域；用轻量分隔和标题，不再给每层套厚重卡片。
- [ ] 沿用 `FILTERS` 和 `taskPresentation` 处理会话/任务状态；五类筛选不能覆盖掉waiting_approval、waiting_user、blocked的实际文字。
- [ ] 筛选计数在同一数据集合中计算；任务与会话分开展示来源和计数，不能按相似标题合并成一个任务。
- [ ] 搜索无结果时显示清除筛选；当前无任何任务时才显示创建引导。
- [ ] 行左侧使用3px状态线，标题正文优先；状态、空间、时间排列稳定。等待审批用文字标记并保留入口。
- [ ] 选择记录后宽容器显示主从双栏，窄容器使用Drawer；关闭详情不清空筛选或滚动位置。
- [ ] 上下键只在列表获得焦点时切换当前过滤结果，首页/末页不绕回；不能在输入框中移动任务选择。

筛选计数示意，只消费当前加载记录，不能作为后端总数：

```ts
const visibleCount = filteredTasks.length;
const loadedCount = tasks?.length ?? null;
const countLabel = loadedCount === null
  ? "正在加载任务"
  : `当前显示 ${visibleCount} / 已加载 ${loadedCount}`;
```

其中filteredTasks使用页面已有过滤结果命名接入，不重新定义另一套过滤算法。

验收C-01：全部原任务入口仍可到达；C-02：筛选、搜索、来源与计数相符；C-03：主从切换不丢选择。

## C2 TaskDetailPanel 信息重排

- [ ] 保留 `task/workspaceName/onOpenWorkspace/onChanged` 接口；由当前task.id决定详情，不建立第二份“当前任务”。
- [ ] 顶部固定任务名、状态和已有启动/重试/取消操作；按钮根据现有isTaskStartable等条件显示。
- [ ] 组织三个页签：“执行过程”“输入与产出”“事件详情”。执行过程保留executions/timeline/decisions；输入与产出保留ResourceBindingPanel、ArtifactResultsPanel与现有文件操作；事件详情展示原事件。
- [ ] 记忆转技能候选继续可到达，可放在输入与产出底部，不能被页签重排隐藏。
- [ ] 等待审批/等待输入时，让对应决定区域保持明显；不要只藏在用户未打开的末尾页签。
- [ ] `retryTask` 按当前语义写“重试任务”。除非当前调用明确使用检查点恢复接口，否则不写“从断点继续”。
- [ ] 只有具有明确总步骤与完成步骤时才显示“已完成x/y步”；不能按耗时填充百分比进度条。
- [ ] 切换task.id时详情的加载/错误区只影响详情；上一任务产出不短暂显示在新标题下面。通过当前ID关联展示，不改数据层协议。
- [ ] 复制产出路径复制完整路径；只有真实已有open能力才显示“打开”，不得用打开成功Toast代替实际动作。

| 任务状态 | 展示要求 | 操作规则 |
|---|---|---|
| draft/ready | 显示草稿/就绪 | 原启动条件成立才启用 |
| running | 执行中及已有当前步骤 | 原取消入口可用 |
| waiting_approval | 等待批准和决定入口 | 只接现有决定流程 |
| waiting_user | 明确需要输入 | 展示真实输入请求 |
| blocked | 阻塞原因 | 不显示旋转成“仍在执行” |
| completed | 完成、真实产出 | 不从“有文件”反推验证通过 |
| failed/cancelled | 错误或取消原因 | 允许重试与否跟随现有逻辑 |

验收C-04：三页签与原入口全部保留；C-05：重试/取消只调用一次，返回状态刷新；C-06：任务A/B快速切换不串产出。

## C3 工作空间卡片和创建表单

- [ ] 卡片grid使用01的自适应规则；300px只是目标最小值，窄窗口允许单列不横向溢出。
- [ ] 顶部使用40px文件夹图标、名称、真实更新时间。卡片用min-height，长名称不盖住操作。
- [ ] 中部真实root_path放入路径区，左图标，文字允许中间省略，旁边显式复制按钮。
- [ ] 无root_path时写“未绑定本地目录”，不显示假路径；active_tasks未返回时不写0。
- [ ] 只显示真实active_tasks，不摆放子代理/知识库/会话假计数。
- [ ] 保留现有名称和描述创建字段；本轮可在同一个表单接入 `createWorkspace` 已有可选root_path，不做三步挂载向导。
- [ ] 创建中按钮busy，失败保留表单，成功后调用现有列表刷新并关闭弹窗；不能先画一张“成功卡片”再补请求。

路径显示合同：数据使用 `workspace.root_path` 原字符串；UI省略仅发生在呈现层。复制反馈1.5秒后回默认，失败有可读提示。长路径完整内容可从Tooltip或详情选择复制。

验收C-07：有/无路径、长中文名称、0任务/未返回任务数；C-08：创建失败和成功表单状态；C-09：复制内容等于API原路径。

## C4 工作空间详情

- [ ] 顶部保留返回入口，展示真实名称、描述、路径与active_tasks（存在时）。概览不要堆满三张无数据统计卡。
- [ ] 保留“新建任务”主动作和当前空间任务列表；任务创建继续带当前workspace_id。
- [ ] 中间任务列表与C1使用同一种状态/日期展示，详情复用C2，不能复制TaskDetailPanel内部实现。
- [ ] 不新增Knowledge/Subagents设置页签：当前Workspace DTO没有这些绑定结构。
- [ ] 不增加“在此空间发起对话”假入口；仅导航到 `/chat?workspace=...` 不构成上下文绑定。如果以后有真实消费链路，另开接线任务。
- [ ] 原有可编辑/删除动作若页面已有则保留；本轮不专门新增删除流程、空间名称二次输入或新的权限层。

验收C-10：从空间列表进入详情后创建任务，列表显示同一workspace_id的真实任务；返回保持导航正常。

## C5 任务画布兼容和目标视图可读性

这是全局UI优化的配套检查，不重写TaskWorld。

- [ ] 确认任务中心仍能进入 `/task-world/:graphId`，目标/专家视图与既有计划/验收/修复入口可到达。
- [ ] 在TaskWorldPage/StudioHeader可改范围内改善标题和按钮间距，长图名不挤出保存、返回与视图切换。
- [ ] 新样式仅加父级范围；不改节点几何、存储位置、Schema绑定、save队列或业务hooks。
- [ ] ContextRail仍默认收起，一次一个面板；普通选节点不自动定位、不自动开面板。
- [ ] 宽窗口增加画布可用面积；窄窗口保证工具条、节点操作和面板关闭按钮可用。
- [ ] 对任务画布之外的资源/ProofLoop组件仅做回归，不顺手改算法、状态或协议。

验收C-11：选择节点不移动镜头，定位按钮才移动；C-12：图的保存与目标/专家切换保持原行为。

## GC验证与交付

```powershell
Set-Location 'F:\项目开发\忆涟千言\YiLianQianYan\frontend'
npm.cmd run test -- src/pages/TaskCenterPage.test.ts src/features/tasks src/components/layout/taskCenterRouting.test.ts src/features/task-world/taskCorePaths.test.tsx
npm.cmd run build
```

Q复用现有画布稳定性/核心路径脚本，结合C-01至C-12。工作空间路径和产出用独立测试目录，不打开或覆盖用户当前项目文件。

完成记录需区分：任务UI通过、工作空间UI通过、任务画布兼容通过。某一项失败不能用其余两项替代。
