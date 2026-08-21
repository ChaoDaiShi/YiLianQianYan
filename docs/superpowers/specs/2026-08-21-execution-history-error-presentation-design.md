# Execution History and Error Presentation Design

## Goal

让用户无需理解 Tool Call、JSON、调用 ID 或底层错误字段，就能从执行轨迹中直接看懂：执行了什么、结果如何、为什么失败、下一步应该做什么。同时保留完整真实数据供技术排查。

## Scope

本次仅调整前端展示与纯展示辅助函数：

- 执行轨迹历史卡片的信息层级与展开详情；
- 基于现有 `name / args / result / reason / status` 生成确定性动作摘要；
- 全局任务错误和工具失败的用户可读描述；
- 原始错误字段的技术日志展示。

不修改 SSE 事件语义、Agent 执行逻辑、Store 状态模型、API Schema、数据库结构或后端安全策略。

## Root Cause

现有历史卡片在默认层级显示 Tool 名和 `toolCallId`。调用 ID 对排障有用，但不能回答用户最关心的“刚才执行了什么”。实际持久化记录已经包含 `name`、`args`、`result`、审批状态、验证状态和时间，因此不需要新增存储字段。

现有任务错误组件把原始 `event.error` 放在技术详情中，但默认文案只有笼统的“执行过程中遇到了问题”。用户既看不到可理解的具体原因，也容易在其他失败展示中直接遇到底层错误字符串。

## Chosen Approach

采用前端派生展示，不持久化重复文案：

1. 原始记录继续作为唯一事实来源。
2. 展示辅助函数根据真实 Tool 名与参数生成动作摘要。
3. 错误辅助函数从真实错误内容提取并分类用户可读描述。
4. 技术详情保留原始字段，默认折叠。

这样历史记录重新打开时仍能生成一致摘要，也避免显示文案和原始数据发生不同步。

## Execution History Card

### Collapsed state

默认层级依次显示：

1. 动作摘要，例如：
   - `执行命令：Get-Process`
   - `读取文件：README.md`
   - `截取当前屏幕`
   - `请求网络资源：bilibili.com`
   - 信息不足时回退为 `调用 PowerShell`，不猜测目的。
2. Tool 的用户可读名称与真实耗时。
3. 执行状态和验证状态。
4. 展开指示图标。

`toolCallId` 不再出现在默认层级。

### Expanded state

整张卡片提供可访问的展开按钮，技术详情仅展示真实存在的数据：

- Tool Name；
- Tool Call ID；
- Command；
- Arguments；
- Result / Error Log；
- 审批原因与风险级别；
- 执行耗时；
- 验证状态与验证原因。

取消、拒绝或未执行的记录明确显示“该操作未执行”，不会把它描述为执行失败。

## Semantic Action Formatting

新增小型确定性 formatter，不建立庞大硬编码映射：

- 文件工具优先读取 `path / file / target`；
- Shell 工具优先读取 `command`，只取首个非空行并限制长度；
- HTTP 工具优先读取 `url`，默认只展示主机与必要路径；
- Process、Screenshot、Mouse、Keyboard 使用可观察操作名称；
- MCP 使用现有 display name formatter；
- 缺少可靠参数时仅显示“调用 {Tool}”。

formatter 不推测“优化”“修复”“完成”等不存在的意图。

## Human-Readable Error Presentation

错误展示分为两层。

### User layer

必须用普通用户可以直接理解的中文说明，并在能够可靠判断时给出下一步：

- connection refused / failed to fetch：`无法连接到服务，请确认后端或目标应用正在运行后重试。`
- timeout：`操作等待超时，目标服务可能响应较慢，请稍后重试。`
- unauthorized / 401：`身份验证失败，请检查对应服务的账号或密钥设置。`
- forbidden / access denied / 403：`当前权限不足，无法完成这个操作。`
- not found / 404：`没有找到需要访问的资源，请检查路径或地址。`
- cancelled / aborted：`任务已停止，没有继续执行后续操作。`
- 其他结构化错误：提取 `message / error / detail / reason` 中最具体的文本，去掉字段包装后展示。
- 无法安全提取时：`任务执行时遇到了未能识别的问题，请查看错误日志。`

用户层不得展示 JSON、堆栈、内部类型名或整段 stderr。

### Error log layer

默认折叠的“查看错误日志”中展示真实存在的：

- error code / HTTP status；
- raw message；
- stack；
- stderr；
- raw result。

前端不伪造日志。Agent 终止错误继续由现有后端日志缓冲记录，UI 技术层仅呈现已收到的原始错误内容。

## Storage

不新增数据库字段，不存储派生动作标题或翻译后的错误文案。现有执行历史字段保持原样，以免文案升级后旧历史无法获得新展示效果。

历史水合继续兼容旧记录。缺少参数、时间或结果时，UI 隐藏对应详情并使用保守回退文案。

## Accessibility and Interaction

- 历史卡片展开按钮使用 `aria-expanded` 和 `aria-controls`；
- 展开状态有 Chevron 图标，但不只依赖图标表达；
- 状态仍由文本和颜色共同表达；
- 技术长文本允许换行和横向滚动；
- 键盘焦点保持可见；
- 不新增持续动画。

## Testing

测试优先覆盖用户可见行为：

- Shell、文件、网络、截图和未知 Tool 的动作摘要；
- 摘要不显示调用 ID；
- 历史详情默认折叠，可展开查看真实字段；
- 取消、拒绝、成功和失败状态文案；
- JSON 错误字段提取；
- 网络、超时、认证、权限、资源不存在错误的用户中文描述；
- 原始错误只出现在错误日志区域；
- 旧执行历史水合仍然可用。

完成后运行聚焦测试、完整 `npm test` 和 `npm run build`。
