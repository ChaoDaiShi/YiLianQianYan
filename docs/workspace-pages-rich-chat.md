# 富交互聊天与工作空间页面

工作空间详情的「页面」页签提供按父级分页的页面树、块编辑器、版本记录和回收站。页面数据属于当前本地资料库，并与原有工作空间 ID 关联。归档工作空间可以查看页面。

## 富交互消息

助手可以在普通 Markdown 段落之间插入单独的 `yilian-rich` 围栏。下例展示版本 1 的图表声明。

````markdown
```yilian-rich
{"schema_version":1,"blocks":[{"type":"chart","id":"sales","title":"销量","kind":"line","labels":["一月","二月"],"values":[8,12]}]}
```
````

版本 1 可显示 Markdown、代码、公式源文本、本地图片与文件、表格、图表、表单、已阅卡片、真实任务状态、资源成果和可审查的页面修改提案。无法解析的围栏保留原始 Markdown，由客户端显示。组件通过注册目录渲染声明式数据。图片、文件和任务跟随当前对话的资源和工作空间绑定。

`ag-ui` 围栏接收名为 `yilian.rich_content` 的 `CUSTOM` 事件，其 `value` 使用忆涟富内容版本 1。[AG-UI 事件规范](https://docs.ag-ui.com/concepts/events#custom)

`a2ui` 围栏接收 A2UI 1.0 和 0.9.1 中的基础目录子集，可解析文本、容器、本地资源图像和已阅按钮，并将计算结果投影为内部内容块。容器按阅读顺序展开，绑定支持 JSON Pointer。消息重建和页面保存均使用内部内容结构。外部连接器仍使用现有会话与事件传输。[A2UI 消息规范](https://a2ui.org/concepts/messages/)

规范化消息及组件状态由 `GET /api/v1/conversations/{conversation}/messages/{message}/rich-content` 读取。交互提交使用同一路径下的 `/{component}/actions`，并携带 `action`、`state_version` 和对应参数。版本冲突返回 HTTP 409。工作空间内保存的聊天成果保留提交时的内容与状态，同时可以打开原对话。

## 页面与版本

- `GET /api/v1/workspaces` 列出已有工作空间。
- `GET /api/v1/pages?workspace_id=…&parent_page_id=…&limit=…&offset=…` 分页读取子级。可使用 `q` 搜索标题和 Markdown 正文，或通过 `view` 查看收藏、最近、回收站。
- `POST /api/v1/pages` 创建根页面或子页面，可同时提交 Markdown 正文。
- `GET /api/v1/pages/{id}` 读取含内容块的页面。界面打开页面时使用 `visit=true` 登记最近访问。
- `PATCH /api/v1/pages/{id}` 更新带版本的元数据和排序位置。修改父级时检查同一工作空间及父链环路。
- `POST /api/v1/pages/{id}/blocks/patch` 接受带 `revision` 的插入、替换、排序、移除操作。页面内容可以包含 Markdown、页面、知识文档、文件及聊天成果的引用。
- `GET /api/v1/pages/{id}/revisions` 读取版本记录。使用 `POST /api/v1/pages/{id}/revisions/{revision}/restore` 按当前版本校验后恢复历史版本，恢复本身会创建新版本。
- `DELETE /api/v1/pages/{id}` 将页面及在用的子页面移入回收站。使用 `POST /api/v1/pages/{id}/restore` 恢复同一批次。
- `POST /api/v1/conversations/{conversation}/messages/{message}/save-to-page` 支持创建独立页面、创建子页面或追加到已有页面。
- `GET /api/v1/pages/{id}/markdown` 导出 Markdown。一般文本结构按正文保留，文件及聊天成果导出本地引用，不打包资源原件。

SQLite 迁移 1122 添加消息组件状态，1123 添加页面、内容块、版本和引用表。现有工作空间、对话与资源记录作为关联来源。AI 提案的目标与当前对话工作空间必须一致，提交先检查对应的块和页面版本，再从审查界面应用。

## 验收入口

`cargo test -p yilian-backend --test rich_workspace_contract` 执行受保护 API 的页面和交互契约测试。`node e2e/core-paths.mjs --rich-workspace` 在前端目录运行浏览器验收。后者启动隔离数据库及真实后端，以回环模型端点提供可重复的有效富内容。
