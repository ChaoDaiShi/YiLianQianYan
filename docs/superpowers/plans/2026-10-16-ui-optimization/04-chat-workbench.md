# 04 工作包 B：对话工作台与执行反馈

**目标：** 让用户能持续阅读、明确发送/停止、找到执行和审批信息，并保留已有附件与语音能力。

**输入：** `ChatPage`、`ChatView`、`ChatInput`、消息/执行展示、现有会话数据。**输出：** 对话UI增量和GB证据。

**可改：** 02中B的精确文件范围。拟新增 `frontend/src/styles/ui-optimization/chat.css`、`frontend/e2e/ui-optimization/b-chat.mjs`。

**只读：** 聊天runtime、SSE解析、`api/chat.ts`、语音runtime、执行reducer、审批API和共享toolDisplay/taskTitle。公共Drawer由A改。

## B1 三栏尺度与显式展开

- [ ] 在 `ChatPage.tsx` 保留当前full/compact/narrow判定和右栏初始收起状态。
- [ ] 根容器增加 `.ui-opt-chat`；中央列必须 `min-width:0; min-height:0`，外层不滚动，消息列表自己滚动。
- [ ] full下会话栏目标240–260px，右收起轨道52px；右展开面板目标360px，可在空间足够时调整到460px。
- [ ] 中央内容最大860px，宽度不足时收缩；输入与正文左右边界对齐。
- [ ] compact右侧使用现有Drawer，narrow两侧用现有互斥Drawer；每种布局只显示一个同义开关。
- [ ] 新增Ctrl+B/Ctrl+J时复用现有开关handler；输入法/文本框/浮层内不触发。不要再挂Esc停止生成。
- [ ] 不实现边缘hover自动开栏。点击后面板稳定存在，直到用户显式收起。

布局CSS参考：

```css
.ui-opt-chat .conversation-message-list-inner,
.ui-opt-chat .conversation-composer-inner {
  width: min(100%, 860px);
  margin-inline: auto;
  min-width: 0;
}
.ui-opt-chat .conversation-message-user { max-width: min(75%, 720px); }
@media (max-width: 959px) {
  .ui-opt-chat .conversation-message-user { max-width: 92%; }
}
```

两个inner类是拟添加到MessageList/ChatInput实际内容容器的类；先确认元素位置再加，不能在额外套层后改变已有滚动容器。

验收B-01：959/960/1179/1180边界布局准确；B-02：展开/收起焦点返回正确，中央宽度与滚动位置保持。

## B2 会话历史与消息阅读

- [ ] `ConversationSidebar` 保留新建、搜索、切换、删除；列表按已有created/updated字段组织“今天/近7天/更早”，字段不具备时保持当前顺序。
- [ ] 不新增置顶/改名按钮：当前 `api/conversations.ts` 仅有列表、创建、加载、删除。持久化支持不在本轮。
- [ ] 操作图标在hover和focus-within都可见；无精细指针场景直接显示。列表行内删除按钮不嵌套到另一个button内。
- [ ] 用户消息右对齐，AI正文使用实一些的承载面和清楚行距；保留现有头像开关，不强制每条消息重复头像。
- [ ] 代码和表格只在自身内部横向滚动；不让整个聊天页出现横向滚动。
- [ ] 复制按钮保留完整原始文本。显示“已复制”必须发生在clipboard写入成功之后，失败时可见“复制失败”。
- [ ] 现有TTS按钮、外链打开与附件展示保留；不把整个消息替换成纯HTML或额外Markdown实现。

验收B-03：长中文、长路径、代码块、Markdown表格各一条不裁切；B-04：键盘可到达删除和复制，动作只作用于当前记录。

## B3 输入舱与发送/停止

