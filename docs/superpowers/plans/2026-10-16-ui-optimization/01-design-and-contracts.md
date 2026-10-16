# 01 设计规范、全状态与公共契约

> 本册是所有工作包的共同输入。以下参数扩展均为拟新增；当前代码未因此发生改变。A实现公共组件，B–G消费，I处理入口，Q验证行为。

## 1. 术语

| 用语 | 执行者应该怎样理解 |
|---|---|
| 设计变量 / token | CSS中统一命名的值，例如 `var(--surface-solid)`；页面引用，不复制色值 |
| 工作包 | 一个人可以独立完成和交付的一组修改，例如 B 对话 |
| 公共契约 | 组件参数、字段含义和回调规则；依赖方不能自行改名 |
| 门槛 G | 一个可检查的完成条件，不是工作时间段 |
| 夹具 | 用来复现某状态的测试数据；不能伪装成真实用户数据 |
| 主从布局 | 左侧选择一条记录，右侧显示这一条的详情 |
| 状态投影 | 将现有状态转成文字、图标和色彩，不产生新的业务事实 |

## 2. 视觉方向与固定尺度

目标是清楚、轻量、稳定。保留粉紫水波、背景图和适量毛玻璃；增加文字承载面与层级。不要给每层都增加阴影、渐变和发光。

| 项目 | 本轮固定规则 |
|---|---|
| 页面边距 | 宽窗口24px，普通窗口20px，窄窗口16px；优先与当前容器对齐 |
| 间距刻度 | 4、8、12、16、20、24、32px；卡片内默认20px，紧凑14px |
| 字体 | 保留 typography.json：正文16px，长文17px，控件14px，说明13px，辅助标签12px，页标题22px |
| 行高 | 正文1.55，长文1.65；不在全局使用 `text-xs` 缩小正文 |
| 控件高度 | 小按钮最小32px，标准36px，大按钮44px；图标按钮点击区域至少32×32px |
| 圆角 | `--radius-xs/sm/md/lg/xl` 复用现有变量；卡片12、结构16、浮层20px；不把表格每行都圆成胶囊 |
| 导航 | NavRail目标72px，单项最小56×52px；文字至少12px；52px ContextRail维持 |
| 消息阅读区 | 目标 `min(100%, 860px)`；代码/表格允许内部横向滚动；正文不超宽 |
| 状态颜色 | 使用 success/warning/danger/info 系列；同时提供文字/图标，不只靠颜色 |
| Tooltip | 最大260px，允许换行，6×10px内边距；说明13px或现有caption；不能裁切到窗口外 |
| 浮层间距 | Drawer窗口内边距8px；Modal四周最少16px；高缩放时操作区仍可到达 |

尺寸是布局目标，不允许靠遮盖内容满足目标。卡片写 `min-height`，不要将有动态文字的卡片锁死210px。长名称换行或省略后提供完整详情。

## 3. 四层表面与主题

| 层 | 用途 | 变量与处理 |
|---|---|---|
| L0 环境 | 应用背景、背景图、边缘水波 | `--bg-app`、`--shell-overlay`、`--page-wash`；正文中央不叠强光 |
| L1 结构 | 主栏、侧栏、分组外框 | `--surface`、`--divider`；最多一层结构毛玻璃，弱阴影或无阴影 |
| L2 内容 | 文字、输入、任务/资源卡 | `--surface-solid` 或 `--surface-elevated`；文字区更实，不对整个容器设 opacity |
| L3 浮层 | Modal、Drawer、菜单、Tooltip | `--surface-elevated`、`--shadow-float`、`--backdrop`；始终盖住下层内容 |

`opacity: .8` 会连文字一起变淡，不用于实现阅读托底。对背景颜色本身处理透明度。深色模式通过表面明度和边框区分，避免只加黑色阴影。

A仅调整 `CYRENE_SEMANTIC_TOKENS.light/dark` 中本轮涉及的表面、边框、阴影和动效值；不改变品牌色相、风险状态语义、存储key或旧配置归一规则。G只调用 `setMode` 和已有背景图接口。

