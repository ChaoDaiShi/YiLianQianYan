# 连接器与交互整合开发文档

> 最新实现与验收以[第7轮结果](revision-7-results.md)及[第7轮方案](revision-7-plan.md)为准：画布仅显示输入输出、Git默认收起且显式选择工作空间、设置节点展开居中、Skill工具适配与独立工具列表。早期图稿保留需求历史。

**目标：** 完成 PID 中的 11 项需求及后续修订，包括更多导航页、节点编辑、显示定制、紧凑画布及多画布删除。

**架构：** 保持 React / TypeScript / Zustand、Tauri、Rust / Axum / Tokio、SQLite。连接器使用仓库已有 `@xyflow/react`；复用现有语音会话、安全执行网关、密钥存储和用量记录。新增界面通过现有模块接口接线。

**执行方式：** 2026-10-21 用户授权开始实现并允许子智能体；按文件所有权并行实施，主执行者统一集成和验收。导航网格按从左向右、从上向下顺序排列，尾行从第一列开始。

本文保留开发拆解和原计划文件名。实际新增文件、集成修正与验证状态见 [实现报告](implementation-report.md)；下列未勾选步骤不用于推定实现缺失或验收通过。

## 1. 设计阶段代码基线

2026-10-20，本地 `develop`，HEAD `4399a43`，开始撰写时工作树无修改。下面只记录本次变化需要触及的接口，不作为旧功能需求清单。

| 当前文件 | 实际依据 | 本版动作 |
|---|---|---|
| `frontend/src/components/layout/NavRail.tsx`、`frontend/src/components/layout/AppShell.tsx` | 当前桌面窄栏、主/次导航、更多弹层、底部安全/设置/状态 | 保持窄栏，将更多弹层改为独立导航页 |
| `frontend/src/components/layout/PageShell.tsx`、`frontend/src/styles/workspace.css` | 页标题与说明同排，花瓣背景和半透明容器 | 预览与实际开发使用相同外壳约定 |
| `frontend/src/components/layout/navRegistry.ts` | 任务、搜索、工作流为一级项；Skill/插件有独立项 | 替换导航和快捷键注册 |
| `frontend/src/surfaces/workspace/WorkspaceSurface.tsx` | `/tasks`、`/task-world/:graphId`、`/search`、`/workflows` 等路由 | 添加连接器；移除指定页面路由，能力内容改弹窗承载 |
| `frontend/src/features/task-world/canvas/TaskWorldCanvas.tsx` | 已使用 React Flow | 复用库及通用画布交互；不复用 TaskGraph 业务语义 |
| `frontend/src/features/voice/ChatVoiceInput.tsx` | 两个入口有常驻文字 | 改图标入口，区分听写与呼叫 |
| `frontend/src/features/voice/GlobalVoiceHost.tsx`、`runtime/useGlobalVoiceRuntime.ts` | 统一会话、lease、generation、续答和播放控制 | 补齐连接阶段、自动轮替和独立窗口呈现 |
| `frontend/src/features/voice/GlobalVoiceLeaf.tsx` | 当前为页面内 pill/panel | 改为通话浮窗内容与圆形收起态 |
| `frontend/src/features/settings/model/settingsNavigation.ts` | 模型、语音分组 | 合并为基础配置 |
| `frontend/src/features/llm/modelPresets.ts`、`llmModelUtils.ts` | 两处预设集合不一致，分别涉及 OpenAI/DeepSeek 和 Qwen/GLM | 归并为后端可消费的统一预设目录 |
| `backend/src/modules/voice/provider.rs` | 已有 OpenAI-compatible 和 MiniMax 适配 | 逐服务注册预设与真实 adapter |
| `backend/src/safety/policy_engine.rs` | 已有 Allow / RequireApproval / Deny，高风险要求审批 | 明确持续完全授权和规则优先级 |
| `frontend/src/features/settings/sections/PermissionSettings.tsx` | yolo/ask/plan 选择，与高风险说明并存 | 统一为三态权限 UI 和后端行为 |
| `backend/src/safety/grant/model.rs` | 已有主体与资源授权 | 扩展现有存储表达需批准，不另造网关 |
| `backend/src/api/agents.rs` | agent_view 返回 instructions | 分离用户提示词和项目模板 |
| `frontend/src/api/system.ts`、`frontend/src/types/index.ts` | `/api/llm/usage` 及 LlmUsageReport 已有 | 监控接入并扩展未知用量与分组 |
| `backend/src/integrations/llm/usage.rs` | 已记录厂商 usage | 检查完整请求覆盖和记录去重 |
| `frontend/src/theme/applyTheme.ts` | 当前变量生成使用语义 token，未应用传入 theme 的透明度 | 连接外观配置与实际表面变量 |
| `src-tauri/src/main.rs`、`src-tauri/tauri.conf.json` | 桌面窗口初始化入口 | 增加通话和监控浮窗 |

