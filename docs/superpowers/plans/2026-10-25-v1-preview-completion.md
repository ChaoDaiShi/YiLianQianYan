# v1 预览界面补齐

**目标：** 将用户指定的 `2026-10-21-v1-major-update/previews/index.html` 及 R2 交互规格落实到当前应用，保持既有主题、技术栈和已实现功能。

**基线：** `develop` / `7d1d145`，与 `origin/develop` 同步；实施分支 `codex/complete-v1-preview-ui`。原有未跟踪 `.agents/skills/`、`artifacts/` 保留。用户明确允许子智能体，未授权提交、推送或部署。

## 实施与验收

- [x] A：工作空间改为文件夹列表与真实最近成果；详情默认文件树/预览，任务、成果按页签进入，Git 按需打开。新增限定注册目录的只读文件接口，测试正常读取及越界拒绝。
- [x] B：角色搜索/列表/单一编辑区；记忆列表与详情弹窗；时间任务列表及状态筛选。保留已有审批、撤回、角色与资源绑定语义。
- [x] C：学习独立页面，资料/笔记/练习分区；创作文稿编辑、版本保存与导出。复用学习 API；缺失的创作持久化按原契约最小补齐并测试。
- [x] 主集成：聊天历史默认收起并可重新打开；更多功能采用两列入口并连接学习/创作空间选择；账号默认身份和数据操作，登录按需；首次配置集中当前步骤；知识集合导航与新建；日志导出及详情弹窗。
- [x] 逐项复查消息画布、能力、设置、监控、插件、语音、渠道，保留已有真实功能和主题，不重画、不填假数据。
- [x] 执行 frontend 测试和生产构建；Rust 若变更执行 fmt、fmt --check、check 与新增模块测试；浏览器实际检验默认页、入口、编辑/保存/返回、空态、错误、1366/960/390 宽度。

## 约束

复用现有语义 token 和 PageShell、Modal、Button 等组件。固定工作区，次要操作弹窗，普通页面不出现横向溢出。概念图中的名称、统计、文件和状态均不作为产品数据；不照抄图中文字错误。不新增依赖、防御性兼容层或静默 fallback。真实后端、受控模型、静态构建和浏览器布局证据分别报告。

## 已落地范围

| 预览 | 实现结果 |
|---|---|
| 01 对话 | 历史默认收起、全尺寸开关及 Ctrl+B；推荐任务按需展开；非图片附件为紧凑文件行 |
| 02 消息画布 | 保留现有真实消息分支和画布入口，复核原实现 |
| 03/04 工作空间 | 文件夹列表、真实最近成果；详情文件/任务/成果页签，真实目录与 Markdown/文本预览，Git 弹窗，绑定空间开始对话 |
| 05 能力 | 保留现有分类、工作流与导入表单，复核现有实现 |
| 06 智能体 | 搜索列表与单角色编辑器；沿用真实 profile/资源配置；未保存编辑保留在当前窗口 |
| 07 记忆 | 当前列表为主，筛选按需，详情弹窗；保留 active/withdrawn 及现有治理语义 |
| 08 知识 | 集合侧栏、新建集合、当前集合导入、原文阅读；窄窗口保留集合选择和新建入口 |
| 09 设置 | 保留设置画布和已有六个功能分组及编辑器，不删除当前可用设置 |
| 10 监控 | 服务概览与真实用量优先，工具箱/审批/设备详情按需展开；保留审批深链接 |
| 11 日志 | 真实记录、当前筛选记录 JSON 下载、单条详情弹窗 |
| 12 账号 | 默认本地身份与数据操作；登录表单按需展开；同步/隐私原入口保留 |
| 13 首次配置 | 顶部步骤栏，保存与测试分离；使用当前模型管理 API 保存、验证、激活，解决仅保存旧 settings 不产生学习可用模型的问题 |
| 14 更多 | 两列功能入口、单独插件区域、学习/创作工作空间选择弹窗 |
| 15 插件 | 复用已安装工作流真实参数表单；运行结果弹窗和管理入口 |
| 16 定时 | 列表、状态筛选、真实下次触发、启停与次级菜单；原机器人/审批语义保留 |
| 17 学习 | 独立固定工作区，资料/笔记/练习页签、版本、来源和作答 |
| 18 创作 | 独立文稿编辑/阅读、SQLite + Resource 版本存储、历史对照与恢复草稿、Markdown/TXT 导出；图片/办公文档显示已有真实成果 |
| 19 语音 | 保留现有全局语音浮层与会话绑定；没有改变设备/音频执行 |
| 20 渠道 | 保留此前已实现的账号/会话/预设管理界面，检查入口与窄窗口 |

