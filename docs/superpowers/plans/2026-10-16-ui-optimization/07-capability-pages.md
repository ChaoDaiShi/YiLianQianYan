# 07 工作包 E：智能体、技能、插件与能力矩阵

**目标：** 四类能力有不同的内容组织方式，同时共享状态、搜索、详情和动作反馈。

**输入：** AgentsPage、SkillsPage、PluginsPage、CapabilitiesPage及现有API返回值。**输出：** 四页GE证据。

**可改：** 02中E文件。拟新增 `frontend/src/styles/ui-optimization/capabilities.css`、`frontend/e2e/ui-optimization/e-capabilities.mjs`。

**并行方式：** E内部四页可先各做布局，但若用户安排多人，必须再明确页面与共享capability组件所有者；未分配前仍由E单一负责人写共享目录。

## E0 四页共同结构

- [ ] 每页使用稳定PageHeader、搜索/过滤工具栏、主内容、详情区，不重复出现两个页面主标题。
- [ ] 加载、无数据、无搜索结果、失败、详情加载失败分别呈现。
- [ ] 卡片正文按字段语义组织，不把大量JSON直接塞进主卡；技术标识进入详情，名称与用途留在列表。
- [ ] 保存、连接测试、刷新都显示操作中/成功/失败，不能混成一个全局loading让整页消失。
- [ ] 图标按钮具备具体名字，例如“配置 demo-mcp”，不能全部只有“编辑”。

## E1 智能体角色卡

现有数据：`AgentDefinition` 包含name、description、instructions、allowed_tools、model、capabilities、enabled、source等；页面已有listAgents、listTeams与createAgent。

- [ ] 保留智能体与团队信息入口，卡片布局适应2–3列；名称过长允许两行。
- [ ] 左上44px角色图标，右侧名称/用途/真实模型（无model写“使用默认配置”）；不新增头像服务。
- [ ] instructions摘要用两行内凹文本区；完整内容通过详情阅读，避免将整段instructions塞进短Tooltip。
- [ ] allowed_tools/capabilities最多显示3项，其余 `+N` 打开已有详情；N必须由数组长度算出。
- [ ] enabled只表示启用，不当成运行健康；source分别展示真实builtin/local_file/database中文说明。
- [ ] 创建表单按“名称与用途”“指令”组织；现有字段之外的模型温度滑块、编辑、复制角色、直接对话本轮不新增。
- [ ] 创建失败保留输入并显示API错误；成功后真实列表出现新ID。

验收E-01：长名称、长instructions、无model、0/3/8项能力；E-02：创建结果与返回ID一致，团队入口没有消失。

## E2 技能手册卡与正文编辑

现有 `SkillSummary` 只有name、description、path、editable；详情有content、root_dir。不能据此虚构触发命令、启用状态或参数schema。

- [ ] 卡片使用技能图标、名称、用途和可编辑/只读标签；路径放在次级信息区。
- [ ] 详情顶部展示真实名称、来源路径、可编辑性三项，不生成假的技能ID。
- [ ] 正文沿用现有loadSkill结果；阅读区行高至少1.55，代码/路径等宽，但普通说明仍可读。
- [ ] 如加入行号，用可选中的textarea/pre与只作装饰的行号栏；行号不进入复制内容，不引入重型编辑器依赖。
- [ ] 编辑只对editable=true的技能开放；readonly详情不显示可点击的保存按钮。
- [ ] 保存沿用validateSkillDraft和updateSkill；错误保留formContent，成功刷新列表与当前详情。
- [ ] 名称/路径复制只复制真实值；不把 `/技能名` 显示为可以执行的指令。

验收E-03：只读与可编辑技能行为不同；E-04：长正文滚动/复制/保存失败后的内容完整；E-05：修改后重新打开显示已保存内容。

## E3 插件卡与连接状态

这是四页中状态最多的一页，先保留所有原入口再调整外观。