现有语音、权限和 usage 的存在只说明可复用的接入点，不等于本版行为已经完成。

## 2. 公共开发约束

- 2026-10-21 已进入实现；开发前检查工作树和 AGENTS。当前工作分支为 `codex/connector-workbench`，保护其他未提交改动。
- 新开发按仓库分支规则从 develop 建功能分支，不在 main 开发；用户另有明确分支要求时服从其要求。
- 按用户此次明确要求，实现模型池内的顺序自动降级与切换提示；不扩展到其他模块的自动替换、兼容层、隐藏修复或额外代理。
- 不把“真实验收”写成无法完成的免责声明；每个任务给出可执行的验证步骤。
- 页面入口删除不自动清理历史数据；用户从“管理画布”明确删除某图时，执行本版约定的整图删除链路。
- 保持当前桌面 80px/68px 窄导航栏、上下排布图标标签和底部安全/设置/状态灯；更多改为导航页，不保留浮动菜单。
- 复用已有 secret store，不在新的公开 DTO、日志、截图中写密钥。
- 保持现有取消、SSE 和执行网关接线。已有安全基础与本次三态控制统一实现。
- 新路径均明确列在各任务“新增”中；既有路径在执行前再次检查，不将规划路径当成已有实现。

## 3. D01：主题与弹窗基础

**输入：** PID R05/R06/R11、契约第 9 节。<br>
**输出：** 统一管理弹窗、阅读弹窗、图标操作组件；透明度和模糊度实际生效。

修改：

- `frontend/src/theme/applyTheme.ts`
- `frontend/src/theme/presets.ts`
- `frontend/src/theme/types.ts`
- `frontend/src/features/settings/sections/AppearanceSettings.tsx`
- 现有 `frontend/src/components/ui/` 弹窗和按钮组件，以实际组件名为准。

拟新增：

- `frontend/src/components/ui/ManagementDialog.tsx`
- `frontend/src/components/ui/TextDetailDialog.tsx`
- `frontend/src/styles/management-dialog.css`

步骤：

- [ ] 在现有主题生成处接入表面 alpha 与模糊变量，区分页面面板与阅读/配置层。
- [ ] 管理弹窗采用 header / category navigation / scroll body / footer 四区，正文 `min-height:0; overflow:auto`。
- [ ] 实现关闭后焦点归位、Esc 关闭最上层弹窗、背景滚动锁定；长文本使用专属阅读弹窗。
- [ ] 以 50 条列表和 5000 字详情验证独立滚动，页头与保存按钮保持可见。
- [ ] 验证浅色、深色、背景图、35%/70%/100% 面板透明度。

验收：A01～A04、T01～T04。页面外壳不能出现整页滚动。

## 4. D02：导航替换与连接器

**依赖：** D01。<br>
**输入：** ConnectorView/ConnectorBinding；已有对话、智能体、能力、模型、记忆/知识引用。<br>
**输出：** 可保存布局、可增删实际绑定的 `/connectors`。

同时交付 `/more` 卡片导航页、节点编辑、每画布显示配置和完整画布删除。

修改：

- `frontend/src/components/layout/navRegistry.ts`
- `frontend/src/components/layout/NavRail.tsx`
- `frontend/src/components/system/productPreferences.ts`
- `frontend/src/components/layout/useUiNavigationShortcuts.ts`
- `frontend/src/surfaces/workspace/WorkspaceSurface.tsx`
- `frontend/src/components/chat/currentTaskEntry.ts`
- `frontend/src/components/layout/ContextRail.tsx`
- `frontend/src/pages/SetupWizardPage.tsx`
- `backend/src/app/router.rs`
- `backend/src/db/migrations.rs`
- `frontend/src/api/taskWorld/calls.ts`
- `backend/src/modules/task/api/graph_routes.rs`
- `backend/src/modules/task/runtime.rs`
- `backend/src/db/task_world.rs`
- 运行时对话选取智能体与资源的位置，沿当前 chat/engine 调用链定位。

拟新增：

