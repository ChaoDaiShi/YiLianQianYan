# Handoff B — v2 / AI-B

## Foundation identity

- Tag: `shared-foundation-v1-v2`
- Foundation SHA: 以 `git rev-parse 'shared-foundation-v1-v2^{commit}'` 的完整输出为唯一权威值；开始前与 v1 worktree 核对相同。
- Base v0.9 commit: `7906ce7d8cbc29b207a22a222f24c52f594eae35`

## Ownership and scope

你拥有 DesktopSurface、未来 DesktopSpace、desktop/widgets、Tauri desktop/app_mount native adapter。DesktopSpace 不继承 Workspace；通过 DesktopSurfaceSkeleton 承载现有 WorkspaceSurface。不要读取 TaskGraph 或重写 v0.9 Workspace。

## Shared APIs

- Surface: 扩展 `DesktopSurfaceSkeleton`，保留同一 WorkspaceSurface contract。
- Event/Command: 实现 `desktop.*` provider/handler，保持 Envelope；真实 Windows 动作必须通过授权与 SecurityExecutionGateway。
- Presence/Voice: 分层 Presence 可直接驱动跨 Surface 呈现；Voice 只生成 Command intent。
- Resource: 使用 Resource ID，不接收原始绝对路径，也不把上传等同执行授权。
- Task Mock: `MockTaskProjectionProvider` 让 v2 Widget/通知可在 v1 尚未完成时开发；替换真实 provider 时保持 Projection shape。
- DesktopContextProjection: 只暴露有限上下文，不把 HWND/window tree 泄露给 v1。

## Migration and native boundary

只使用 2000–2999。现有 Tauri default capability 仅 `core:default` 与 `shell:allow-open`，不要扩大。外部 Wallpaper、第三方 UI 或 embedded web content 必须规划独立低权限 window/webview，默认不得与主业务 WebView 共享高权限上下文。

Frozen Shared breaking change 需双方 integration review。跨域能力达到 L1 即可消费，真实实现按 L2/L3 后续替换。
