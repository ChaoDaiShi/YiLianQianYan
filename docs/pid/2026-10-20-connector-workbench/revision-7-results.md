# 第7轮开发与验证结果

日期：2026-10-21。分支：`codex/connector-workbench`。对应最新六项反馈。

## 行为修正

| 问题 | 修正 |
|---|---|
| 工具过程被画成节点 | 消息投影只保留用户输入和助手输出，跳过带tool_calls的协议行及工具结果。父子边直接跨过这些过程行。工具、思考、审批和完整分支上下文继续保存在会话历史。 |
| 对话右侧空槽 | 收起时实际网格仅为会话列表与聊天两列；不再创建第三个保留列。Git面板填满其实际侧栏宽度。 |
| Git自动出现、目录来源错误 | 工作/创造模式与会话切换不再自动打开Git。用户从顶部手动打开后，默认尚未选择工作空间，也不请求Git。状态与差异接口必须传入显式workspace_id，且该空间有绝对本地目录；禁止回退到应用当前目录。 |
| 设置位置与展开视角 | 默认类别围绕中心对称排列，子节点列以类别纵向居中。展开定位类别和子节点，收起定位分类概览。“居中”仅调整相机，“整理布局”由用户主动重排位置，保留连接及展开状态。 |
| Codex插件技能被当成本项目可用能力 | 根据实际工具注册表检查技能声明与已识别宿主调用。Read/Write/Edit/Glob/Grep/单次命令有明确项目工具与参数适配；缺少宿主能力的技能不进入AI可用列表，也不能由load_skill冒充执行。技能页默认仅显示本项目可用流程，复选框可查看宿主专用项、缺失工具与适配入口。 |
| 工具仍是旧能力全页 | 工具管理独立为单列紧凑列表，使用/api/tools的实际运行时工具。点击一项进入单项说明与参数，再返回列表；移除能力类型、Provider、状态筛选、双栏总览与新增能力来源区。节点编辑也使用同一工具管理器。 |

## 接口与数据

没有删除对话执行数据。消息画布API的nodes只投影user/assistant，已有对话处理过程和审批记录保留。

`GET /api/workspace-git`及`GET /api/workspace-git/diff`现在要求`workspace_id`。没有选择或目录为空/相对路径时返回400，不能用应用工作目录替代。前端在没有选择时不发出请求。Git读取仍不修改index、提交或远端。

Skill兼容性包含`status`、`runtime_ready`、`required_tools`、`missing_tools`、`tool_mappings`与`reason`。当前状态为portable/adapted/external_host。外部原始文件保持不变，load_skill返回真实根目录和适配说明；当前实际可用工具清单来自内置工具和已就绪MCP注册结果。带作用域的Bash参数不是新的工具依赖。

## 文件

- 消息投影：`backend/src/db/connector_messages.rs`、`connector_messages/overview.rs`与测试。
- 聊天与Git：`frontend/src/pages/ChatPage.tsx`、`features/git/GitNodeManager.tsx`、`gitNodeManager.css`、`components/chat/chatProcess.css`、`styles/ui-optimization/chat.css`；`backend/src/api/workspace_git.rs`与测试。
- 设置：`features/settings/canvas/settingsGraph.ts`、`SettingsCanvasPage.tsx`、`features/connectors/ConnectorCanvas.tsx`。
- Skill：`backend/src/tools/skill/compatibility.rs`、`discovery.rs`、`skill.rs`、`api/skills_route.rs`、`app/state.rs`、`modules/capability/runtime_providers.rs`；`frontend/src/pages/SkillsPage.tsx`及技能类型。
- 工具管理：新增`features/capabilities/ToolManager.tsx`、`toolManager.css`；接入`CapabilityNavigation.tsx`与`ConnectorNodeEditor.tsx`，收紧`api/system.ts`工具返回类型。
- 验收：新增`frontend/e2e/workbench-revision7.mjs`；默认test:e2e指向本轮。

## 验证

| 检查 | 结果 |
|---|---|
| 前端完整测试 | 156文件，743/743通过 |
| 前端生产构建 | PASS |
| Rust单元测试（最终源码） | 1118/1118通过 |
| 架构 / 安全集成 | 5/5、10/10通过 |
| Skill参数声明集中回归 | 3/3通过 |
| cargo fmt --check / cargo check | PASS |
| 后端server与独立Tauri构建 | PASS |
| 浏览器端到端 | 28/28通过 |
| Tauri桌面工作台端到端 | 28/28通过 |
| 原生窗口/浮窗回归 | 10项通过，含2次真实收起圆形拖动 |
| 原生未处理JS异常 / 意外控制台错误 | 0 / 0 |

几何验收：1440px浏览器中默认两列为250px和1110px，聊天右边缘空隙0；1200逻辑像素桌面客户区中为250px和870px，右边缘空隙0。手动展开Git前无Git请求，未选择工作空间仍无Git请求；选择后仅请求该登记workspace_id。实际工作区差异与文件内容相符，Git index字节保持不变。

设置展开类别与五个子节点的联合包围框横向偏差为0；浏览器纵向偏差约0.14px，原生桌面约-0.05px。检测期间确实复现过横向偏移300px，最终修正React Flow排队选项被重渲染覆盖的问题；保留同一几何断言复测通过。旧保存布局可以由用户点击“整理布局”主动重排。

Skill验证使用隔离工作空间的实际.codex/skills与.claude/skills文件。具有allowed-tools: Write的技能返回Write→write_file参数适配，后续模型请求真实调用项目write_file，文件skill-procedure-result.txt实际生成且内容核验相符。依赖tools.use_figma但未注册对应工具的技能默认不显示，勾选后显示依赖原因，且不出现在提供商收到的Available Skills中。外部原文件未被适配器改写。

工具弹窗检查无combobox、旧capability-split-layout、能力概览、新增能力来源；使用独立列表进入写入文件详情，再返回完整列表。消息画布检查没有process节点、工具中间文字或会话组底板，分支依旧保留正确祖先。

命令：`npm --prefix frontend run test`、`npm --prefix frontend run build`、`npm --prefix frontend run test:e2e`、`cargo fmt --check`、`cargo check`、`cargo test -p yilian-backend --lib`、`cargo test -p yilian-backend --test architecture_boundaries --test security_execution`、`cargo test -p yilian-backend --lib tools::skill::compatibility::tests`。原生QA从当前源码构建独立二进制，使用实际WebView2及Tauri IPC。

[验证摘要](verification/revision7-summary.json)、[原生结果](verification/revision7-native.json)、[浏览器结果](verification/revision7-browser.json)、[原生构建证据](verification/revision7-native-build.json)、[22张实际预览](revision7-previews/index.md)。前18张为实际Tauri桌面截图，后4张为浅色浏览器对照；全部保留原图并记录SHA-256。

Skill是操作规程，不会安装原宿主插件或注册原宿主函数。静态工具适配与实际执行分别验收；具体步骤仍经过项目既有权限、审批、取消与结果验证。测试使用隔离数据、受控模型协议和真实文件/Git/SQLite/Tauri窗口，未读取用户密钥或停止用户桌面实例。本轮不提交、推送或发布安装包。