- `frontend/src/features/connectors/ConnectorsPage.tsx`
- `frontend/src/features/connectors/ConnectorCanvas.tsx`
- `frontend/src/features/connectors/ConnectorNode.tsx`
- `frontend/src/features/connectors/connectorProjection.ts`
- `frontend/src/pages/MorePage.tsx`
- `frontend/src/features/connectors/ConnectorNodeEditor.tsx`
- `frontend/src/features/connectors/ConnectorDisplayDialog.tsx`
- `frontend/src/features/connectors/CanvasManagerDialog.tsx`
- `frontend/src/api/connectors.ts`
- `backend/src/api/connectors.rs`
- `backend/src/db/connectors.rs`

步骤：

- [ ] 扫描 `/tasks`、`/task-world`、`/search`、`/workflows`、`/skills`、`/plugins` 的导航消费点，形成需改入口列表。
- [ ] 删除指定一级项、快捷键和显式路由；将任务状态摘要留在其所属对话，移除“前往任务中心”等按钮。
- [ ] 在现有导航偏好中将任务入口替换为连接器，删除已移除项；保留用户其他可见性/顺序设置。更多导航到 `/more`，以卡片承载智能体、记忆、知识库、监控等，并提供二级页面返回入口。
- [ ] 更多和能力的标题、卡片网格在内容区水平居中；每行从左向右排列，未满的末行从第一列开始。插件新增功能根据 features 动态生成更多入口，底部窄栏位置不变。
- [ ] 为能力的工作流/Skill/插件弹窗接入点保留调用组件；从侧栏和显式整页路由中移除它们。
- [ ] 建立节点投影：左右上下四方向、单中心智能体、不创建 TaskGraph。
- [ ] 接入布局 API 与绑定 API；连线成功后由后端返回实际关系。
- [ ] 在对话与模型/工具/上下文选择处读取绑定。绑定改变后通过实际回复或工具调用确认效果。
- [ ] 实现智能体切换、定位、适应视图、25%～200% 缩放与布局恢复。
- [ ] 建立独立 canvas_id，支持同一智能体多张画布；每张分别持久化显示范围、隐藏节点、布局、视口。
- [ ] 每类节点点击/键盘激活打开对应模块编辑弹窗，保存回写摘要，取消不改值，拖动不误触编辑。
- [ ] 加入全部/仅对话/仅能力/自定义；显示筛选只改变投影，隐藏节点连线一起隐藏，不更改业务绑定。
- [ ] 将初始布局改为边缘间距 48～64px、同列 16～24px，支持密集/舒展与适应可见节点。
- [ ] 管理弹窗列出所有连接器画布和历史 TaskGraph；删除请求显式携带该行的类型、ID、版本，不能闭包读取当前活动图。
- [ ] 实现新增连接器 DELETE 和历史 TaskGraph DELETE；后者需要同步事务数据、runtime 缓存与其他打开视图，处理运行中取消后删除。
- [ ] 建立 A/B/C 三张画布，依次覆盖删除非当前 B、当前 A、最后 C；再重启后核对列表/GET；另测批量部分失败、取消失败、版本冲突及输入框 Delete 键。

注意：D04 完成后接入安全约束节点；该节点编辑的是现有安全规则的统一新界面。

验收：N01～N06、C01～C12、D01～D08，插件入口与 D05 联合验收 X01～X03。布局、显示配置、业务绑定与整图删除分别验证。

## 5. D03：统一基础配置与厂商预设

**依赖：** D01；与 D02 在单执行者流程中顺序推进。<br>
**输入：** ProviderAccount、ServiceProfile、ProviderPreset、ModelPool 与 [模型池细则](model-pools.md)。<br>
**输出：** 多厂商账户、七类服务配置、分级模型池、自动降级、切换提示、真实连接测试。

修改：

- `frontend/src/pages/SettingsPage.tsx`
- `frontend/src/features/settings/model/settingsNavigation.ts`
- `frontend/src/features/settings/sections/ModelSettings.tsx`
- `frontend/src/features/settings/sections/VoiceSettings.tsx`
- `frontend/src/features/llm/modelPresets.ts`
- `frontend/src/features/llm/llmModelUtils.ts`
- `backend/src/modules/settings/application/service.rs`
- `backend/src/modules/settings/application/provider_test.rs`
- `backend/src/modules/settings/application/secret_lifecycle.rs`
- `backend/src/api/search.rs`
- `backend/src/modules/voice/provider.rs`
- `backend/src/integrations/llm/client.rs`

拟新增：