- [ ] 读取 `ChatInput` 当前 `canSend`、AttachmentQueue和onSend返回值语义，不重写发送判定。
- [ ] 文本区最小44px，最大180px沿用当前高度上限；超出内部滚动，底部动作固定可见。
- [ ] 输入舱底边目标16px留白；页面空间不足时减少装饰留白，不盖住最新消息或审批按钮。
- [ ] sending与isLoading分别显示“正在提交”和“正在生成/停止”；停止仍调用已有onStop，不关闭会话或清空草稿。
- [ ] 中文输入法组合状态下Enter不发送；Shift+Enter换行，Enter按原规则发送。
- [ ] 附件未准备完成时显示既有原因，发送失败保留输入和已上传附件；不得为了美观重置队列。
- [ ] 全局语音输入入口保留；确保悬浮语音控件不覆盖发送/停止。
- [ ] 不增加无合同的 `/` 技能执行、`@`文件查询、深度思考开关和模型/空间伪选择器。现有能力入口可以重新排布。

| 输入状态 | 发送区 | 本轮应保留的内容 |
|---|---|---|
| 空文本、无附件 | 禁用发送 | 草稿空态提示 |
| 附件上传/预览未完成 | 不可发送，原因可读 | 附件队列及移除操作 |
| 可发送 | 主发送按钮 | 文本和ready资源ID |
| onSend未完成 | 提交中，不能重复点 | 原输入直到确认接受 |
| 正在生成 | 停止按钮 | 当前输出与历史记录 |
| 停止后 | 恢复发送能力 | 已收到的输出，不伪造完成 |

验收B-05：输入法/换行/单次提交；B-06：附件失败及发送失败后原内容仍在；B-07：语音组件不遮挡主动作。

## B4 滚动与执行摘要

- [ ] 保留 `isNearBottom` 的48px语义；收到token前先看用户是否在底部。
- [ ] 用户向上翻阅后不再自动滚到底。出现“回到最新”按钮；点击后滚底并恢复跟随。
- [ ] 新输出到来不重建全部MessageBubble、不改变消息key；持续流式内容不重复播放进入动画。
- [ ] 执行过程使用现有CurrentActionCard/AgentProgressCard/ToolCallCard字段；摘要只说“正在调用工具”等可证事实。
- [ ] 工具参数卡收起为摘要后仍可展开；跨栏高亮按现有execution/tool-call标识匹配，无可对应ID时不提供联动按钮。
- [ ] 不生成“思考了3.2秒”或模型内部推理正文；没有开始/结束时间就不显示耗时。
- [ ] 展开/收起工具卡不会取消阅读位置或把页面拉回顶部。

验收B-08：流式中上翻三屏后新增内容不抢滚动；B-09：工具卡展开和执行栏显示同一条真实记录。

## B5 审批卡与执行轨迹

- [ ] 只重排 `ApprovalCard` 展示区：工具名称、风险、目标路径/命令、参数和两个现有决定动作清晰可见。
- [ ] 保留maskArgs及原语音审批展示证明流程；样式不能把卡片做成透明但仍被判定可见的隐藏占位。
- [ ] 维持onApprove/onReject现有参数和调用次数；不加“本会话始终允许”或四档授权。
- [ ] resolving时禁用重复决定，保持宽度；拒绝后状态跟随现有事件，不直接把任务标成功。
- [ ] 运行、成功、执行失败、验证失败分别保留。执行返回成功但verification失败的记录必须仍呈现失败说明。
- [ ] 在200%缩放和窄Drawer中，命令可查看完整内容，批准/拒绝按钮无需横向滚动即可触达。

验收B-10：待审批→决定中→已处理，状态与真实回调一致；B-11：执行成功/验证失败不被统一成绿色。

## GB交付与验证

```powershell
Set-Location 'F:\项目开发\忆涟千言\YiLianQianYan\frontend'
npm.cmd run test -- src/components/chat src/features/execution src/components/approval src/components/layout/workspaceLayout.test.ts
npm.cmd run build
```

相关已有行为检查优先复用 `e2e/chat-reliability.mjs`、`chat-view-race.mjs` 的场景，由Q/I接入既有runner；不要把导出函数模块当命令直接运行。

交付必须包含：B-01至B-11结果、浅/深色消息图、窄窗口输入与审批图、流式滚动记录。没有真实模型也可以用明确标记的SSE测试夹具验证前端；真实模型链路另列，不把夹具称为真实模型成功。
