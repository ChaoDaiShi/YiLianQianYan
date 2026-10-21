# 连接器工作台实现与验证记录

更新日期：2026-10-21。当前阶段：实现与集成验收。

用户已授权从设计进入产品实现，并允许按文件分工使用子智能体。当前工作分支为 `codex/connector-workbench`，设计基线为 `4399a43`。本文记录本轮实际代码接入；最终构建版本、测试结果和交付状态由主执行者在集成检查后填写。

本轮实现范围见第一节，自动化、API 与原生窗口结果见第三节，真实厂商与设备验收记录见第四节。[验收清单](acceptance.md) 保留 91 条操作标准；2026-10-20 的预览保留为设计依据，运行截图另行列出。

## 1. 当前实现范围与接入位置

下表的“已接入”指当前工作树中存在对应实现，不代表真实服务或设备验收已经通过。路径均相对于仓库根目录。

| 模块 | 当前代码已接入的行为 | 关键文件 |
|---|---|---|
| 导航与能力入口 | 连接器、更多独立页；指定旧页面显式路由移除；能力分类通过管理弹窗承载。能力与更多网格整体居中，按行从左向右、再从上向下排列，末行从第一列开始 | `frontend/src/components/layout/NavRail.tsx`、`navRegistry.ts`；`frontend/src/surfaces/workspace/WorkspaceSurface.tsx`；`frontend/src/pages/MorePage.tsx`；`frontend/src/features/capabilities/CapabilityNavigation.tsx`、`capabilityNavigation.css` |
| 连接器视图与绑定 | 多画布、布局/视口/显示设置持久化；真实资源绑定与解除；节点编辑；隐藏视图与解除业务绑定分开；聊天运行入口读取智能体、能力、模型、记忆和知识绑定 | `frontend/src/features/connectors/`；`frontend/src/api/connectors.ts`；`backend/src/api/connectors.rs`、`chat_runtime.rs`；`backend/src/db/connectors.rs` |
| 画布删除 | 指定连接器画布删除保留业务实体与绑定；历史任务图按版本删除，检查活动执行、更新运行时状态，保留审计数据；前端提供停止并删除流程 | `frontend/src/features/connectors/CanvasManager.tsx`、`connectorActions.ts`；`backend/src/modules/task/api/graph_routes.rs`、`runtime.rs`；`backend/src/db/task_world.rs` |
| 三态权限 | full/approve/deny 持久化；匹配禁止优先；其余按资源、工具/能力、智能体、默认排序，同级需批准优先；接入现有安全网关及批准前重新判断 | `frontend/src/features/security/AccessMatrix.tsx`、`AccessScopeDialog.tsx`；`backend/src/db/access_rules.rs`；`backend/src/api/access_rules.rs`；`backend/src/safety/execution_gateway.rs` |
| 基础配置 | 对话、STT、TTS、搜索、识图、生图、Embedding 入口集中到基础配置；账户与模型管理、服务请求/连接测试接线；凭据沿用现有 secret store | `frontend/src/features/settings/sections/BasicConfiguration.tsx`、`BasicServicePanel.tsx`；`frontend/src/pages/SettingsPage.tsx`；`backend/src/api/model_pools.rs`、`basic_services.rs`、`search.rs`；`backend/src/modules/settings/application/provider_test.rs` |
| 模型池与切换 | 对话/识图/生图按用途配置候选层级；固定档案与池分开绑定；按用户候选顺序尝试、分类失败、限制候选和总时限；流式中断通过路由事件替换正文；逐次记录用量及取消结束状态 | `frontend/src/features/llm/ModelPoolPanel.tsx`、`ModelRouteNotice.tsx`、`providerCatalog.json`；`backend/src/db/model_pools.rs`；`backend/src/integrations/llm/model_pool.rs`、`client.rs`、`usage.rs`；`backend/src/modules/settings/application/basic_services.rs`；`backend/src/tools/basic_services.rs` |
| 导入与插件功能 | Skill 目录及归档资源保存；结构化工作流/MCP 导入与本地服务发现入口；插件声明式贡献安装、启停和功能页；记录历史贡献归属并阻止已停用/卸载/移除的工作流启动 | `frontend/src/features/capabilities/StructuredImport.tsx`、`LocalServiceDiscovery.tsx`、`ManagedImports.tsx`；`frontend/src/features/plugins/PluginFeaturePage.tsx`；`backend/src/api/capabilities.rs`、`capability_plugins.rs`；`backend/src/modules/capability/import_store.rs`、`import_archive.rs`、`import_owner.rs`；`backend/src/modules/workflow/application/run_service.rs` |
| 用户角色 | 用户提示词与角色档案保存；公开详情区分用户指令与项目模板；聊天、任务执行等入口组合角色内容，权限继续由网关决定 | `frontend/src/features/agents/PersonaDialog.tsx`；`backend/src/api/agent_persona.rs`、`agents.rs`、`chat_runtime.rs`；`backend/src/modules/task/orchestrator.rs` |
| 通话与桌面浮窗 | 主窗口持有音频采集/播放；浮窗订阅状态与发送控制；连接检查、连续采集、插话接线；通话固定原对话，浏览其他对话后由用户显式切换；窗口创建、收起、展开与位置命令 | `frontend/src/features/voice/GlobalVoiceHost.tsx`、`CallOverlay.tsx`、`floatingSurface.ts`、`voiceContinuation.ts`；`frontend/src/surfaces/AppRoot.tsx`；`src-tauri/src/floating.rs`、`main.rs`；`src-tauri/capabilities/default.json` |
| 用量监控 | Token 汇总、日期与模型筛选、未知/失败/进行中计数；基础服务池中真实报告的 Token 加入统计；图片数量保留原始单位；监控浮窗复用主窗口的数据源 | `frontend/src/features/monitor/TokenUsagePanel.tsx`、`monitorStore.ts`、`usageRange.ts`、`MonitorOverlay.tsx`；`backend/src/api/llm_models.rs`；`backend/src/db/llm_models.rs`、`model_pools.rs`、`basic_services.rs` |
| 主题 | 昔涟主题变量、表面透明度与浮窗主题同步接线 | `frontend/src/theme/applyTheme.ts`、`ThemeProvider.tsx`；`frontend/src/styles/connector-workbench.css`；`frontend/src/features/voice/floatingSurface.ts` |

