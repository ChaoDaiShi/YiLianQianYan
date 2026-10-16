# 00 基线与 Gemini 建议对照

## 1. 核对范围与证据边界

- 仓库根目录：`F:\项目开发\忆涟千言\YiLianQianYan`。
- 原稿：用户附件 `C:\Users\25113\.codex\attachments\e1708cc5-bd8a-465e-975e-968f1a46c438\已粘贴的文本.txt`。此附件是设计意见，不是运行证据；本文已归纳必要内容，实施者不必持有该附件。
- 本次基线：2026-10-16，`develop`，`bb39f95d124d43dad660c5429073f4fc1cfad5d5`。
- 核对方式：读取页面、公共组件、主题入口、API 类型、CSS 清单、既有计划和 E2E 入口；未启动浏览器、桌面窗口或后端。
- 结论用语：“源码已有”仅代表代码中存在；“运行通过”必须有本轮实际执行证据。

所有下文 `frontend/...` 路径均相对于仓库根目录。标注“拟新增”的文件在本次文档任务中不会创建。

## 2. 开工前执行的只读命令

```powershell
Set-Location 'F:\项目开发\忆涟千言\YiLianQianYan'
git branch --show-current
git rev-parse HEAD
git status --short
rg --files -g AGENTS.md -g '!node_modules' -g '!target'
Get-Content frontend/package.json
```

将结果写入工作包交接记录。存在其他改动时记录路径，不清理、不覆盖。分支或文件结构改变时，只更新实际受影响的映射，不照搬旧补丁。

## 3. 原稿中必须校正的前提

| 编号 | 原稿说法 | 当前源码事实 | 本计划决定 |
|---|---|---|---|
| K01 | 四套原生主题可选，暖色默认 | `ThemeProvider.normalizeStoredTheme` 归一到 Cyrene；`applyThemeToDom` 固定 `data-theme=cyrene-ripple`；外观页只提供模式切换 | 用浅色/深色/跟随系统三卡预览；不恢复四套主题 |
| K02 | 所有样式改 `index.css` | `index.css` 只是有顺序的导入清单；样式分布于 `styles/` | 页面独立 CSS；I 独占清单 |
| K03 | 正文字号 13–14px、导航 11px | `theme/typography.json` 正文16、阅读17、控件14、说明13、标签12 | 保留可读性基线，不整站缩字 |
| K04 | 三栏默认展开，边缘悬停唤醒 | `ChatPage` 右栏默认收起；52px ContextRail 已存在 | 保留默认收起和点击展开；不加悬停开栏 |
| K05 | 悬停运行日志自动移动画布 | 当前任务画布约定选中不改镜头，定位须显式触发 | 悬停只突出；点击“定位节点”才移镜头 |
| K06 | 任务仅有五种状态 | `api/tasks.ts` 还含 draft、ready、waiting_approval、waiting_user、blocked | 筛选可分组，展示仍保留完整状态 |
| K07 | 空间有知识库、子代理、会话三项计数 | `Workspace` 有 `active_tasks?`，没有这些绑定数量 | 显示真实活跃任务；不补三个假数字 |
| K08 | 记忆置信度条 | `MemoryRecord` 没有 confidence；metadata 也未约定置信度语义 | 本轮不显示置信度；不拿检索分数代替 |
| K09 | 网络指标和各模块响应耗时 | `SystemData` 有 CPU/内存/磁盘/GPU；`RuntimeHealth` 无模块时延或 SSE/MCP 健康字段 | 使用已存在字段；不生成网络数值或统一绿灯 |
| K10 | “已暂停·积压 N 条”的实时日志流 | `LogsPage` 每2秒轮询；暂停时不拉取，无法知道新增数量 | 写“自动刷新中/已暂停”，不写未知积压数 |
| K11 | 设置页需要重新分四组 | 已有模型、语音、智能体、安全、扩展、外观六组和旧深链映射 | 保留六组，只优化布局与反馈 |
| K12 | 点击 API Key 显示已保存明文 | 当前密钥有保留、替换、显式清除语义，不回传已保存值 | 可切换本次输入明文，不能显示凭据库原值 |
| K13 | 思考链与“本会话始终允许”按钮 | 可用的是执行事件/摘要和既有审批回调 | 展示已有行动摘要；不构造模型内部推理，不新增授权范围 |
| K14 | 共14页就是全部应用 | 还有任务画布、搜索、账号、初始化、语音宿主及重定向 | 纳入公共变更兼容检查，保留已有能力 |

## 4. 逐页面任务映射

