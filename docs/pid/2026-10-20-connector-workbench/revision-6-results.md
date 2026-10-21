# 第6轮开发与验证结果

日期：2026-10-21。分支：`codex/connector-workbench`。对应最新五项验收反馈；未提交、推送或发布安装包。

## 实现行为

| 反馈 | 当前实现 |
|---|---|
| 画布不应有额外组底板 | 画布只生成实际消息和实际工具过程节点。主线自上而下连接，另一条路径从来源回答连接；相连分支在主页归为同一个会话族，以路径选择器切换。没有关系的根会话独立。拖动保存实际消息绝对坐标，旧布局读入时转换相对坐标。 |
| 二级设置应展开收敛节点 | 点击六类设置节点展开或收起相连的实际子节点，状态与位置保存。类别不打开选择弹窗；点击具体子节点进入其参数编辑器。插件声明的设置归入扩展类别及节点管理器。 |
| 保留过程、去头像、顶栏控制 | 每轮回答可展开模型实际返回的思考、工具参数、结果和执行状态；刷新、切页和审批续接保留。实时通道拥塞时用完整思考快照校正。回答头像及占位移除，正文从原头像位置开始；执行审批和Git控制移到顶部，关闭面板后右侧列宽为零。 |
| 知识库外部导入 | 文件、文件夹及拖放导入；保存原文件、完整正文、派生知识片段。支持全文查看、原文件下载、正文检索、整份移除。聊天按完整导入语料检索相关摘录，审批恢复重新取得同一问题的资料。 |
| 自动发现现有Skill | 发现工作空间与用户的Codex、Claude Code、通用Agent、GitHub/VS Code及Copilot标准技能目录，以及已存在插件缓存；显示来源、数量、错误与刷新入口。发现列表与实际load_skill共用目录结果，references/scripts使用原目录，不改写外部文件。 |

## 数据与接口

新增迁移1025保存知识文档正文、原文件引用、哈希和片段关系；迁移1024的消息分支关系继续使用。导入相同文件内容返回已有文档，不复制知识片段。源文档派生的片段在记忆页只读，修改/删除API返回409并引导到源文档，避免正文与索引不一致；手动记忆仍可编辑。

```text
GET /api/knowledge/documents
POST /api/knowledge/documents {resource_id}
GET /api/knowledge/documents/:id
DELETE /api/knowledge/documents/:id
GET /api/knowledge/search?q=...
GET /api/skills/discovery
POST /api/skills/discovery
```

搜索返回`hits`，每项具有真实`document_id`、`document_name`、`content`与从零开始的正文字符偏移`start_char`，不会把正文摘录标为不对应的memory片段。文档详情包含完整`text`和实际`chunks`。导入知识独立检索排序；原有个人记忆在候选限制和排序之前排除这些派生片段，避免大知识文档挤掉个人偏好。

设置画布状态新增`expanded_group_ids`，记录六组的展开关系；消息读取新增实际`message_reasoning`和`turn_reasoning`，流新增`reasoning`及`reasoning_snapshot`，既有事件和API保留。

## 主要代码

- 消息森林与会话族：`features/connectors/messages/ConversationCanvas.tsx`、`messageForest.ts`；`db/connector_messages/overview.rs`。
- 设置关系：`features/settings/canvas/settingsGraph.ts`、`SettingsCanvasPage.tsx`；`db/settings_canvas`。
- 过程持久化：`agent/engine.rs`、`api/chat_lifecycle.rs`、`db/conversations.rs`；`features/execution/conversationProcess.ts`、`MessageProcess.tsx`、`ChatPage.tsx`。
- 知识导入：新增`api/knowledge.rs`、`db/knowledge.rs`、`db/knowledge/context.rs`、`modules/resource/document.rs`；`api/knowledge.ts`、`features/knowledge/KnowledgeLibrary.tsx`。
- 技能发现：`tools/skill/discovery.rs`、`api/skills_route.rs`；`SkillDiscoverySources.tsx`、`SkillsPage.tsx`。
- 端到端验收：新增`frontend/e2e/workbench-revision6.mjs`，默认`npm run test:e2e`指向本轮场景。

## 验证记录

| 检查 | 结果 |
|---|---|
| 前端完整测试 | 156文件，741/741通过 |
| 前端生产构建 | PASS |
| Rust单元测试 | 1112/1112通过 |
| 最后一次审批资料检索回归 | 1/1通过，含无用户提问的协议审批 |
| 架构 / 安全集成 | 5/5、10/10通过 |
| cargo fmt --check / cargo check / server构建 | PASS |
| 浏览器端到端 | 20/20通过 |
| 原生桌面工作台流程 | 20/20通过 |
| 原生窗口与浮窗 | 10项通过，包含2次真实收起圆形拖动 |
| 原生未处理JavaScript异常 | 0 |

命令：`npm --prefix frontend run test`、`npm --prefix frontend run build`、`npm --prefix frontend run test:e2e`、`cargo fmt --check`、`cargo check`、`cargo test -p yilian-backend --lib`、`cargo test -p yilian-backend --test architecture_boundaries --test security_execution`。最后一次小范围修正为协议审批没有用户检索问题时不执行知识检索；修正后重新通过安全集成及`cargo test -p yilian-backend --lib approval_grounding_reloads_actual_last_user_question_and_canonical_tail`，随后完成实际server与原生构建。

端到端实际核验文件写入及Git差异；提供方实际reasoning_content刷新后仍在；模型工具后续请求保留DeepSeek推理协议；分支只继承来源之前的祖先、不加入后续兄弟内容；三个执行会话呈现为两个会话族；实际拖动消息后绝对位置可恢复。设置展开类别显示五个模型服务子节点和曲线，分类不打开弹窗，具体参数表单可编辑。知识文件超过12000字符的正文尾部可查看、检索，并实际进入模型system上下文；删除后检索同步移除。工作空间Codex与Claude技能文件被发现且保持只读，实际load_skill将文件尾部与根目录返回给模型。

独立监控/通话圆形经Win32 SendInput拖动后窗口各移动38物理像素，客户区保持64×64逻辑像素，光标恢复核验通过。只操作测试PID与窗口。先前一次主机光标恢复检查失败未记作通过；最终重新运行全部原生检查并通过。构建保留既有未使用项和大chunk提示，无编译错误。

[验证摘要](verification/revision6-summary.json)、[原生结果](verification/revision6-native.json)、[浏览器结果](verification/revision6-browser.json)、[原生构建证据](verification/revision6-native-build.json)、[16张实际预览](revision6-previews/index.md)。前14张来自Tauri WebView2，后2张是浅色浏览器对照；截图保留原图，附来源与SHA-256。

## 使用边界

PDF使用已有文本层解析；图片扫描件没有文本时返回解析错误，不生成替代正文。单文档完整提取上限200万字符。技能发现限定已知目录与插件缓存，不扫描整块磁盘、不安装外部软件。只有厂商实际返回的思考内容会保存和显示。

审批权限、工具风险和取消机制继续由既有运行时执行。知识资料明确作为资料注入，不改变系统或用户指令。外部Skill保留原有工具审批约束。本轮验证使用隔离目录、测试凭据与受控模型HTTP服务；文件工具、Git、数据库和桌面窗口均走实际实现，不使用用户密钥。
