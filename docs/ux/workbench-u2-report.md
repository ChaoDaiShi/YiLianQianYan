# Workbench UX U2 验收报告

日期：2026-09-27。范围：v1 前端体验与呈现层。

## 交付结论

**U2 前端体验验收 PASS；Canvas Stability 与 Studio C2 回归 PASS。**

本轮采用真实后端与 system Edge 验证。Chat 外部模型是明确标注的本地
OpenAI 兼容协议夹具，不是云端模型验收。一次瞬时回复暴露的既有 Chat
初始化竞态保留为限制，没有改运行时来扩大本轮范围，也没有声称它已修复。

- 基线：`cd5100eb153ce0a2415b5db5215581b6f018f895`，`feat/v1-canvas-studio-c2`。
- 分支：`feat/v1-workbench-ux-u2`。
- 独立工作树：`F:\项目开发\忆涟千言\YiLian-v1-workbench-u2`。
- 代码与 E2E 提交：`6ff6551fbfec58f2e8956b7297ad5c1fb3e8b60d`。
- 最终文档提交、远端 SHA 和工作树状态由 ignored `target/workbench-u2/manifest.json`
  与 `verification/push.json` 记录，避免文档自身 SHA 循环引用。
- 未使用子智能体。未合并、未修改 main/develop/v1 release/v2。

## 改造结果与设计判断

Settings 原有默认模型、Embedding、多模型档案、用量和高级参数具有接近的
视觉权重。现在首层为模型服务、写入式密钥、模型名称、连接状态和保存/测试
操作。DeepSeek、OpenAI、OpenAI Compatible、Custom 仅填已有配置字段；
自定义 URL 和 BYOK 自由度保留。高级采样、URL、环境变量和 Embedding
进入折叠区，多模型档案/模型树/用量仍可展开访问。

现有 Provider 测试接口验证当前运行配置，活动模型档案可以覆盖默认模型。
因此采用任务书允许的独立“保存设置 → 测试连接”，并显示活动档案的优先级。
存在未保存修改时禁用测试，修改/保存后清除旧测试结果，避免把保存误报成可用。
状态明确区分未配置、已保存但未测试、连接正常与当前不可用。

语音是可选配置。STT/TTS 的地址、环境变量、超时进入各自高级区；保留当前
模型、音色、语言、凭据操作和实际状态。没有发明新的启用标志。

Chat 侧栏和普通会话行降低边框与阴影，选中项保留品牌色。助手内容无重卡片
背景，输入区使用实色、12px 圆角、轻阴影和已有主题边框。工具详情继续按需
展开，原有名称/状态/耗时/结果摘要保持。真实 Artifact 增加图标和统一卡片层级。
同时修正旧会话样式优先级对 U2 的覆盖，使已打开会话也采用这套样式。

Capability 首屏先回答“忆涟现在会什么”，展示真实名称、描述、来源、状态与
权限摘要。来源导入管理折叠到次级区域，详情展示真实 source/version/hash，
没有数据时不虚构状态、更新或来源。托管列表复用 EmptyState/Skeleton/ErrorState。
Memory、Knowledge、System、Navigation、Dialog 只做表面层级统一；数据与行为保留。

Global Voice 只调整表面、边框、阴影与圆角。展开面板在 Chat 中通过 CSS
预留空间，防止覆盖输入区。没有修改语音 TS、generation/epoch/barge-in/
approval/echo/media release，也没有修改 Canvas 的 safe-area 模型。

共享 Input 使用 React useId，修正默认模型和档案表单同名字段产生重复 ID 的
可访问性问题。预设实现归属 llm 模块，Settings 通过其公开入口组合，避免新增双向依赖。

## 自动化验证

