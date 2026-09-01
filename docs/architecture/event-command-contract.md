# Event and Command Contract

## Event

`YiEvent` 是低频产品事件 Envelope：`id`、`namespace`、`type`、`source`、可选 `scope`、`timestamp`、`schema_version` 与对象型 `payload`。事件名使用小写分段命名，例如 `resource.created`、`voice.session.started`、`task.progress.changed`。

后端 `EventHub` 是有界广播通道，`GET /api/events` 以受控制会话保护的 SSE 暴露。Consumer 必须忽略未知 additive 字段；事件丢队时发送 `core.events.lagged`，而不是伪造领域成功。

## Command

`CommandRequest` 包含 `command`、`request_id`、`source`、可选 `target`、对象型 `payload` 和 `schema_version`。`CommandResult` 返回 `succeeded`、`failed` 或 `not_found`，并使用结构化 result/error。

`CommandRouter` 只负责注册与路由，不授予权限。任何真实副作用 handler 必须在内部调用现有安全执行路径。Shared 提供：

- `core.echo`：功能型本地 handler；
- `presence.get`：功能型只读 handler；
- `desktop.app.open`、`desktop.space.switch`：L1 Mock，固定返回 `executed=false`、`simulated=true`、`provider=mock`。

Voice 不直接调用 task/desktop 方法；它只能解析为 `task.*` 或 `desktop.*` Command。

## 兼容策略

Schema 当前为 1。先增加可选字段；Consumer 忽略未知字段。删除、改名或改变既有字段语义属于 breaking change，必须升级 `schema_version`、提供迁移说明并通过 integration gate。
