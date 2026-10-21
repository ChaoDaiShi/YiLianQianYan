# 第4轮开发与验证结果

日期：2026-10-21。分支：`codex/connector-workbench`。本轮对应用户最新8项验收反馈，代码和文档保留在当前工作区，未提交、推送或发布安装包。

## 行为变化

| 反馈 | 交付行为 |
|---|---|
| 弹窗宽、字号大 | 常规设置最大560px，模型管理740px，能力管理740px；外观新增12–20px字号控制，默认14px，自动保存并随主题应用。列表和较长表单在弹窗内部滚动。 |
| 能力弹窗顶部分类 | 移除跨类别标签，首页卡片决定打开哪个管理器，弹窗只呈现对应内容。 |
| 权限复杂 | 完全审批、部分审批、完全权限三个入口；原范围规则收进高级约束；明确禁止、沙箱和角色拒绝继续执行。 |
| 命名与顺序 | 主页置顶，连接器改为画布。主页与对应消息画布双向跳转；未绑定会话由用户选择实际目标画布。 |
| 后台运行与审批恢复 | 独立会话运行时持有SSE和执行状态；导航卸载不停止任务。刷新读取实际运行状态、执行记录和待审批。审批、消息、终态使用同一会话交接锁与事件写入屏障。 |
| 工具显示与右栏 | 进度为可展开摘要，工具记录为紧凑行。待确认操作独立可操作，当前动作与历史不重复；失败历史不抢占当前动作。 |
| 工具错误日志 | 从既有持久化执行记录读取参数、结果与验证原因，支持报错/全部、搜索、详情和返回会话。系统日志先筛选后截取，并保留筛选来源选项。 |
| 模型与思考模式 | 输入框选择实际模型档案及标准/深度思考。请求选项保存到本地，审批续接复用同一选择；不改全局默认。DeepSeek工具链保存并回传reasoning_content。 |
| 设置默认与插件节点 | 新设置画布默认14项系统节点，已有选择可点“显示全部系统设置”。节点管理器常态收起，分系统设置与插件设置；插件启用并声明设置后出现入口，可连接、编辑与保存。 |

## 后台工作的实际验证

本轮在普通浏览器与真正Tauri窗口分别执行：输入工具任务 → 出现待审批 → 切去能力页 → 返回原会话 → 允许本次 → 再切去设置页 → 后台完成文件写入与回复 → 返回显示已保存答案。另执行审批后立即刷新，让原SSE观察连接断开，再从持久化状态恢复到完成。检查了工作目录中`qa-background-route.txt`和`qa-background-refresh.txt`的实际内容，均为`BACKGROUND_EXECUTION_OK`。

模型选择使用两个不同档案验证：全局默认`deepseek-v4-flash`，输入框选择`deepseek-chat`。首轮和审批续接的真实HTTP请求都使用`deepseek-chat`及`thinking.type=enabled`，续接携带实际保存的reasoning_content；全局默认仍是原档案。模型响应由本地可控HTTP服务提供，工具调用、审批、文件写入、SQLite保存及桌面交互均运行项目的实际实现。

## 代码入口

- 会话生命周期：`frontend/src/features/execution/conversationRuntime.ts`、`chatRuntime.ts`、`components/chat/ChatView.tsx`；后端`api/chat_lifecycle.rs`、`api/chat.rs`、`api/approvals.rs`与`db/conversations.rs`。
- 审批与模型：`features/security/ApprovalModeControl.tsx`、`components/chat/ComposerModelControls.tsx`、`chatRequestOptions.ts`、`features/settings/sections/PermissionSettings.tsx`；后端`api/chat_options.rs`、`integrations/llm/thinking.rs`、`safety/execution_gateway.rs`与配置类型。
- 主题与弹窗：`theme/ThemeProvider.tsx`、`applyTheme.ts`、`AppearanceSettings.tsx`、`components/ui/Modal.tsx`与`features/capabilities/CapabilityNavigation.tsx`。
- 画布导航：`components/layout/NavRail.tsx`、`navRegistry.ts`、`features/connectors/ConnectorsPage.tsx`、`conversationCanvasTarget.ts`、`messages/MessageBranchWorkspace.tsx`。
- 插件设置：新增后端`plugin/settings.rs`、`api/plugin_settings.rs`，扩展`api/settings_canvas.rs`；前端`SettingsNodeManager.tsx`、`PluginSettingsDialog.tsx`与`SettingsCanvasPage.tsx`。
- 日志：后端`api/logs.rs`与新增`api/tool_logs.rs`；前端`pages/LogsPage.tsx`与新增`features/logs/`。

## 验证结果

| 检查 | 结果 |
|---|---|
| 前端完整测试 | 146文件，700/700通过 |
| 最后弹窗尺寸调整的聚焦测试 | 2/2通过 |
| 前端生产构建 | PASS |
| Rust单元测试 | 1078/1078通过 |
| 架构边界 / 安全执行集成 | 5/5、10/10通过 |
| cargo fmt --check / cargo check | PASS |
| 普通浏览器端到端 | 15/15流程通过 |
| 原生桌面 | 10项原生窗口/浮窗检查通过，复验本轮15项工作台流程通过 |
| 原生脚本异常 / 意外控制台错误 | 0 / 0 |

前端使用`npm --prefix frontend run test`、`npm --prefix frontend run build`、`npm --prefix frontend run test:e2e`。端到端端口为前端11420/后端19420。原生窗口前端11422、嵌入式后端19422、WebView2 CDP19423。Browser插件未提供，使用项目已有Playwright与系统Edge、真实WebView2。测试工作目录、窗口和进程独立，未接管或关闭用户启动的桌面实例。

本轮原生构建使用外部QA工程复制当前`src-tauri`源码并逐文件SHA-256核对，沿用项目依赖和构建配置，输出不同文件名，避免覆盖正在运行的开发程序。后端直接引用当前仓库。

机器可读证据：[验证摘要](verification/revision4-summary.json)、[原生结果](verification/revision4-native.json)、[浏览器结果](verification/revision4-browser.json)、[原生构建与源码哈希](verification/revision4-native-build.json)。[九张原生预览](revision4-previews/index.md)均为原始截图，附来源和哈希。

## 实际边界

整页刷新后恢复后端实际运行状态、审批、工具记录与完成答案；刷新前尚未保存的流式片段不回放。页面内导航保留原SSE持续接收。思考协议目前识别DeepSeek和支持的OpenAI推理型号，其他模型使用标准模式。此次没有发起付费官方厂商验收调用；协议、路由选择及完整执行链使用可控模型服务验证。生产构建保留大chunk提示及Rust现有未使用项提示，无编译错误。
