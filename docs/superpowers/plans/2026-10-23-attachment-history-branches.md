# 附件与对话分支实施计划

**Goal:** 完整实施已确认的附件卡片、来源分类、分支入口与画布交互。

**Architecture:** 复用资源服务和分支运行；为用户消息保存附件关联，以已有渠道映射提供来源，前端共享附件卡片和分支编辑组件。

**Tech Stack:** Rust/Axum/SQLite；React/TypeScript/Vite/Tauri；不增加依赖。

## 执行约束

主智能体在当前分支实施。先备份本次涉及的已有文件；不提交、不推送，不改动无关工作。用户已确认设计，无需重复询问。

## 1. 附件与来源数据

- [x] 在 `backend/src/db/conversation_presentation.rs` 添加附件快照、来源和分支复制测试，观察失败。
- [x] 增加 SQLite 消息附件关联；在 `conversation_turns.rs` 接收用户消息时同事务保存明确的 `resource_ids`。
- [x] 在 `api/conversations.rs`、`api/connector_messages.rs` 通过查询投影提供附件和来源；旧消息不猜测归属。
- [x] 复制分支历史时复制附件关联；渠道来源保存后可供历史列表使用。
- [x] 运行定向 Rust 测试与 `cargo check`。

## 2. 附件卡片

- [x] 在 `features/resources` 共享图片/文件卡片和预览弹窗，替换输入框折叠入口。
- [x] 在 `MessageBubble.tsx` 显示已发送附件；`ChatView.tsx` 和 `conversationRuntime.ts` 保留即时与持久化附件。
- [x] 按后续反馈将附件放在输入框／对应消息文字上方；以记录恢复可核实的历史附件关联，验证发送、刷新、放大、Word 解析预览及移除。

## 3. 历史与分支

- [x] 为 `ConversationSidebar.tsx` 添加桌面/渠道来源筛选、来源标签、常显分支图标和数量。
- [x] 增加分支总览弹窗；复用 `messageForest.ts` 与当前家族数据，支持主线/分支切换和画布定位。
- [x] 定向验证来源判断与分支家族的行为。

## 4. 画布与验收

- [x] 调整 `ConversationCanvas.tsx`、`MessageBranchWorkspace.tsx`、`messageBranches.css`：来源筛选、消息详情、分支上下文、流式发送/停止/错误状态。
- [x] 运行 `cargo fmt`、`cargo fmt --check`、`cargo check` 和相关测试；运行前端相关测试及生产构建。
- [x] 使用已有 Playwright 与系统 Edge 验证实际界面，再检查原生 Tauri 窗口。Browser 插件当前不可用。
- [x] 实际截图和验收文件保存在 F 盘临时验收目录；完整报告实际结果与限制。
