# 忆涟千言 v1.0.0-rc.2 候选说明

本版本只收口首次 Provider 引导、Settings/Secret 生命周期和 Voice 产品体验，不包含 v1.1、v2 或 Gate 4 功能。

## 本轮修复

- Settings 保存操作在 1280×720、1366×768、1920×1080 下可见可达，Global Voice Pill 不覆盖保存按钮。
- 模型、Embedding、STT、TTS 密钥支持安全保存、替换和显式清除；普通设置保存不会误删既有密钥，后端重启后仍可解析同一 Secret。
- 模型、STT、TTS 提供真实最小连接测试；失败仅显示安全归一化错误码。
- 未配置模型时，Chat、任务规划和 AI 审查给出明确的设置入口。
- 首次模块选择后增加薄引导；语音保持可选，不强制 MiniMax。
- 普通 Chat 语音识别结果进入编辑框且默认不自动发送；Assistant 消息支持手动 TTS。
- “与小涟语音对话”复用同一 GlobalVoiceSession，跨 v1 页面保持，并启用现有免手持打断与回声保护。
- v1.1 服务化方向仅记录到文档，没有开始实现。

## 安装包

- 文件：`忆涟千言_1.0.0-rc.2_x64-setup.exe`
- 大小：`8,838,102` bytes
- SHA-256：`3B4FA8A73C5A155CCC447B81B094D3A268CDFBAEA29360D01B478C2FA37BE406`
- 签名：未签名

## 当前状态

- 完整源代码技术门禁：PASS。
- 安装包构建与独立复算 SHA：PASS。
- 打包后 GUI 自动化：PASS。既有忆涟实例自然退出后，隔离安装、可见窗口、健康版本、截图内容、正常关闭、端口释放和卸载均通过。
- 远端 CI：`BLOCKED_BY_WORKFLOW_SCOPE`。
- 真人验收：`HUMAN_PENDING`。
- 正式 v1.0.0：`NOT RELEASED`。