高对比度的旧预设名称只属于兼容定义；本轮仍要检查 Windows 强制颜色模式下焦点/按钮可辨认，但不宣称新增可选主题。

## 4. 动效合同

| 场景 | 时长与曲线 | 禁止行为 |
|---|---|---|
| 颜色/边框/焦点反馈 | 150ms ease-out | `transition: all` |
| 页面内容出现 | 120ms，opacity与最多4px位移 | 路由完成前展示假数据 |
| 普通卡片hover | 150ms，最多上移2px | 表格行、任务行、画布节点跟着跳 |
| 面板进入 | 240ms `cubic-bezier(.2,.8,.2,1)` | 持续动画width导致大面积重排 |
| Modal进入/退出 | 进入180ms、退出150ms；96%到100% | 未卸载管理却声称退出动画生效 |
| 真实运行状态 | opacity轻变化；周期≥1200ms | 快速闪烁、满屏流光、成功状态持续脉冲 |
| 减少动态效果 | 保留即时状态变化，取消位移/缩放/循环动画 | 关闭动效后看不到loading或运行文字 |

参考CSS是完整声明片段，放入A负责的公共样式；本次不创建该文件：

```css
.ui-opt-interactive-card {
  transition: transform 150ms ease-out, border-color 150ms ease-out,
    box-shadow 150ms ease-out;
}
.ui-opt-interactive-card:hover { transform: translateY(-2px); }
.ui-opt-interactive-card:active { transform: translateY(0); }
.ui-opt-focus:focus-visible {
  outline: 2px solid var(--focus-ring);
  outline-offset: 3px;
}
@media (prefers-reduced-motion: reduce) {
  .ui-opt-interactive-card { transition: none; }
  .ui-opt-interactive-card:hover { transform: none; }
}
```

## 5. 所有页面必须实现的可见状态

| 状态 | 用户看见什么 | 可执行动作 | 事实来源 |
|---|---|---|---|
| 首次加载 | 贴近最终布局的骨架；标题/导航稳定 | 离开页面 | 首次请求未完成 |
| 刷新中 | 保留旧内容，刷新按钮busy | 仍可阅读；同一刷新按钮不重复触发 | 请求pending |
| 无数据 | 说明当前没有记录；提供真实创建/前往入口 | 已有创建操作 | 请求成功且数组为空 |
| 无筛选结果 | 明确“没有匹配结果”，显示清除筛选 | 清除当前搜索/过滤 | 有数据但过滤后为空 |
| 列表加载失败 | 原错误说明与现有重试动作 | 手动重试、离开 | API失败 |
| 详情加载 | 只在详情区显示加载 | 列表仍可选择 | 当前ID详情请求 |
| 保存中 | 按钮原宽、busy文案；表单值保留 | 等待完成，按既有关闭规则 | mutation未完成 |
| 保存成功 | 显示已保存，真实列表/快照同步 | 继续编辑 | API成功及当前既有刷新结果 |
| 保存失败 | 错误在表单内，输入不清空 | 修改并重试 | API失败 |
| 选中 | 持久边框/底色，语义属性可查询 | 打开/关闭对应详情 | 当前selected ID |
| 禁用 | 原因可见；键盘不会触发 | 可查看原因 | 现有业务条件 |
| 运行/等待/失败 | 状态文字、图标、对应动作 | 原有停止/审批/重试 | 业务状态，不能靠动画计时推导 |

无关页面无需人为创建业务状态。例如知识库静态说明没有保存操作，就不添加“保存中”。文档中的“全状态”指覆盖该控件实际可进入的状态。

## 6. 公共组件参数增量

下表以现有参数为基础追加。旧调用不迁移也应可编译；不新增同名第二套组件。未传新增交互参数时保持原操作语义，控件尺寸与表面按本规范统一。

