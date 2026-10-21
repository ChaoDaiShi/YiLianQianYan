# 第三版：连接器与节点式设置

日期：2026-10-21。用户要求本轮将页面职责拆分：连接器整页用于消息分支无限画布，之前的资源关系图迭代为新版设置。当前分支`codex/connector-workbench`，未提交、未推送。

## 页面行为

| 入口 | 本轮行为 |
|---|---|
| 连接器 `/connectors` | 全宽消息树，仅保留问答、分支、节点详情、平移缩放、对话目录及画布管理；资源设置面板和添加资源/显示筛选控件已经移出该页 |
| 设置 `/settings` | 节点式无限画布，默认全局设置中心；选择已有配置画布后，沿用原智能体/模型/工具/记忆等关系与原绑定API，并补充设置子功能节点 |
| 右侧节点管理器 | 每次进入默认收起，仅保留48px入口；展开为可滚动列表，分已连接、未连接预设以及实际资源；支持配置、连接、断开和恢复隐藏资源 |
| 设置节点编辑 | 点击节点直接打开对应真实表单；保留保存、丢弃、密钥替换/清除、连接验证和主题即时生效语义 |

十四类预设为：对话模型、语音识别、语音合成、联网搜索、图像理解、图像生成、向量模型、智能体行为、上下文管理、权限审批、执行沙箱、扩展目录、界面外观、导航布局。默认连接对话模型、权限审批与界面外观，其余显示为未连接预设。

设置节点的连接表示将入口加入当前设置画布；断开不会清除已填参数。实际智能体资源仍使用原连接/解除绑定操作。全局与各配置画布分别保存节点、位置和视口；设置画布状态使用已有SQLite settings表，不建立第二套配置或凭据存储。

## 主要实现位置

- `frontend/src/features/connectors/ConnectorsPage.tsx`：整页消息画布与原画布增删选择。
- `frontend/src/pages/SettingsPage.tsx`：转入新设置画布。
- `frontend/src/features/settings/canvas/`：设置图、节点管理器、共享14项目录、布局组合与测试。
- `frontend/src/features/settings/editor/SettingsEditorDialog.tsx`：复用原配置保存与secret语义的逐节点编辑器；依赖AppConfig的表单在真实读取完成后开放编辑。
- `frontend/src/components/system/ModuleSetup.tsx`：原入口与新导航节点共享同一实际导航表单。
- `backend/src/api/settings_canvas.rs`、`backend/src/db/settings_canvas.rs`：GET/PUT `/api/settings/canvas?canvas_id=...`，画布级状态隔离与删除清理。
- `frontend/src/components/ui/useDialogFocusLifecycle.ts`：只有最上层弹窗捕获Tab，验证厂商账户编辑不会被底层设置弹窗抢焦点。

## 验证

| 范围 | 结果 | 证据 |
|---|---|---|
| 前端自动化 | 141文件、671测试通过 | target/revision3-vitest.json |
| 前端生产构建 | TypeScript与Vite通过 | target/revision3-build.log |
| 后端 | 1069单元与15项相关集成测试通过；cargo check通过 | target/revision3-rust-tests.log、target/revision3-cargo-check.log |
| 设置画布专项 | 4项通过，覆盖KV持久化、重开、作用域隔离、GET不写、真实配置不变、删除清理及API错误 | target/revision3-settings-tests.log |
| 浏览器 | 27项实际流程检查通过 | target/canvas-e2e/2026-10-21T04-59-30-274Z/result.json |
| 原生桌面 | 原10项浮窗检查及同一组27项修订流程通过 | F:/target/workbench-desktop/2026-10-21T05-02-38-834Z/results.json |
| 桌面构建与格式 | QA及正常配置构建、cargo fmt --check、git diff --check通过 | target/revision3-native-build.log、target/revision3-native-normal.log |

实际流程完成了三轮问答和兄弟分支隔离；连接器内容宽度等于完整页面工作区。设置页验证默认收起、3个默认连接与11个未连接预设，连接STT节点后通过真实settings API修改模型名，保存、重开读回、断开后参数保留；配置画布间状态相互独立，既有资源仍存在。明暗主题、透明输入、顶部通知和三种画布删除仍通过。

模型历史验证使用受控本地HTTP服务，经产品真实API/SSE、运行时与持久化；未进行付费厂商与声学设备验收。构建保留主包超过500kB的提示，没有生成安装包或替换已安装程序。测试仅使用独立数据目录与QA进程。

[实际运行预览](revision3-previews/index.md) · [机器可读结果](verification/revision3-summary.json) · [实施计划](revision-3-plan.md)