| 检查 | 实际结果 |
|---|---|
| 前端完整测试 | 99 个文件、476 个测试全部通过，0 failed |
| Architecture R2 前端检查 | 完整测试中 5 项边界检查通过；另作依赖人工复核 |
| 最后样式定点检查 | conversationThemeVisual 4/4，通过 |
| 生产构建 | TypeScript + Vite 通过；保留主包 500 kB 阈值警告 |
| Core real-backend E2E | PASS，真实创建 3 个图，Settings 三档可达与语音避让通过 |
| Canvas Stability E2E | PASS，12 个快照，真实 stale 409 一次，执行 succeeded |
| Canvas Studio C2 E2E | PASS，四档窗口、面板交互、保存、真实执行、夜间与语音避让 |
| Workbench U2 E2E | PASS，16 个页面/尺寸记录，完整 Settings/Chat/Capability 流程及深色截图 |
| 保护路径审查 | 0 Backend/Rust/Cargo/schema/migration/API/Canvas/voice runtime/dependency 变更 |

完整测试在模块归属修正后执行。最后的 Chat CSS 修正追加定点视觉检查、生产
构建和浏览器计算样式断言；没有为样式反复运行无关后端检查。未运行 Cargo。

复现命令（从此工作树执行；E2E 需已有 `target/debug/yilian-server.exe`）：

```powershell
npm.cmd --prefix frontend test -- --maxWorkers=2 --minWorkers=1
npm.cmd --prefix frontend run build
npm.cmd --prefix frontend run test:e2e
$env:YILIAN_E2E_STUDIO='after'
npm.cmd --prefix frontend run test:e2e
Remove-Item Env:YILIAN_E2E_STUDIO
$env:YILIAN_E2E_WORKBENCH='after'
npm.cmd --prefix frontend run test:e2e
Remove-Item Env:YILIAN_E2E_WORKBENCH
```

E2E 启动自己的临时数据库/工作区和端口，使用 system Edge
`154.0.4258.37`。复用后端二进制 SHA-256：
`AFF6732E1EE3595BA1C6A0BF3B1AFF11280B67241ADB48734594A39482F333C5`。

## Secret、状态与真实交互证据

Windows 用户的 OS SecretStore 不随临时数据库隔离，因此测试不替换或清除
默认用户密钥。使用服务器生成 UUID 的一次性模型档案与专属凭据槽，逐项验证：

1. GET 模型列表和 Settings 不返回明文。
2. 省略 key 的真实更新保留 key，随后 HTTP 验证仍以原值认证。
3. UI 普通参数编辑发送 empty key，保存后仍以原值认证。
4. UI 输入新的非空值后，HTTP 验证使用新值。
5. 显式确认清除只作用于该一次性档案，GET 返回 configured=false。
6. 最后删除一次性档案并复查不存在，避免遗留凭据。

Provider 验证覆盖成功、主动拒绝和恢复，拒绝产生的一次 400 与浏览器诊断
精确匹配，其余浏览器错误不豁免。Chat 经真实 SSE 接收标注的 fixture 文本，
刷新并重新打开会话后仍可读取；文件、语音和发送可达，展开语音不覆盖输入区。
Capability 使用已知的本地惰性 Markdown 声明验证预览、安装、启用、发现、详情、
禁用，没有安装或执行未知远端代码。

## 响应式与前后对比

Chat / Settings / Capability / Canvas 覆盖 1280×720、1366×768、1920×1080、
2560×1440；System 额外覆盖同样四档。未发现非预期横向溢出。
深色覆盖 Chat / Settings / Capability / Canvas，并复核实际截图的文字、控件和分区。

| 指标 | C2 基线 | U2 |
|---|---:|---:|
| Settings 默认展开的 input 数量（不计下拉选择） | 20 | 2 |
| Settings 折叠结构总高度，1280/1920 | 2522.55 px | 789.22 px |
| Settings 1920 表单宽度 | 1180 px | 1180 px |
| Chat 输入区 backdrop-filter | blur(12px) saturate(1.22) | none |
| Canvas 顶栏 | 60 px | 60 px |
| Canvas 节点尺寸 | 240×128 px | 240×128 px |

可编辑功能没有删除。2 个基础 input 是密钥与模型名称，另有模型服务下拉框；
高级区和多模型区展开后保留其他字段。