- [ ] 区分内置能力、MCP配置、子智能体三类；现有数据来源和页面分区保持可辨认。
- [ ] MCP卡显示name、transport、enabled和runtime_status；不存在版本号时不画 `v1.0`。
- [ ] 用一个有文字标签的switch/button接现有toggleMcpServer；切换过程中锁定当前插件按钮，不让整页插件全部不可用。
- [ ] 健康条由runtime_status/safe_error生成；缺少原因则写“暂不可用，查看配置或测试连接”，不猜测缺API Key。
- [ ] “测试连接”调用testMcpServer；明确区分请求成功与result.ok=false，后者仍为连接失败。
- [ ] tools_count/resources_count/prompts_count缺失时不填0；已有值可直接显示，刷新后以真实返回为准。
- [ ] 详情保持tools/resources/prompts三页签、资源模板参数、资源预览和Prompt预览；折叠卡片不丢配置草稿。
- [ ] 配置弹窗保留stdio与streamable_http两种字段；密钥/环境变量处理沿用现有语义，不将API响应假定为可显示的真实密钥。
- [ ] 切换/保存失败显示在当前卡片或弹窗，不用一个成功Toast盖过错误。

| enabled | runtime_status | 卡片表达 |
|---|---|---|
| false | 任意 | 已禁用；不再显示“已连接”主状态 |
| true | ready | 已就绪；可显示真实工具数 |
| true | misconfigured | 配置有误；展示safe_error（存在时） |
| true | unavailable/degraded | 不可用/能力受限；保留原始区别 |
| true | 缺失或unknown | 状态未确认；提示用户测试/刷新 |

不要把所有非ready都映射成同一句“进程未响应”。不同transport与状态需沿用现有presentation逻辑。

验收E-06：启用不等于连接；E-07：测试返回ok=false仍显示失败；E-08：资源与Prompt预览保持真实动作；E-09：未提交的环境变量不进入截图/日志。

## E4 能力矩阵与详情

当前能力状态列表含ready/unavailable/disabled/misconfigured/degraded/unknown；原稿“五种状态”不完整。

- [ ] 保留KIND_FILTERS、PROVIDER_FILTERS、STATUS_FILTERS完整值；三组过滤可以排成一行，但小窗口必须换行，不裁切选项。
- [ ] 类型保持高频分段或select，来源/状态用现有原生select外观优化；不为下拉菜单引入组件库。
- [ ] 默认矩阵五列：名称/类型/来源/状态/操作；列宽用minmax，名称优先，不用固定百分比压掉长名称。
- [ ] 增加“列表/卡片”视图切换只影响本地呈现；两种视图复用同一filtered数据和selectedId。
- [ ] 状态解释显示API现有说明；未知状态保留“未知/未确认”，不以ready兜成绿色。
- [ ] 点击名称打开详情，保留getCapability、Schema展示和真实管理目标入口；详情失败可单独重试。
- [ ] 上下键只对列表焦点生效，切换当前过滤结果；详情打开不清除过滤条件。
- [ ] 保留ManagedImports和“新增能力来源”流程；“调用测试”会执行真实能力，不在本轮新增此入口。

矩阵CSS参考：

```css
.ui-opt-capabilities .ui-opt-capability-row {
  display: grid;
  grid-template-columns: minmax(220px, 2fr) minmax(90px, .8fr)
    minmax(110px, 1fr) minmax(110px, 1fr) minmax(130px, 1fr);
  align-items: center;
  gap: 12px;
  min-height: 60px;
}
```

在空间不足时切换为卡片/两行结构，不让整页横向滚动。页面根类 `.ui-opt-capabilities` 可同时用于这四个页面，再加 `.ui-opt-agents/.ui-opt-skills/.ui-opt-plugins`限定各页差异。

验收E-10：7类类型、7类来源、7个状态选项含all且可组合搜索；E-11：视图切换后结果与选择一致；E-12：不可用能力进入正确管理入口，未自动执行工具。

## GE验证

```powershell
Set-Location 'F:\项目开发\忆涟千言\YiLianQianYan\frontend'
npm.cmd run test -- src/components/capabilities src/features/capabilities src/pages/agentsCapabilityContract.test.ts src/pages/skillsCapabilityContract.test.ts src/pages/pluginsCapabilityContract.test.ts src/pages/capabilitiesCapabilityContract.test.ts
npm.cmd run build
```

GE由四页分别通过组成；E-01至E-12使用隔离测试配置。真实MCP连接与资源读取可在本地专用测试服务验证，不能把静态卡片的绿色文字算作服务连通。