## 关键文件

- 修改：`pages/ChatPage.tsx`、`WorkspacesPage.tsx`、`WorkspaceDetailPage.tsx`、`AgentsPage.tsx`、`MemoryPage.tsx`、`MorePage.tsx`、`AccountCenterPage.tsx`、`SetupWizardPage.tsx`、`SystemPage.tsx`、`LogsPage.tsx`，以及已有知识、日志、插件、附件、定时功能组件。
- 新增：`features/workspace-preview/`、`features/creation/`、`features/learning/WorkspaceLearningPage.tsx`、`features/agents/PersonaEditor.tsx`、`api/workspaceFiles.ts` 以及四份局部预览样式。路径均相对 `frontend/src/`。
- 后端新增 `api/workspace_files.rs`、`api/creations.rs`、`db/creations.rs`；接入既有路由和数据库初始化。文稿迁移为 `1066_workspace_creations`，仅为文稿新增元数据/版本关系，正文仍使用已有 ResourceService，不改变原 Artifact 的已验证执行成果语义。
- 同步 `README.md`、前端路由和样式清单；更新三处因卡片→列表、Drawer→Modal 而过期的显示契约断言。

## 验证与边界

- `npm.cmd run test -- --reporter=dot`：167 文件、791 测试通过。
- `npm.cmd run build`：TypeScript 和 Vite 生产构建通过；仍有大于 500 kB 分块的构建提示。
- `cargo fmt`、`cargo fmt --check`、`cargo check`、`cargo build --bin yilian-server`：通过；保留原有 deprecated/dead_code 等警告。
- `cargo test --lib api::workspace_files::tests -- --nocapture`：Windows 3 项通过；Unix 符号链接测试不在本机执行。
- `cargo test --lib creations::tests -- --nocapture`：数据库及真实资源 API 3 项通过，含历史版本字节、父版本、修订冲突、跨空间和归档行为。
- 浏览器：系统 Edge + 项目已有 playwright-core。当前环境没有 Browser 插件，使用现有 Playwright，未安装依赖。验收地址为 `http://127.0.0.1:1438`，独立后端为 `127.0.0.1:9438`，全部使用 `F:/tmp/yilian-preview-20261025` 隔离数据。
- 共 54 个页面/尺寸检查点（1366×768、960×768、390×768）；没有页面级横向溢出、空白页或框架错误。390 宽度为窄窗口检查，不代表完整移动端产品验收。
- 15 项浏览器流程检查通过：历史开关、学习空间选择、账号登录按需、知识集合导入/原文、角色保存刷新与未保存草稿、真实文件预览/越界、空间页签、文稿版本/草稿/下载、首次配置模型保存验证激活、学习笔记与评分、插件工作流参数与结果。浏览器无 pageerror，最终集成检查无 console error。
- 学习及首次配置的模型服务为本地受控 OpenAI 协议 fixture；学习存储、笔记版本、评分和 API 使用真实后端，不代表真实商业模型效果。插件清单在测试中受控注入，运行参数、执行和结果使用真实工作流 API，未安装示例插件。
- 未在本轮验收真实语音硬件、Tauri 安装包、真实外部消息收发、新图片或办公文档生成；未执行后端全量测试或原 R7 全量 E2E。没有新增依赖、提交、推送、部署；原有 `.agents/skills/`、`artifacts/` 保留。

浏览器证据与命令日志保存在仓库外 `F:/tmp/yilian-preview-20261025`，其中 `main-checks.json` 与 `integrated-checks.json` 为本轮实际检查结果。