- `frontend/src/features/settings/sections/BasicConfiguration.tsx`
- `frontend/src/features/settings/components/ServiceProfileDialog.tsx`
- `frontend/src/api/serviceProfiles.ts`
- `backend/src/modules/settings/domain/provider_catalog.rs`
- `backend/src/modules/settings/application/service_profiles.rs`
- `backend/src/api/service_profiles.rs`
- `frontend/src/features/settings/components/ProviderAccountDialog.tsx`
- `frontend/src/features/settings/components/ModelPoolPanel.tsx`
- `frontend/src/features/settings/components/ModelPoolDialog.tsx`
- `frontend/src/features/chat/ModelRouteNotice.tsx`
- `backend/src/modules/settings/domain/model_pool.rs`
- `backend/src/integrations/llm/model_pool.rs`
- `backend/src/api/model_pools.rs`

步骤：

- [ ] 统一两处厂商预设来源，建立按服务和区域声明的目录。
- [ ] 将已有模型、语音和搜索配置写入统一档案，复用凭据引用；在迁移测试中核对前后生效值一致。
- [ ] 接入七类配置卡和弹窗，移除主页面 details/accordion 长表单。
- [ ] 为每个正式声明支持的服务实现 adapter、表单、测试请求和结果呈现。
- [ ] 保存与测试使用 profile revision，配置修改后不能保留旧“连接成功”标记。
- [ ] 基础配置、初始化向导、连接器资源选择均读取同一目录。
- [ ] 每个发布预设使用官方账号或用户测试账号完成对应服务请求，记录日期和结果。
- [ ] 多厂商账户并存，一个账户添加多个模型档案；池只引用档案，不复制凭据。
- [ ] 实现层级、同层候选排序、增删、模型用途校验和池版本，连接器支持绑定池或固定档案。
- [ ] 在单次 LLM 调用边界实现顺序调度、超时/限流等原因分类、每候选一次和总时限；用户取消立即停止。
- [ ] 处理部分流中断与新候选替换正文，复用已完成工具结果，不从头重跑 Agent。
- [ ] 路由事件接入 SSE、对话、通话浮窗和连接器；按实际尝试分别记录厂商/模型 usage。
- [ ] 使用受控服务检验失败分类与切换顺序，再用至少两家真实服务验收一级不可用、二级成功和下轮恢复。

验收：B01～B18。厂商保存、模型池配置、自动切换与切换提示必须连成实际调用流程。

## 6. D04：三态权限与连接器约束

**依赖：** D02。<br>
**输入：** AccessRule、AccessResolution；现有 PermissionId、PolicyDecision。<br>
**输出：** 三态设置实际控制所有相关调用路径。

修改：

- `backend/src/safety/policy_engine.rs`
- `backend/src/safety/execution_gateway.rs`
- `backend/src/safety/grant/model.rs`
- `backend/src/safety/grant/evaluator.rs`
- `backend/src/safety/grant/store.rs`
- `frontend/src/features/settings/sections/PermissionSettings.tsx`
- `frontend/src/features/security/GrantEditor.tsx`
- `frontend/src/api/system.ts`

拟新增：

- `frontend/src/features/security/AccessMatrix.tsx`
- `frontend/src/features/security/AccessScopeDialog.tsx`
- `backend/src/safety/tests/access_modes.rs`

步骤：

- [ ] 先给出现有风险/角色/授权合并路径，确定唯一决策函数，不在各工具内部复制策略。
- [ ] 为 full/approve/deny、范围匹配、禁止优先和同级冲突编写行为用例。
- [ ] 扩展授权存储表达 approve，处理旧规则迁移，移除 yolo/ask/plan 对新配置的隐式覆盖。
- [ ] 对用户已明确授权的 full 范围直接执行；approve 等待批准；deny 不调用工具。
- [ ] 更新审批 UI，并处理审批等待期间规则变化、取消和拒绝。
- [ ] 在连接器显示约束节点与有效权限摘要，节点打开相同规则编辑弹窗。
- [ ] 分别从对话、通话、工作流、MCP 触发同类操作验证结果一致。

验收：S01～S08。必须验证实际执行器是否被调用及文件/网络等外部结果，不能仅断言 UI 标签。

## 7. D05：能力导航、导入与本地发现

**依赖：** D01/D03/D04。<br>
**输入：** 现有 Skill/工作流/MCP/工具/插件数据；ImportSource/ImportPreview/LocalServiceCandidate。<br>
**输出：** 五类能力导航，弹窗管理，导入后可调用。

