# 03 工作包 A：公共组件与应用外壳

**目标：** 让所有页面使用同一套可读表面、控件尺寸、浮层生命周期和导航反馈。

**输入：** 00源码基线、01公共契约、02所有权、现有Cyrene语义变量。**输出：** 向后兼容的公共组件和GA证据。

**依赖：** G0；可与B–G的页面结构同时开发。禁止修改API、Store、任务状态或其他工作包页面。

**文件：** 02中A的现有文件；拟新增 `frontend/src/styles/ui-optimization/foundation.css`、`frontend/src/components/layout/useUiNavigationShortcuts.ts`。`index.css`由I接入。

## A1 主题表面与可读性

独立成果：浅/深两种模式下，结构、正文和浮层有明显层级，背景图不冲淡正文。

- [ ] 读取 `theme/applyTheme.ts` 的 `buildThemeVariables`，确认页面最终使用的是 `CYRENE_SEMANTIC_TOKENS`。
- [ ] 列出聊天消息、输入框、Panel、Modal各自的当前背景变量；不要修改 `PRESETS` 中没有运行到的旧主题来冒充完成。
- [ ] 在 `presets.ts` 的light/dark映射中只调整承载面、边框与阴影，记录修改前后值；文字/品牌色保持当前语义。
- [ ] 为L1/L2/L3使用01规定的变量；将高模糊限制在结构外框或浮层，正文不再叠一层模糊。
- [ ] 检查无背景图、明亮图片、暗色图片三种情况下的标题、正文、输入边界和错误文本。
- [ ] 检查图文、代码、审批命令都使用不影响文字opacity的背景托底。

验收：A-01，两种模式均能辨认主文字、次文字、焦点和危险操作；不能仅比较色值表。

## A2 控件状态与稳定尺寸

独立成果：Button、Input、Textarea、Badge、Panel可用且原调用兼容。

- [ ] 给Button追加loading/loadingLabel；原variant/size/原生属性保留。busy期间同一个按钮不能再次触发。
- [ ] 使用两份文字占据同一grid单元、visibility切换的方式保持宽度：长文案决定宽度，隐藏文案设置aria-hidden；按钮提供正确可访问名称。
- [ ] 统一sm/md/lg最小高度32/36/44px；不要截断多行按钮说明。
- [ ] Panel增加density，只映射14/20/24px；padding=false时三档都不生效。
- [ ] Input/Textarea的error文本放在原hint位置附近，以稳定id关联；已有label/hint关系保留。只消费调用方错误，不发起第二次校验。
- [ ] Badge加可选dot；默认无dot。业务状态由页面传入，不从绿色自动推导“成功”。
- [ ] 在已有真实页面分别验证默认、hover、focus-visible、active、disabled、loading与error；无需为纯圆角修改写快照测试。

Button内容布局参考：

```tsx
<span className="grid items-center justify-items-center">
  <span className="col-start-1 row-start-1" style={{ visibility: loading ? "hidden" : "visible" }} aria-hidden={loading || undefined}>
    {children}
  </span>
  <span className="col-start-1 row-start-1" style={{ visibility: loading ? "visible" : "hidden" }} aria-hidden={!loading}>
    {loadingLabel ?? "处理中"}
  </span>
</span>
```

此片段中loading、loadingLabel、children来自原Button参数；spinner可放在busy文本前，减少动态效果时保留静态busy图标和文字。

验收：A-02，点击保存进入loading前后宽度差≤1 CSS px，回调触发一次；A-03，键盘焦点和错误说明可辨认。

## A3 Modal、Drawer与Tooltip

独立成果：浮层开关平滑，键盘和焦点不会穿透到后面。

