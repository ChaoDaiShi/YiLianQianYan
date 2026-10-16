# 06 工作包 D：工作流编排与运行检视

**目标：** 编辑和运行信息有清楚分区，用户能从真实运行步骤找到对应节点。

**输入：** WorkflowsPage、RuntimeWorkflowList、WorkflowGraphEditor、WorkflowRunInspector、WorkflowRecentRuns、workflowEditorModel。

**输出：** 工作流工作台布局、节点状态、显式定位和GD证据。拟新增 `frontend/src/styles/ui-optimization/workflows.css`、`frontend/e2e/ui-optimization/d-workflows.mjs`。

**边界：** 保留模板/runtime两类入口和现有WorkflowGraphDefinition；不将其转换为TaskGraph，不引入新的画布库。

## D1 页面工作区重排

- [ ] 读取WorkflowsPage当前templates/runtime、editorOpen、activeRunId状态，记录每个入口用途。
- [ ] runtime宽窗口组成左工作流列表（目标260–280px）、中间编辑/概览、右侧运行检视；没有选中run时右侧默认收起。
- [ ] 不在无图时自动创建空图；显示“选择工作流”与既有新建动作。
- [ ] 编辑器头部固定名称、保存状态、保存/运行/返回；中间画布占剩余高度，侧栏自身滚动。
- [ ] 工作台可用宽度<1100px时，列表与编辑采用显式切换，运行详情采用Drawer；不把三列硬缩到各200px。
- [ ] templates仍可管理已有模板，不用runtime列表覆盖模板功能。

验收D-01：模板、runtime、新建、编辑、返回路径完整；D-02：1366×768和200%缩放时保存、运行、返回不被裁切。

## D2 节点、连线与编辑状态

- [ ] 继续使用 `layoutWorkflowNodes` 的布局结果，现有节点类型为agent/tool/subagent/condition/output。
- [ ] 给五类节点使用已有presentation语义色，不新增不存在的trigger/input节点类型；色条只是展示。
- [ ] 背景增加16px低对比点阵；减弱深色模式点阵，不能压过节点内容。
- [ ] 节点默认、hover、focus、selected用边框/底色表达；不在hover时缩放节点位置或锚点导致线段漂移。
- [ ] 保留现有点击/键盘选节点和参数编辑；编辑dirty时运行按钮沿用原禁用条件。
- [ ] 保存成功以API返回图为准；保存失败保留节点和表单，不把dirty清空。
- [ ] 放大/缩小/100%/适应视图只修改当前显示坐标系；不把缩放后的位置写进业务定义。若现有画布没有镜头状态，在本包局部增加scale和scroll定位即可。

局部视图建议合同：`scale`初始1，取值0.5–1.5，±按钮每次0.1；100%设1；适应视图根据当前容器与layout.width/height计算，保持上述范围。它只存在于工作流编辑器，不增加数据库字段。

验收D-03：五种节点图标/文字/状态可辨，保存行为保持；D-04：视图缩放不改变导出的WorkflowGraphDefinition。

## D3 运行与画布联动

- [ ] 在父页面使用01定义的selectedNodeId与highlightedNodeId合同；运行面板通过onNodeSelect通知选择，不直接操作编辑器DOM。
- [ ] hover某运行步骤仅设置highlightedNodeId；移出后撤销，不滚动、不重新布局。
- [ ] 点击运行步骤更新选中高亮；用户点击独立“定位节点”才执行onLocateNode并滚动/居中。
- [ ] 点击画布节点时运行检视可展开对应已存在的run节点数据；没有匹配run节点时写“此运行中没有该节点记录”，不复制其他记录。
- [ ] 用真实字段 `run.workflow_graph_id` 对照当前 `graph.id`，再用 `run.nodes[].node_id` 关联节点；workflow_graph_id缺失时只展示运行详情，不启用图联动。编辑后的新节点不显示旧运行成功状态。
- [ ] 展示真实pending/running/waiting_approval/completed/failed/cancelled/skipped；失败步骤的错误来自run记录。
- [ ] 仅真实running节点使用轻微脉冲；上游连接没有执行状态字段时保持静态，不画推测的执行流光。

事件流：

```text
运行步骤 hover → highlightedNodeId → 对应节点边框高亮
运行步骤 click → selectedNodeId → 对应详情展开
定位按钮 click → onLocateNode(id) → 画布显式定位
轮询返回run → 节点状态展示更新 → 不改变用户当前镜头
```

验收D-05：hover与轮询都不移动镜头；D-06：显式定位准确，切换run后不串用前一个图的数据。

## D4 审批、取消与最近运行

- [ ] 保留WorkflowApprovalCard及其真实审批接口；重排视觉，不改决定语义。
- [ ] 取消运行按钮busy期间不可重复触发；状态以getWorkflowRun结果为准。
- [ ] 最近运行显示真实status、created_at和updated_at。run本身没有started_at/finished_at，不把updated_at伪装成完成时间；只有节点的started_at/finished_at同时存在时才显示节点耗时。
- [ ] 加载失败只在运行检视区说明和提供现有重试；编辑草稿保留。
- [ ] 结束的run不持续播放运行脉冲；取消、跳过、失败和完成有不同文字。

验收D-07：待审批与取消链路UI保持；D-08：失败后可读错误，草稿和最近运行不丢失。

## GD验证

```powershell
Set-Location 'F:\项目开发\忆涟千言\YiLianQianYan\frontend'
npm.cmd run test -- src/components/workflow src/pages/workflowEditorContract.test.ts src/api/workflowRuntime.test.ts
npm.cmd run build
```

API测试只是兼容确认，不授权修改API。Q的工作流夹具使用已有可安全执行的输出节点/固定测试工具；真实失败场景与预设UI夹具明确区分。

交付D-01至D-08、宽/窄工作台截图，以及hover不移动和点击定位移动的前后镜头记录。