基础配置集中七类服务的操作入口与实际调用。对话模型复用 `llm_models`，识图/生图使用 `basic_service_profiles`；语音、搜索和 Embedding 复用各自已有配置，凭据继续通过项目的 SecretRef 与凭据存储读取。

本轮保留现有 React、Tauri、Rust、SQLite 技术栈。历史任务图只涉及本次明确要求的删除管理，不扩大为旧任务业务重建。

## 2. 集成审查后的针对性修正

- 识图/生图池按用途保存，不覆盖同一智能体的聊天模型绑定。
- 绑定智能体的记忆/知识范围可以为空；解除最后一条绑定后不扩大到全局检索。
- 聊天模型客户端先完成配置校验，再注册运行状态，避免初始化失败留下运行中记录。
- 通话上下文与页面浏览上下文分别维护，切换通话对话由用户操作触发。
- 模型池尝试在取消后写入结束状态，不永久累积为进行中。
- 插件被移除的贡献保留所有权记录；停用、卸载或新版本移除的工作流不能通过直接执行入口重新启动，历史运行审计保留。
- 识图返回的实际 Token 进入日期与模型汇总，避免只保存原始用量却漏计；纯图片数量不换算为 Token。

这些修正已进入当前工作树，并纳入第三节对应的单元、集成和交互验收。

## 3. 统一验证记录

验收环境为 Windows、PowerShell、项目已安装的 Node/Rust、系统 Edge 与实际 Tauri WebView2。浏览器工具不可用，使用项目已有 Playwright；没有安装额外全局软件。

| 检查层次 | 执行范围 | 结果 | 证据位置 |
|---|---|---|---|
| 前端自动化 | `npx vitest run` | 138文件、657用例通过，0失败 | `target/workbench-vitest.json` |
| 前端生产构建 | `npm run build`（TypeScript + Vite） | 通过；保留主包超过500kB的构建提示 | `target/workbench-build.log` |
| Rust格式与编译 | backend/src-tauri 的 `cargo fmt --check`；backend `cargo check` | 通过；现存dead_code等警告未扩大为无关修改 | `target/workbench-cargo-check.log` |
| Rust单元回归 | backend `cargo test --lib`，随后新增布局定向回归 | 1,059项单元、2项布局回归通过 | `target/workbench-final-rust-tests.log`、`target/workbench-layout-tests.log` |
| Rust集成回归 | architecture_boundaries、cycle6_voice_api、security_execution、task_canvas_api、typed_binding | 26项通过 | `target/workbench-final-rust-tests.log` |
| Tauri构建 | 独立QA配置构建、正常项目配置 `cargo build -p yi-lian-qian-yan --bin yi-lian-qian-yan` | 均通过；正常开发二进制恢复使用项目原配置 | `target/workbench-native-build.log`、`target/workbench-native-normal-build.log` |
| 浏览器与API | `npm run test:e2e`，真实后端与1440×900 Edge | 13项检查通过：导航、弹窗、绑定/隐藏/解除、避让、权限、删除、降级、用量 | `target/canvas-e2e/2026-10-21T01-16-21-807Z/result.json` |
| 原生桌面 | 实际WebView2 CDP、Tauri IPC与Win32窗口观测 | 10项检查通过：独立窗、置顶、64px展开收起、主题、最小化、关闭后停止轮询、错误路径、无未处理JS错误 | `F:/target/workbench-desktop/2026-10-21T01-10-14-108Z/results.json` |
| Git工作树 | `git diff --check`，核对分支与文件范围 | 通过；`codex/connector-workbench`，基线`4399a43`；未提交、未推送 | [变更文件清单](verification/changed-files.txt) |