- [ ] 保留 `useDialogFocusLifecycle`；在其中建立只用于现有浮层的顶层顺序。按Esc仅当前顶层处理，事件置defaultPrevented。
- [ ] 浮层生命周期使用 `closed → opening → open → closing → closed`。关闭时先取消交互并播退出动画，结束再卸载；减少动态效果时直接到closed。
- [ ] 焦点约束与“浮层仍然可见”保持同一生命周期；关闭动画期间不能把Tab放到被遮住的页面。
- [ ] Modal实现sm/md/lg，标题、内容、footer分开；`max-height: calc(100dvh - 32px)`，内容 `min-height:0; overflow:auto`，页脚不被推出视口。
- [ ] Drawer实现01中的可选宽度和调整回调；pointerdown捕获指针，move更新宽度，up结束。拖动期间只禁选文字，不影响页面持久状态。
- [ ] 手柄提供 `role="separator"`、`aria-orientation="vertical"`、可聚焦与当前宽度值；左右键以16px改变，不超过边界。
- [ ] Tooltip保留焦点触发，鼠标延迟300ms；最大260px，窗口边缘换位或约束，滚动时跟随触发元素或关闭，不停留在旧坐标。
- [ ] Escape关闭最上层后，确认底层Modal仍开，触发按钮重新可操作；关闭Drawer不能顺带停止对话。

契约事件表：

| 输入 | 当前状态 | 下一状态/结果 |
|---|---|---|
| open=true | closed | mounting/opening，进入焦点 |
| 动画结束 | opening | open |
| 用户关闭 | open | onClose一次；closing；业务open变false |
| 动画结束 | closing | 卸载，焦点回触发入口 |
| open重新true | closing | 取消卸载并回opening，不使用旧timer删除新浮层 |
| Esc | 存在两层浮层 | 只关闭最上层 |

这里的生命周期处理直接服务已承诺的退出动画，不引入通用弹窗平台或全局工作流。

验收：A-04嵌套浮层；A-05宽度和键盘；A-06 Tooltip越界；A-07减少动态效果。

## A4 外壳与导航

独立成果：导航可辨认、当前页面明显、连接反馈真实，主内容不被导航挤出。

- [ ] 在NavRail保留 `NAV_GROUPS` 的15项真实入口和三分组；目标宽72px、标签≥12px，720px高窗口允许导航项区域自身滚动。
- [ ] 长“工作空间”标签完整呈现或提供名称Tooltip，不删到难以辨认的单字。
- [ ] 当前路由使用aria-current加左侧短线；hover变色但不改变点击位置；按下效果不移动整条导航。
- [ ] 读取现有healthCheck结果显示服务可用/不可用/检查中。仅有数据库状态时就只显示数据库，不推导SSE或MCP连接正常。
- [ ] “刷新连接状态”调用现有healthCheck一次；“查看日志”跳 `/logs`。不新增后台自动重连和心跳服务。
- [ ] 采用01快捷键，hook只处理当前页面未消费、无输入焦点且无模态浮层的事件。
- [ ] 核对Tauri现有拖拽区域；如当前窗口有原生标题栏，不再加16px空白拖拽带。交互按钮不落在data-tauri-drag-region内。

新增hook合同：`useUiNavigationShortcuts(): void`，内部使用现有router navigate，仅由AppShell调用一次；离开外壳时卸载监听。

验收：A-08所有导航路径和快捷键；A-09窄高窗口导航滚动；原生拖拽单列为桌面证据。

## A5 公共加载、空态与头部

- [ ] PageHeader由页面外壳固定高度或sticky，禁止新增window.scrollY联动所有页面。
- [ ] Skeleton尺寸对应最终行/卡片；刷新已有列表时不清空整屏重放骨架。
- [ ] EmptyState使用一个标题、一句说明和最多一个主要动作；静态空态不持续闪烁。
- [ ] ErrorState保留真实错误和手动重试，不自动循环调用；仅在已有可展示诊断文本时加details。
- [ ] 路由加载区域保持标题/外壳位置，不展示另一个页面的标题作为通用占位。

验收：A-10加载/空/错状态几何和文案；不同页面不会同时出现两个主操作按钮。

## GA交付检查

```powershell
Set-Location 'F:\项目开发\忆涟千言\YiLianQianYan\frontend'
npm.cmd run test -- src/theme src/components/ui src/components/layout
npm.cmd run build
```

预期退出码均为0；实际测试数量以当次输出为准。构建不能证明焦点、退出动画或拖宽成功，还需Q的A-01至A-10记录。

交付给B–G：新增参数说明、已验证示例页面、尚未通过的组件。未过A3时可以交付A2，不强制其他工作线停工。
