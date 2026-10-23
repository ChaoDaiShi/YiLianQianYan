# R3 页面预览与原型检查

[打开可点击原型](index.html) · [返回研究总文档](../README.md) · [页面与交互规格](../experience.md)

本版以当前项目结构为基础，用 HTML/CSS 与项目既有背景/图标素材绘制，截图由浏览器直接渲染。它不是生产实现，也不是当前 Tauri 程序截图。所有记录和任务均为示例，不连接后端、不调用模型、不访问用户数据、不发送外部消息。

## 页面截图

| 编号 | 页面/状态 | 入口 | 原图 |
|---|---|---|---|
| 01 | [已有对话](index.html#chat) | `/chat/:id` | [查看](screenshots/01-chat.png) |
| 02 | [新对话](index.html#home) | `/chat` | [查看](screenshots/02-home.png) |
| 03 | [消息画布](index.html#canvas) | `/connectors` | [查看](screenshots/03-canvas.png) |
| 04 | [工作空间](index.html#workspaces) | `/workspaces` | [查看](screenshots/04-workspaces.png) |
| 05 | [空间详情](index.html#workspace) | `/workspaces/:id` | [查看](screenshots/05-workspace.png) |
| 06 | [能力入口](index.html#capabilities) | `/capabilities` | [查看](screenshots/06-capabilities.png) |
| 07 | [工具管理](index.html#tools) | `能力 → 工具` | [查看](screenshots/07-tools.png) |
| 08 | [智能体](index.html#agents) | `/agents` | [查看](screenshots/08-agents.png) |
| 09 | [记忆](index.html#memory) | `/memory` | [查看](screenshots/09-memory.png) |
| 10 | [知识库](index.html#knowledge) | `/knowledge` | [查看](screenshots/10-knowledge.png) |
| 11 | [设置总览](index.html#settings) | `/settings` | [查看](screenshots/11-settings.png) |
| 12 | [设置分组展开](index.html#settings-expanded) | `/settings` | [查看](screenshots/12-settings-expanded.png) |
| 13 | [监控](index.html#monitor) | `/system` | [查看](screenshots/13-monitor.png) |
| 14 | [日志](index.html#logs) | `/logs` | [查看](screenshots/14-logs.png) |
| 15 | [账号与隐私](index.html#account) | `/account` | [查看](screenshots/15-account.png) |
| 16 | [首次配置](index.html#setup) | `/setup` | [查看](screenshots/16-setup.png) |
| 17 | [时间任务](index.html#automation) | `拟新增 /automations` | [查看](screenshots/17-automation.png) |
| 18 | [消息渠道](index.html#channels) | `拟新增 /channels` | [查看](screenshots/18-channels.png) |
| 19 | [资料学习](index.html#learning) | `拟新增空间页签` | [查看](screenshots/19-learning.png) |
| 20 | [创造与成果](index.html#outputs) | `空间成果 / 当前结果面板` | [查看](screenshots/20-outputs.png) |
| 21 | [更多功能](index.html#more) | `/more` | [查看](screenshots/21-more.png) |
| 22 | [插件功能弹窗](index.html#plugin) | `/more/plugins/:pluginId/:featureId` | [查看](screenshots/22-plugin.png) |
| 23 | [语音浮层](index.html#voice) | `global overlay` | [查看](screenshots/23-voice.png) |
| 24 | 800×600 对话视图 | `/chat/:id` | [查看](screenshots/24-chat-800.png) |

更多页由主导航进入；插件功能由更多页打开；语音浮层由输入框麦克风按钮或回复朗读入口打开。顶部“预览页面”选择器覆盖 20 个主要页面与状态，后 4 张补充专项浮层和窄窗口视图。

## 可验证交互

- 收起/打开会话列表；打开/关闭资料辅助区，宽度恢复，不保留右侧空槽。
- 展开处理过程；查看示例资料与原文高亮，Esc 关闭后回到触发按钮。
- 六类能力入口打开各自管理器；工具管理展示单项详情。
- 六组设置在画布中展开；模型组包含当前项目的五个子项，再打开配置编辑器。
- 工作空间列表搜索；学习题目作答反馈；输入框三模式选择。
- 时间定义、渠道设置、隐私、创作文稿提供表单/说明演示，反馈明确为原型，不冒充保存成功。

## 实际完成的检查

[机器可读检查结果](verification.json)：23 张桌面/浮层截图，另有 1 张窄窗口截图；9 项代表性交互通过；20 个页面在 1440×900 与 800×600 下检查无文档横向溢出；无页面脚本异常、无资源加载失败。

浏览器工具路径：本会话未提供 Browser plugin，使用现有 playwright-core 与本机 Edge 的 headless 模式。没有安装新依赖、启动产品服务器或打开实际用户数据库。

这些检查只证明原型可查看、代表交互可演示。正式产品的 provider、数据库、文件、Git、任务、渠道、语音与安装验收均未在本轮执行。原型未覆盖所有错误弹窗和全部主题组合。