| 组件 | 保留参数 | 拟新增参数与精确定义 | 消费者 |
|---|---|---|---|
| Button | 原生button属性、variant、size | `loading?: boolean=false`；`loadingLabel?: string`。loading时disabled且aria-busy，保留原文本占位宽度 | B–G |
| Panel | div属性、`padding?: boolean=true` | `density?: 'compact'|'standard'|'relaxed'='standard'`；padding=false时不注入padding。只负责容器，不自动变成按钮 | C–G |
| Badge | tone、className、children | `dot?: 'static'|'pulse'`；不传则无点。pulse只有调用方确认running时使用 | B–G |
| Input/Textarea | 原生属性、label；Input另有hint | `error?: string`，关联aria-invalid与aria-describedby；Textarea不凭空假定已有hint；不加入全局自动校验 | C/E/G |
| Modal | open/onClose/title/children/className/footer | `size?: 'sm'|'md'|'lg'='sm'`，480/640/860px上限；内容受视口限制 | C/E/G |
| Drawer | open/side/title/onClose/returnFocusRef/showHeader/children | `className?: string`；`width?: number`；`resizable?: boolean=false`；`onWidthChange?: (width: number)=>void` | B/C/E/G |
| Tooltip | content/children/side | `delayMs?: number=300`；键盘focus立即出现，pointer延迟；可读的描述关联 | A–G |
| PageHeader | title/description/actions/className | 不新增全局scroll状态；容器通过flex和sticky实现固定头部 | 全部 |

Drawer详细规则：`width`与`onWidthChange`同时提供才启用受控拖宽；初始目标420px；宽窗口有效范围360–640px，实际最大值不得超过 `window.innerWidth - 16`。不传width仍沿用原尺寸。窄窗口隐藏拖宽手柄，面板使用窗口内可用宽度。手柄提供可访问名称“调整详情面板宽度”，左右键每次16px；关闭不改变业务选择。

公共按钮布局参考（仅展示如何保留宽度，不取代原Button全部实现）：

```tsx
<button {...rest} disabled={disabled || loading} aria-busy={loading || undefined}>
  <span className="grid items-center justify-items-center">
    <span className="col-start-1 row-start-1" style={{ visibility: loading ? "hidden" : "visible" }} aria-hidden={loading || undefined}>
      {children}
    </span>
    <span className="col-start-1 row-start-1" style={{ visibility: loading ? "visible" : "hidden" }} aria-hidden={!loading}>
      {loadingLabel ?? "处理中"}
    </span>
  </span>
</button>
```

A实现时要把原variant/size/className逻辑保留；上例先展开rest再设置disabled/aria-busy，避免覆盖计算值。同一grid单元叠放两份内容，取两种文案的较大宽度，仅切换visibility。

## 7. 浮层与键盘事件次序

1. 输入法正在合成文字时，Enter/Escape先由输入法处理。
2. 若焦点位于文本输入、select、contenteditable或编辑器内，不触发导航、页面搜索和列表上下切换。
3. Escape首先关闭最上层Modal/Drawer；一次按键只关闭一层。
4. 没有模态浮层时，Escape可以收起当前聊天/画布辅助栏。
5. Escape不执行stopGeneration，不拒绝审批，也不丢弃正在编辑的内容。
6. 浮层打开后焦点进入浮层，Tab在模态浮层内循环，关闭后回到触发按钮；触发按钮已移除时回到页面标题/列表容器。

`useDialogFocusLifecycle.ts` 已有Tab约束和焦点恢复。A在此实现顶层判断，不另造第三方焦点系统。其他工作包禁止再挂一个相互竞争的全局Escape监听。