修改：

- `frontend/src/pages/CapabilitiesPage.tsx`
- `frontend/src/pages/SkillsPage.tsx`
- `frontend/src/pages/PluginsPage.tsx`
- `frontend/src/pages/WorkflowsPage.tsx`
- `frontend/src/components/workflow/WorkflowGraphEditor.tsx`
- `backend/src/api/skills_route.rs`
- `backend/src/api/mcp_runtime.rs`
- `backend/src/integrations/mcp/config.rs`
- `backend/src/plugin/manifest.rs`
- `frontend/src/api/plugins.ts`

拟新增：

- `frontend/src/features/capabilities/CapabilityManagerDialog.tsx`
- `frontend/src/features/capabilities/ImportCapabilityDialog.tsx`
- `frontend/src/features/capabilities/LocalServiceDiscovery.tsx`
- `backend/src/api/capability_imports.rs`
- `backend/src/modules/capability/import.rs`
- `backend/src/modules/capability/discovery.rs`
- `frontend/src/features/plugins/PluginFeaturePage.tsx`
- `backend/src/api/plugin_features.rs`

步骤：

- [ ] 从现有页面抽出可放入弹窗的内容组件，保留原有业务调用，不嵌套完整 PageShell。
- [ ] 首页按草图排列三列两行卡片：Skill、工作流、MCP、工具、插件、导入与本地发现。首次进入只显示导航，点击后才开对应弹窗；不在首页追加最近使用长列表。
- [ ] 工作流编辑使用大弹窗，运行结果在弹窗内查看。
- [ ] 实现目录/ZIP/Git Skill 导入与资源路径保存，工作流 JSON 和 MCP 配置导入。
- [ ] 插件导入沿现有清单格式实现解析与安装；在文档中写明支持的具体格式。
- [ ] Windows 枚举用户选定目录和本机监听端口，与读取到的配置候选合并；探测协议并显示候选来源。
- [ ] 选择模型候选写入基础配置，选择 MCP 写入能力；安装/启动走 D04 的权限。
- [ ] 分别导入一个实际 Skill、工作流、MCP 和插件，接给智能体并调用。
- [ ] 扩展声明式 features，提供已启用功能条目 API，更多页展示所属插件入口；功能页复用已安装贡献，不重复创建资源。
- [ ] 验证工具型插件不多出页面入口，带功能贡献的插件可进入使用，停用/卸载后入口同步撤下。

验收：A01～A08、I01～I06、X01～X03。

## 8. D06：连续通话与桌面浮窗

**依赖：** D03/D04。<br>
**输入：** 现有 VoiceRuntimeSnapshot、lease 与 generation；CallPresentation。<br>
**输出：** 一次点击连接、连续交谈、插话、独立浮窗与圆形态。

修改：

- `frontend/src/components/chat/ChatInput.tsx`
- `frontend/src/features/voice/ChatVoiceInput.tsx`
- `frontend/src/features/voice/GlobalVoiceHost.tsx`
- `frontend/src/features/voice/GlobalVoiceLeaf.tsx`
- `frontend/src/features/voice/useVoiceCapture.ts`
- `frontend/src/features/voice/runtime/useGlobalVoiceRuntime.ts`
- `frontend/src/api/voice.ts`
- `backend/src/modules/voice/runtime.rs`
- `backend/src/modules/voice/provider.rs`
- `backend/src/api/voice.rs`
- `src-tauri/src/main.rs`
- `src-tauri/capabilities/default.json`

拟新增：

- `frontend/src/features/voice/CallOverlay.tsx`
- `frontend/src/features/voice/callState.ts`
- `frontend/src/features/floating/FloatingSurfaceHost.tsx`
- `frontend/src/features/floating/windowBridge.ts`
- `src-tauri/src/floating_windows.rs`

步骤：

- [ ] 改为麦克风和呼叫图标，保留 Tooltip/aria-label。
- [ ] 实现 connecting 直到会话、设备、STT/TTS 均准备完成，状态不依赖定时器假定成功。
- [ ] 自动采集和断句，生成回复后自动合成播放；播放完成回到聆听。
- [ ] 加入插话检测与回声处理，取消旧回答和旧播放，使用现有会话失效机制。
- [ ] 创建无系统标题栏、可拖动、可置顶的 Tauri 通话窗口；收起到 64px，状态和音频 owner 不变化。
- [ ] 在多显示器、主窗口最小化、关闭浮窗和退出应用时验证生命周期。
- [ ] 真实设备完成 10 轮通话与 5 次插话，记录连接、首音频、停止播放时间。

