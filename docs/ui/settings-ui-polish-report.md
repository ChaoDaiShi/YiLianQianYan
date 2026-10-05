# Settings UI Polish Report

日期：2026-10-06  
分支：`feat/v1-settings-ui-polish`  
基线：`feat/v1-typed-data-binding-u31 @ 9f935304e394ccbbd10747fa2666f29167f15983`

## 结果

设置页从 8 个一级入口收敛为 6 个：模型服务、语音能力、智能体、安全控制、能力扩展和外观。旧的权限/沙箱、智能体/压缩、技能/子智能体不再重复占用一级导航。旧 URL section 参数继续映射到对应新分组。

页面标题栏现在是唯一的全局保存入口，同时显示未保存、保存中、已保存和失败状态。资源授权创建/删除保留独立显式语义；当前授权列表优先显示，创建授权默认折叠。

视觉统一使用现有主题 token 和 Lucide 线性图标。没有引入外部贴图、依赖、重玻璃或大型插画。标题、正文、控件和辅助文字使用统一层级；设置内容中的原 `text-xs` 提升为 13px，常规控件保持 14px，输入高度不低于 40px。

## 宽屏与响应式

参考 Microsoft Windows app 的 responsive layout 指导：

- `< 900px`：分类导航移动到顶部并横向滚动，内容单列，保存区换行。
- `900–1599px`：248px 左侧导航，内容最大宽度 960px。
- `>= 1600px`：264px 左侧导航，内容最大宽度 1280px，独立卡片使用双列。
- 长文本、密钥、路径和复杂模型配置使用宽卡片，不被拉成难以阅读的超长行。
- 页面不新增设置搜索、快捷入口或第二套保存操作。

参考资料：

- https://learn.microsoft.com/en-us/windows/apps/design/layout/
- https://learn.microsoft.com/windows/apps/design/layout/responsive-design
- https://learn.microsoft.com/windows/apps/design/layout/screen-sizes-and-breakpoints-for-responsive-design

## 主要变更

- 新增 `settingsNavigation.ts`：六组元数据、统一图标和旧 deep-link 映射。
- 新增 `SettingsSectionFrame.tsx`：分组标题和卡片网格。
- 新增 `settings.css`：设置页唯一的布局与响应式样式来源。
- 更新 `SettingsPage.tsx`：单一保存入口与六组组合。
- 更新模型、语音、智能体、压缩、安全、沙箱、技能、外观和资源授权的卡片层级。
- 删除 `system.css`、`shell.css`、`workbench.css` 中重复的旧设置页规则。

## 范围与安全

后端、Cargo、数据库、REST、SecretStore、依赖清单、Canvas、Chat、Voice 运行时和 v2 均未修改。原字段、回调、确认流程、资源授权 CRUD 和保存 API 保留。

## 验证

根据用户明确要求，本轮没有新增或运行单元测试、集成测试、E2E、视觉自动化或人工多尺寸验收。

只运行一次前端生产构建：

```text
npm.cmd run build
exit 0
✓ built in 8.03s
```

构建保留既有主 chunk 大于 500kB 的提示。本轮没有进行无关拆包。

源码审计结果：六个可见分组、四个旧链接别名、一个全局保存入口；保护路径改动为 0。证据保存在 ignored `target/settings-ui-polish/`。

## 明确保留的限制

- 未运行测试，因此不声明测试套件通过。
- 未进行浏览器或真实桌面尺寸截图验收；响应式结论来自实现规则与生产构建，不表述为实机视觉验收。
- 现有源码契约测试仍描述旧的八组导航和保存卡片，本轮遵守“不做测试”要求，未修改或执行这些测试；后续若恢复测试门禁，需要同步该契约文件。
- 本分支保留本地，未推送、未合并、未发布。