| 快捷键 | 生效范围 | 动作 |
|---|---|---|
| Ctrl+, | Windows全局，满足上述输入/浮层限制 | 打开 `/settings` |
| Alt+1…6 | 同上 | 任务、对话、搜索、工作流、工作空间、能力；顺序固定并在Tooltip说明 |
| Ctrl+B | 聊天页 | 显式切换会话侧栏；不同断点只打开当前可用入口 |
| Ctrl+J | 聊天页 | 显式切换执行侧栏 |
| `/` | 有搜索框的管理页，非编辑状态 | 聚焦当前页搜索框 |
| ↑ / ↓ | 记录列表自身获得焦点 | 在当前过滤结果中换选择；首尾停止，不循环 |
| Enter/Space | 聚焦按钮/行入口 | 执行该入口的原动作 |

Mac上的Meta兼容可以沿用现有实现，不把Windows验收扩成新增Mac发布任务。Ctrl+F不拦截。

## 8. 公共数据与跨组件契约

- DTO指API返回的数据对象。使用现有 `Task`、`Workspace`、`CapabilityDescriptor`、`MemoryRecord` 等，不为了UI复制一套不同含义的状态。
- task的`completed`、执行的`succeeded`、能力的`ready`分别归各自领域；禁止建立一个强行统一这些字符串的“全局完成枚举”。
- 展示计数从同一份当前列表计算；必须写清“当前已加载任务/当前筛选结果”，不能把limit=200的列表计数标成全库总量。
- 没有字段的数量不显示，零值只有API明确返回0时才表示零；加载中不显示0。
- 复制路径只复制真实完整字符串。界面可省略中间，但复制不能复制省略号。
- `TaskDetailPanel` 保留 `task/workspaceName/onOpenWorkspace/onChanged` 参数；C统一负责两个调用页面的接线。
- D为工作流编辑器增加可选 `selectedNodeId?: string|null`、`onSelectedNodeChange?: (id:string|null)=>void`、`highlightedNodeId?: string|null`。selectedNodeId与回调同时传入时由父级管理选择；旧独立调用保持内部状态。运行面板对应增加可选 `selectedNodeId`、`onNodeSelect?: (id:string)=>void`、`onLocateNode?: (id:string)=>void`。只有onLocateNode触发镜头/滚动定位。
- 以上D参数不传播到TaskWorld，不修改WorkflowGraphDefinition，也不把run数据回写为编辑草稿。

## 9. 响应式合同

| 条件 | 处理 |
|---|---|
| 聊天窗口宽度≥1180px | 保留full；左会话栏+中央消息+默认收起右轨道 |
| 960≤聊天宽度<1180 | 保留compact；右详情使用Drawer |
| 聊天宽度<960 | 保留narrow；左右辅助栏通过现有互斥Drawer打开 |
| 管理页实际内容宽度≥1100px | 列表/详情可双栏；参考52/48，不牺牲列表最小可读宽度 |
| 管理页实际内容宽度<1100px | 单列表+Drawer/显式详情视图，不强塞两列 |
| 资源卡片 | `repeat(auto-fit, minmax(min(100%, 300px), 1fr))`，间距20px |
| 设置 | 导航目标200px；内容最大960px；窄窗口顶部分类条，禁止双层横向滚动 |
| 125%/150%/200%缩放 | 允许减少列数；文字、保存、关闭、停止和批准按钮不得裁切 |

“窗口宽度”和“内容宽度”不是同一个值；聊天继续用现有window断点，管理页双栏按扣除导航后的容器宽度决定。

## 10. 样式与事件命名

- 新页面样式以 `.ui-opt-chat`、`.ui-opt-tasks`、`.ui-opt-workspaces`、`.ui-opt-workflows`、`.ui-opt-capabilities`、`.ui-opt-memory`、`.ui-opt-system` 等根类限定；具体归属见02。
- 不用 `.panel`、`.card`、`button`、`input` 等无范围选择器覆盖全站。
- 禁止靠 `!important` 连续追加覆盖已有 `workbench.css`。先由I指出生效声明，原所有者/集成负责人删除对应冲突声明，再增加局部样式。
- 复制、筛选、页签展开属于页面局部状态，使用现有React状态；不为其增加Zustand全局Store。
- 每次动作只调用一次现有操作；返回失败时展示真实错误，不能先显示成功后再补请求。