机器可读结果见[验证摘要](verification/summary.json)，实际界面见[运行截图](runtime-previews/index.md)。本轮新增模块遵循600行边界，目录导入、连接器资源、模型池和基础服务测试已拆到各自模块，未增加架构豁免。

本轮回归涉及以下重点行为：

- 连接器布局独立保存、画布删除保留绑定、聊天采用绑定工具/记忆/角色、无效模型初始化清理。
- 权限规则持久化、优先级、完全授权、需批准与禁止执行。
- 模型池顺序、429/5xx 与请求错误分类、部分流中断、总时限、取消后尝试结束。
- `plugin_contributions_install_once_and_features_follow_enable_state`：贡献启停、卸载、版本移除与历史归属。
- `basic_pool_tokens_join_usage_without_counting_image_units_as_tokens`：识图 Token、日期、厂商/模型与图片单位。
- `local_usage_days_keep_unknown_failed_attempts_separate_and_filter_models`：本地日期、未知和失败用量及筛选。
- 前端导航、连接器投影与操作、语音 continuation/通话上下文、主题和用量日期范围测试。

## 4. 真实厂商与设备验收

原生窗口与受控HTTP调用已经验收。以下账户与声学场景尚未执行，后续按实际设备、厂商账户分别记录：

| 待验收项目 | 对应清单 | 需记录的结果 |
|---|---|---|
| 各厂商对应服务与账户凭据 | B03～B09 | 实际厂商、服务、模型、调用日期、成功或错误结果，不记录密钥 |
| 两家真实模型服务的降级与恢复 | B10～B18 | 实际尝试顺序、部分流处理、用量归属、停止后无后续候选 |
| 麦克风连续通话、扬声器回声与插话 | V02～V08 | 设备与网络条件、10 轮连续对话、5 次插话、实际延迟和挂断释放 |
| 多显示器与跨屏位置 | V06～V08、M05～M06、T04 | 跨屏拖动、位置恢复及不同Windows缩放；独立浮窗、最小化、置顶、展开收起和关闭已通过本轮原生验收 |
| 实际导入资源的调用 | I01～I05、X01～X03 | 选定的 Skill/工作流/MCP/插件来源、安装结果、调用产物及停用行为 |
| 真实请求用量核对 | M01～M04 | 厂商响应、持久化记录与 UI 汇总一致，未知用量保持未知 |

模型池的 HTTP 顺序、降级、回复持久化与用量已通过受控服务验证；厂商账户接通与设备声学效果按上表分别记录。无需将产品实现切换为其他执行模式。

## 5. 交付说明

- 桌面窄栏沿用当前版本的80px/68px结构，能力/更多内容网格居中且按行从左到右排列。
- 布局保存以canvas_id隔离；重进页面保留当前画布。新绑定节点避让已保存坐标，不挪动用户已有布局。
- 节点删除只隐藏视图；同时移除的附属连线不解除业务绑定。独立删除连线才调用解除API。
- 约束节点编辑当前智能体，全局约束只读展示；设置页继续管理全局规则。模型池绑定选择智能体名称，业务关系显示资源名称。
- 新增迁移使用1020（范围权限）、1021（连接器）、1022（图删除审计），保留原1018 ProofLoop与1019对话目标迁移。
- STT支持OpenAI兼容、MiniMax、Fish、阿里；TTS另支持火山HTTP。联网搜索支持AnySearch、Tavily、Brave、博查。18项对话厂商/本地预设由前后端共用目录提供。
- 测试使用独立数据、工作区与WebView目录；不复制用户配置或凭据，不关闭用户原有实例。截图只捕获本轮应用视图。
- 正常桌面开发二进制位于`target/debug/yi-lian-qian-yan.exe`，QA二进制与脚本另存`F:/target/workbench-desktop`。没有生成安装包，没有提交或推送Git。