验收：V01～V08。只有网页内浮层不能满足独立桌面浮窗验收。

## 9. D07：角色与用户提示词

**依赖：** D02/D06。<br>
**输入：** 智能体定义、来源、用户指令。<br>
**输出：** 官方模板不公开；用户角色在文字和语音中生效。

修改：

- `backend/src/api/agents.rs`
- `backend/src/agent/definition.rs`
- `backend/src/agent/engine.rs`
- `frontend/src/pages/AgentsPage.tsx`
- 智能体定义所在 `backend/src/modules/task/` 类型与数据库读写，按实际定义定位。

拟新增：

- `frontend/src/features/agents/PersonaDialog.tsx`
- `backend/src/agent/prompt_composition.rs`

步骤：

- [ ] 按来源区分项目模板与用户指令，将后者迁移到 user_prompt。
- [ ] 更新公开 DTO 与创建/编辑请求；项目模板正文不返回给前端。
- [ ] 角色编辑弹窗写入名称、语气、语言、自由指令和角色扮演开关。
- [ ] 建立唯一提示词组合函数，接入文字与语音 continuation。
- [ ] 用两个表达风格明显不同的角色进行真实文字和语音对话，确认下一轮切换生效。

验收：P01～P04。不能只隐藏 HTML 节点而仍在普通详情 API 下发项目模板。

## 10. D08：Token 监控与浮窗

**依赖：** D06 的浮窗容器。<br>
**输入：** 现有用量与系统数据。<br>
**输出：** 用量统计、筛选、趋势、桌面监控面板和圆形态。

修改：

- `frontend/src/pages/SystemPage.tsx`
- `frontend/src/components/system/MonitoringToolbox.tsx`
- `frontend/src/api/system.ts`
- `frontend/src/types/index.ts`
- `backend/src/api/llm_models.rs`
- `backend/src/db/llm_models.rs`
- `backend/src/integrations/llm/usage.rs`

拟新增：

- `frontend/src/features/monitor/TokenUsagePanel.tsx`
- `frontend/src/features/monitor/MonitorOverlay.tsx`

步骤：

- [ ] 将已有输入/输出/总量和 days 放入监控。
- [ ] 以请求 ID 处理流式最终用量，添加未知用量计数；检查失败请求的厂商 usage。
- [ ] 实现本地时区筛选与按模型统计，避免当天边界使用 UTC 午夜。
- [ ] 复用浮窗容器，监控内容只订阅数据，不获取麦克风。
- [ ] 通话与监控同时开启，验证收起、位置、置顶、关闭和主题同步。
- [ ] 发起实际模型请求，比对厂商响应 usage、数据库记录与 UI 总量。

验收：M01～M06。

## 11. D09：整合验收与交付

**依赖：** D01～D08。<br>
**输出：** 可运行桌面产品、逐项验收记录、缺陷修复和截图。

- [ ] 检查本版 11 项需求及后续导航、节点编辑、显示、布局、删除修订，逐项完成实际操作。
- [ ] 检查主界面、弹窗、独立窗口的浅/深色和透明度；检查 100%/125%/150% Windows 缩放。
- [ ] 检查已移除入口及跳转，确认连接器不存在旧任务执行操作。
- [ ] 完成真实厂商、真实语音、真实导入与权限流程后记录结果。
- [ ] 运行仓库要求的构建/检查；修复本版引入的问题。
- [ ] 更新 API/用户文档，记录最终文件、行为、验证命令与结果。Git 提交/推送按用户授权执行。

## 12. 验证命令

以下为产品实现后的验证入口，由主执行者在集成状态统一执行并记录实际结果。仅列出命令不表示已运行或已通过。每个代码块均从仓库根目录开始，不串接前一块的工作目录。

```powershell
# 前端，依赖已安装则不重复安装
Set-Location frontend
npm.cmd run test
npm.cmd run build
npm.cmd run test:e2e
```

```powershell
# 后端
Set-Location backend
cargo fmt
cargo fmt --check
cargo check
cargo test
```

```powershell
# 桌面窗口变更
cargo check --manifest-path src-tauri/Cargo.toml
git diff --check
```

前端 E2E 入口已核对为 `frontend/e2e/core-paths.mjs`；新增验收模块接入该 runner。自动化用例重点验证用户行为和副作用，不以源码包含某个中文标签替代行为测试。桌面浮窗、设备音频和真实服务另按验收表执行。
