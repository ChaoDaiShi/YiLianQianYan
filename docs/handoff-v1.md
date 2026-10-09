# Handoff A — v1 / AI-A

## Foundation identity

- Tag: `shared-foundation-v1-v2`
- Foundation SHA: 以 `git rev-parse 'shared-foundation-v1-v2^{commit}'` 的完整输出为唯一权威值；开始前与 v2 worktree 核对相同。
- Base v0.9 commit: `7906ce7d8cbc29b207a22a222f24c52f594eae35`

## Ownership and scope

你拥有 Task/Workflow、WorkspaceSurface 内 Task UX、execution/tasks 与未来 TaskGraph。不要实现 DesktopSpace、App Mount、Wallpaper/Widget，也不要改变 Frozen Shared Envelope。

## Shared APIs

- Event: `YiEvent` / `GET /api/events`；发布 `task.*` 产品事件。
- Command: `POST /api/commands`；注册 `task.*` handler。现实副作用仍进入安全网关。
- Resource: ingest/list/get，以 Resource ID 作为输入身份。
- Voice/Presence: 通过 Command adapter 和分层 Presence 集成，不让 VoiceCore import Task。
- Context/Projection: 实现 `TaskProjectionProvider`；不得向 v2 暴露 TaskGraph。
- Desktop Mock: `desktop.app.open`、`desktop.space.switch` 当前为 `executed=false` 的 L1 mock，可供 v1 先开发命令链。

## Migration and freeze

只使用 1000–1999。Shared 0–999、v2 2000–2999 不可占用。Event/Command/Resource/Presence/Surface/Migration/Security identity 属 Frozen Shared；以 additive extension 为默认。跨域契约达到可消费状态后滚动合入 `integration/v1-v2`。