与最终 C2 证据逐项比较，四档 Canvas 的 canvas/header/toolbar/camera/nodes/panels
几何记录全部相等。TaskWorldPage 的独立生产 CSS 字节和 SHA-256 完全相同；
全局生产 CSS 中 Canvas 相关规则也保持相同。全局变化限定在 U2 表面和允许的
细小语音/Nav 一致性调整。

证据位于 ignored `target/workbench-u2/`：

- `comparison.html`：可切换页面、1280/1920 和并排/纵向布局的截图对比。
- `before-after.json`：前后几何、计算样式、可见字段和 C2 几何对比。
- `manifest.json`：来源、SHA、结果文件、49 张正式截图的哈希索引。
- `verification/`：完整测试 JSON/log、最后样式测试、构建 log、built-CSS diff、push 结果。
- `regression/core-and-canvas-stability/`、`regression/canvas-studio-c2/`：最终回归原始证据副本。
- Before：`2026-09-27T04-21-28-132Z/`。从精确 C2 提交导出源码，等待 System 实际指标；原工作树不变。
- After：`2026-09-27T04-26-50-814Z/`。四档布局、三页深色与真实交互截图。

较早的加载中截图、修复前失败证据同样保留，未覆盖成“成功”。

## 保留的限制与未完成项

- **未修复：瞬时首条回复的 Chat 初始化竞态。** 失败证据位于
  `2026-09-27T04-23-25-981Z/`。本地夹具已发送 SSE，但新会话 UI 未即时展示助手回复；
  现有 ChatView 初次加载历史与流结束可能竞争。本轮没有修改该源码或 API。
  最终视觉 E2E 让外部模型夹具在会话历史加载完成后释放首 token，明确限制其验证范围；
  不将此结果解读为该竞态已修复。
- **未验证真实云端 Provider 和真实麦克风/STT/TTS。** 使用本地协议夹具、不调用收费模型；
  语音仅验证入口、展开及布局。未使用用户真实密钥进行替换或清除。
- 保存与测试保留两个按钮，原因是当前活动档案优先级和已有接口语义，不能假装验证未保存草稿。
- Vite 主入口仍为约 520.81 kB，超过 500 kB 提示阈值；构建成功，本轮未做打包架构调整。
- 浏览器执行为 system Edge，不等同于独立 Windows 设备/Tauri 安装包验收。
- 本轮没有后端改动需求；没有推进 Generic MCP Canvas、GIS、v1.1/v1.2/v2 或其他周期。

## 文件清单

以下为相对本工作树根目录的全部受版本管理变更；target 证据不提交。

- `docs/ux/workbench-u2-design.md`
- `frontend/e2e/core-paths.mjs`
- `frontend/e2e/workbench-flows.mjs`
- `frontend/e2e/workbench-u2.mjs`
- `frontend/src/components/capabilities/capabilityPresentation.test.ts`
- `frontend/src/components/capabilities/capabilityPresentation.ts`
- `frontend/src/components/ui/Input.test.tsx`
- `frontend/src/components/ui/Input.tsx`
- `frontend/src/features/capabilities/ManagedImports.tsx`
- `frontend/src/features/llm/ModelManagerPanel.tsx`
- `frontend/src/features/llm/ModelPresetSelect.tsx`
- `frontend/src/features/llm/index.ts`
- `frontend/src/features/llm/modelPresets.test.ts`
- `frontend/src/features/llm/modelPresets.ts`
- `frontend/src/features/settings/model/workbench.test.ts`
- `frontend/src/features/settings/model/workbench.ts`
- `frontend/src/features/settings/sections/ModelSettings.tsx`
- `frontend/src/features/settings/sections/ProviderReadinessCard.tsx`
- `frontend/src/features/settings/sections/VoiceSettings.tsx`
- `frontend/src/features/tasks/ArtifactResultsPanel.tsx`
- `frontend/src/features/voice/globalVoice.css`
- `frontend/src/index.css`
- `frontend/src/pages/CapabilitiesPage.tsx`
- `frontend/src/pages/SettingsPage.tsx`
- `frontend/src/pages/systemSettingsContract.test.ts`
- `frontend/src/styles/stylesheet.ts`
- `frontend/src/styles/workbench.css`
- `docs/ux/workbench-u2-report.md`
