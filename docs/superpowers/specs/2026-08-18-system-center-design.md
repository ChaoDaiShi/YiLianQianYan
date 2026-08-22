# Phase 4B — System Center Design

## Goal

把 Monitor、Logs、Settings 三个系统页面从偏开发工具的视觉整理为成熟、克制、可读的 Desktop System Center，同时只展示当前 API、Store 和 Theme Engine 已经真实提供的数据与行为。

## Scope

- 只修改前端 Monitor、Logs、Settings 页面及其展示层测试和共享 UI 原语。
- 保留 `/system`、`/logs`、`/settings` 路由。
- 不修改 Agent Runtime、监控后端、日志后端、数据库、API Schema、SSE Parser、MCP、安全网关、审批规则或配置 Schema。
- 不进入 Phase 5，不处理 Bundle Warning、Lazy Loading 或 Runtime 重构。
- 不使用子智能体。

## Current Reality

### Monitor

`SystemPage` 使用 `getSystemInfo()` 访问 `/api/system`，当前真实字段包括主机、操作系统、内核、uptime、CPU、内存、磁盘、GPU。现有刷新周期为 3 秒，继续保留，不新增更高频轮询。健康状态来自已有 `/api/health`，只展示后端、数据库、版本和策略信息；没有真实 Agent/MCP 在线字段，因此不展示它们。

### Logs

`LogsPage` 使用 `getLogs()` 访问 `/api/logs`，真实字段为 `timestamp`、`level`、`source`、`message`。已有 level/source 筛选、2 秒刷新、暂停、自动滚动和清空行为，均保留。日志详情只显示这四个真实字段，不从 raw 数据扩展新的敏感字段。

### Settings

`SettingsPage` 使用 `getSettings()` / `updateSettings()` 读写真实 `AppConfig`，包含 model、agent、permissions、sandbox、skills、subagents、compaction，以及现有安全授权和隔离状态展示。Theme Engine 已真实支持五个主题预设、背景图、主题导入导出、重置和现有主题参数，全部保留。API 密钥继续使用密码输入、来源提示和清除动作，不显示已保存值。

## Architecture

采用增量页面重构，不建立万能 `SystemCenter.tsx`：

- 新增少量共享 System Center 展示原语，例如状态徽标、指标卡、日志行、设置行和 Section Header。
- 每个页面保持独立的业务语义和数据生命周期。
- 共享原语只负责 presentation，不重新实现 API、Store、轮询或配置保存规则。
- 优先复用现有 `PageHeader`、`Panel`、`Badge`、`Input`、`Skeleton`、`ErrorState`、`EmptyState`、`Drawer` 和 Theme Token。

## Page Designs

### Monitor

- Header 使用“系统监控”和“查看应用、服务和资源的当前运行状态”。
- 资源区仅渲染真实存在的 CPU、Memory、Disk、GPU 指标。
- 健康区调用已有 `/api/health`，区分 Healthy、Warning、Unavailable、Unknown；请求失败只显示“状态暂时无法获取”，不把未知状态伪装为系统异常。
- 技术信息保留主机、系统、内核、uptime 等真实字段；没有的字段隐藏。
- 使用内部滚动和 `min-height: 0`，确保 1200×800 / 1366×768 下健康状态不会被资源卡片完全推出视口。

### Logs

- 使用紧凑列表/表格，不使用大面积卡片。
- 顶部保留搜索、真实 level/source 筛选、刷新/暂停和清空行为；固定筛选选项只从当前真实日志数据派生，不能凭空创建来源。
- 日志行显示真实时间、level、source、message；level 只由真实 `level` 字段决定。
- 点击行打开 Drawer，展示四个真实字段；长消息使用等宽字体并允许换行。
- 复制仅复制当前真实 message；不增加日志导出，不展开不存在的 raw/stack 字段。
- 保留接近底部才自动跟随的现有滚动逻辑。

### Settings

- 保留真实配置字段，使用浅色二级导航和设置行布局，不创建重复字段。
- 分类按实际字段组织为：模型、智能体、权限与安全、沙箱、压缩、技能与子智能体、外观。空分类不显示。
- 模型页保留 API 地址、模型、Embedding、温度、最大 Token、超时和真实密钥来源/清除动作；敏感值默认隐藏。
- 权限与安全页保留真实权限模式、需确认工具、Windows 隔离状态和 GrantEditor。
- 外观页保留五个真实主题预设、背景图、现有主题参数、导入、导出和重置；不增加假的主题卡或无效动画开关。
- 当前保存机制为手动保存，保留“保存设置”行为；新增真实 dirty 状态提示和保存失败反馈时，只基于实际本地状态和 API 返回，不伪造成功。

## Accessibility

- 所有 icon-only 按钮提供 `aria-label`。
- 二级导航使用 `aria-current` 或等价 selected 语义。
- Filter、Input、Select、Checkbox 和主题按钮具有可读名称。
- 错误区域使用 `role="alert"`，详情 Drawer 支持 Escape 关闭，键盘焦点保持可见。

## Testing

新增展示契约和纯 formatter 测试，覆盖：

- Monitor 真实指标、健康状态、Unknown、刷新、空态和错误态。
- Logs 真实字段、level/source 搜索筛选、详情、长消息、空态和错误态。
- Settings 真实分类、主题预设、敏感值隐藏、保存成功/失败和不存在字段不展示。
- Frozen UI 路由与 NavRail 未修改。

验证命令：

```text
npm.cmd test -- --run
npm.cmd run build
git diff --check
```

另外启动真实 Tauri，至少检查 `/system`、`/logs`、`/settings`，优先使用 1200×800 或 1366×768 窗口；不改变用户真实 API、模型、密钥、安全或 MCP 设置。

## Explicit Non-Goals

- 不新增 Monitor 后端健康接口、Agent/MCP 在线接口或诊断 API。
- 不新增日志后端、导出协议、脱敏引擎或虚构的 stack trace。
- 不修改 Settings API、数据库 Schema、权限模型或 Theme Engine 语义。
- 不处理 Vite >500 kB warning，不做路由懒加载和代码拆包。
