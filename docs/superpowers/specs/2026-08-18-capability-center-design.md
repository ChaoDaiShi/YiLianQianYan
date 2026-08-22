# Phase 4A Capability Center Design

## Goal

在不改变后端业务模型、API、Store 或运行时语义的前提下，统一技能、插件、智能体和能力四个页面的 UI 语言、信息层级、状态表达、详情结构和错误处理，让用户能够可信地理解当前系统拥有的能力及其状态。

## Scope

本设计只覆盖：

- Skills 页面
- Plugins 页面
- Agents 页面
- Capabilities 页面
- 少量共享展示原语、样式和真实性测试

本设计不覆盖：

- Agent Runtime、Skill Runtime、Plugin Runtime
- MCP 协议、Tool Runtime、Security Gateway
- Permission / Approval 后端语义
- Agent 数据模型、Database Schema、API Schema、SSE Parser
- Workflow、Memory、Knowledge 后端
- Marketplace、Plugin Store、Voice、Live2D、Bundle Optimization

## Data Boundaries

四个页面继续使用独立业务模型，不建立第二套 Capability 数据层，也不把四类实体转换成万能对象。

### Skills

使用现有 `listSkills` 和 `loadSkill`。当前真实字段为 `name`、`description`、`path`，以及加载后的 Skill 内容。页面不虚构版本、启用状态、来源、配置或测试动作。

### Plugins

插件页面继续忠实表示当前 MCP Server 与内置工具。保留现有 MCP 创建、编辑、删除、启停、测试、刷新及 Tools / Resources / Prompts 查看能力。使用真实的 transport、runtime status、protocol version、计数、时间和安全错误字段。环境变量只显示数量，不显示值。

### Agents

使用现有 Agent 与 Team API。Agent 展示真实的 `id`、`name`、`description`、`instructions`、`allowed_tools`、`model`、`capabilities`、`max_iterations`、`enabled`、`source`。保留已有创建能力，不新增没有后端支持的编辑、删除、启停或在线状态。

### Capabilities

使用现有 Capability Registry API。展示 `id`、`kind`、`provider`、`name`、`description`、`risk`、`permissions`、`status`、`enabled` 和真实 metadata。支持现有搜索、kind、provider、status 筛选、刷新和详情查询。页面保持只读，不添加执行或 Toggle。

## Shared UI

新增少量共享展示原语到 `frontend/src/components/capabilities/`：

- `CapabilityStatusBadge`：统一真实状态到中文文案、颜色和可访问文本的映射。
- `CapabilityMetaRow`：统一展示来源、类型、版本、路径等真实字段。
- `CapabilityDetailSection`：统一详情分区和技术详情折叠。
- `CapabilitySearchBar`：统一搜索输入和筛选控件样式。

共享组件只处理展示，不包含 Skill、Plugin、Agent 或 Capability 的业务规则。

## Page Layouts

- Skills：保留左侧列表 + 右侧内容详情，统一 Header、Search、Loading、Error 和 Empty。
- Plugins：保留 MCP 管理和运行时详情，改用统一 Surface、Card、Badge 和技术详情层级。
- Agents：保留 Agent / Team 分区，Agent 使用紧凑 2～3 列 Card，并提供真实字段详情。
- Capabilities：使用高信息密度列表 + 详情面板，保留只读语义。

响应式布局使用大屏 3 列、中屏 2 列、小屏 1 列；详情区域在小窗口自然下移。列表区域使用 `min-height: 0` 和内部滚动。不修改冻结的 NavRail 宽度、分组、Logo 或 Active 布局。

## Visual Language

能力页整体保持约 90% 生产力、10% Cyrene Ripple：

- 复用现有 Theme Token，不建立第二套 Theme。
- 使用粉紫 Accent、涟漪紫、水蓝、月光白 Surface 和夜空紫 NavRail。
- 只保留小型淡紫光斑、选中 Accent 和少量 `✦`。
- 不加入角色图片、动态背景、大范围 Glow 或新大型 Asset。

## Interaction and Accessibility

- 搜索只匹配真实字段；筛选只使用真实 API 字段。
- 用户层显示易理解字段，ID、Raw Type、Raw JSON、Transport、Schema 等放入真实存在时的“技术详情”。
- Icon-only Button 提供 `aria-label`；选中态使用 `aria-selected` 或 `aria-pressed`。
- Card / Row 支持 Enter、Space；Dialog 支持 Escape；状态不依赖颜色单独表达。
- 操作失败保持真实错误，不使用无回滚的 optimistic UI。
- API Key、Token、Secret、Password、Cookie 等敏感值继续隐藏。

## Loading, Empty, and Error

复用共享 Skeleton、EmptyState 和 ErrorState。ErrorState 的重试必须调用真实 reload。

- Skill：`这里还没有技能`。
- Plugin：`还没有可用插件`。
- Agent：`还没有其他智能体`，只有真实 Create API 存在时显示创建入口。
- Capability：`暂时没有可显示的能力信息。`。

不展示虚构的版本、健康状态、调用次数、权限、推荐、Marketplace 或安装功能。

## Testing and Verification

新增 presentation helper 与真实性 Contract 测试，覆盖：

- Skill 列表、搜索、详情、空态和错误态。
- Plugin 状态、详情、操作入口、敏感字段隐藏、空态和错误态。
- Agent 列表、创建、详情字段和不伪造在线状态。
- Capability 搜索、筛选、详情、权限展示和只读行为。
- 共享状态 Formatter 与 UI Freeze 回归。

交付前运行：

```text
npm.cmd test
npm.cmd run build
git diff --check
```

并启动真实 Tauri，检查 Skills、Plugins、Agents、Capabilities 路由及至少一个 1366×768 或 1200×800 窗口；同时检查后端健康状态。既有约 583.12 kB 的 Vite 单 chunk warning 不在本阶段处理。