| 原稿页面 | 当前入口与主要依赖 | 本轮优化 | 明确不冒充已有的能力 | 工作包 |
|---|---|---|---|---|
| 01 对话 | `pages/ChatPage.tsx`；`components/chat/`；`features/execution/` | 阅读区、输入舱、工具摘要、侧栏、滚动和审批展示 | 持久化置顶/改名、`/`技能执行、`@`引用、任意重发编辑 | B1–B5 |
| 02 任务 | `pages/TaskCenterPage.tsx`；`components/tasks/TaskDetailPanel.tsx` | 筛选计数、任务/会话来源、主从详情、成果入口 | 统一假进度、“retry”自动改称断点恢复 | C1–C2 |
| 03 空间列表 | `pages/WorkspacesPage.tsx`；`api/workspaces.ts` | 文件夹卡、路径复制、创建表单、真计数 | 初始挂载子代理、知识库绑定 | C3 |
| 04 空间详情 | `pages/WorkspaceDetailPage.tsx` | 概览、路径、真实任务列表与创建入口 | 无端点的 Knowledge/Subagents 页签；仅改URL就声称携带空间上下文 | C4 |
| 05 工作流 | `pages/WorkflowsPage.tsx`；`components/workflow/` | 编辑/列表/运行区、节点态、显式定位 | 与 TaskWorld 合并成一个引擎，悬停镜头移动 | D1–D4 |
| 06 智能体 | `pages/AgentsPage.tsx`；`api/tasks.ts` | 角色卡、instructions 阅读、已有创建流程 | 未提供的更新/复制/指定角色对话动作 | E1 |
| 07 技能 | `pages/SkillsPage.tsx`；`api/plugins.ts` | 技能卡、来源/路径/可编辑性、正文阅读编辑 | 没有结构化字段的触发命令、参数schema和启用开关 | E2 |
| 08 插件 | `pages/PluginsPage.tsx`；`api/mcpRuntime.ts` | 连接状态、启停、配置、工具/资源/Prompt 详情 | 把 enabled 当 connected，把传输失败猜成缺密钥 | E3 |
| 09 能力 | `pages/CapabilitiesPage.tsx`；`components/capabilities/` | 单行过滤、矩阵/卡片、状态说明、详情 | 自动调用测试、有副作用的“诊断” | E4 |
| 10 记忆 | `pages/MemoryPage.tsx`；`stores/memoryStore.ts` | 分类/来源、正文、详情、增改删反馈 | 置信度、任意解析 metadata 创建指标 | F1–F2 |
| 11 知识库 | `pages/KnowledgePage.tsx` | 已有与未接入能力的分区、记忆入口 | RAG 已运行、文件已索引、文档数量 | F3 |
| 12 监控 | `pages/SystemPage.tsx`；`components/system/` | 真指标小图、信息层级、最近刷新状态 | 网络监控、未采集的服务耗时 | G3 |
| 13 日志 | `pages/LogsPage.tsx`；`api/system.ts` | 控制台、级别、搜索、暂停、详情键盘切换 | 未知积压量、不存在的 JSON payload 属性 | G4 |
| 14 设置 | `pages/SettingsPage.tsx`；`features/settings/` | 六组导航、模式预览、未保存栏、测试反馈 | 四主题、权限四档映射、伪造 Ping 时延 | G1–G2 |

## 5. 公共建议的处理结果

| 建议 | 决定与理由 |
|---|---|
| 背景、结构、内容、浮层分层 | 采用四层 L0–L3；原稿称“三级”却列四层，本文统一叫四层 |
| 所有可点击卡片上浮2px | 仅用于静态资源卡；日志行、任务行、画布节点不移动，避免点击目标跳动 |
| 每个空态、状态点持续呼吸 | 只对实际进行中的动作使用柔和动效；空态与已连接常亮 |
| PageHeader滚动16px后吸顶 | 各页已有独立滚动容器，优先让头部作为非滚动兄弟节点；不新增全站 window scroll 监听 |
| Modal退出动画 | 采用；必须在退出完成后卸载，不能 `open=false` 立刻 return null 还宣称有退出动画 |
| Drawer拖宽360–640px | 只对宽窗口显式启用；窄窗按可用宽度限制；键盘也能调整 |
| 输入报错水平抖动 | 改为稳定错误文本、边框和焦点关联；不靠晃动表达失败 |
| Tooltip固定深色 | 使用当前浮层语义色；侧栏和内容区都要保持文字可读，不硬套旧侧栏色值 |
| Esc关闭浮层或停止生成 | Esc只关闭最上层浮层/当前辅助栏；停止生成保留显式按钮，避免关闭弹窗顺带取消任务 |
| Ctrl+F全局劫持 | 保留浏览器/编辑器查找；页面搜索用非输入状态下的 `/` |
| Ctrl+B / Ctrl+J侧栏切换 | 可加在聊天页，先满足无输入焦点、无输入法合成、无浮层抢占条件 |
| 自动诊断、自动连接重试 | 本轮只优化现有手动动作反馈，不新增重试策略 |

## 6. 对已有实现必须保留的行为

- `/tasks` 已含目标输入、ProofLoop 计划审核与任务图入口；不能用一个五分类卡片页将它们覆盖。
- `TaskDetailPanel` 已接入资源、成果和记忆转技能候选；改页签时要保留入口和真实操作。
- 插件详情已含工具、资源、资源模板和 Prompt；不能只剩服务开关。
- 聊天已有资源附件和全局语音接入；不能重造一个只支持文字发送的输入框。
- 当前工作流编辑器是自有布局加 SVG 连线；任务画布使用另一套状态和组件。二者不是同一数据结构。
- `styles/stylesheet.ts` 的读取清单未覆盖 `index.css` 末尾的 settings/contextRail/cloudExperience；P0由I同步本次需要的清单，不把文本测试看不到这些样式当成样式不存在。
- 既有 `*.test.ts` 中有源码字符串断言。改 DOM 后先检查真实交互，再调整过时断言；不能删掉行为要求来获得绿色结果。

## 7. 源码引用入口

- 主题真入口：`frontend/src/theme/ThemeProvider.tsx`、`applyTheme.ts`、`presets.ts`、`typography.json`。
- 运行路由：`frontend/src/surfaces/workspace/WorkspaceSurface.tsx`。
- 样式顺序：`frontend/src/index.css`、`frontend/src/styles/stylesheet.ts`。
- 原有右栏决定：`docs/superpowers/specs/2026-10-06-context-rail-layout-design.md`；本次仍以当前源码为准确认其已落地。
- 最近业务实现记录：`docs/v1.2/implementation-2026-10-16.md`，只用于理解已有入口，不复用其中结果作为UI新验收。
- 测试入口：`frontend/e2e/core-paths.mjs`、`frontend/e2e/ui-foundation.mjs`。
