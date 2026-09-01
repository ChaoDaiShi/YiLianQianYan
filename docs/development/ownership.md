# Ownership

## v1 / AI-A

主要拥有 `backend/src/task/`、`backend/src/workflow/`、`frontend/src/surfaces/workspace/`、未来 `frontend/src/features/task-graph/`、现有 execution/tasks 功能。AI-A 可以新增 `task.*` Event/Command 与 TaskProjection provider，但不得改变 Desktop 内部、实现 DesktopSpace，或占用 v2 migration 范围。

## v2 / AI-B

主要拥有 `frontend/src/surfaces/desktop/`、未来 `frontend/src/features/desktop/`、`frontend/src/features/widgets/`、`src-tauri/src/desktop/` 与 `src-tauri/src/app_mount/`。AI-B 可以新增 `desktop.*` Event/Command 与 DesktopContext provider，但不得读取 TaskGraph 内部、重写 Workspace 或占用 v1 migration 范围。

## Frozen Shared Zone

以下是冻结区：shared contracts、Event/Command Envelope、Resource identity、Presence contract、Surface host interface、migration framework 和 security identity contract。不得单方 rename/delete 字段、改变既有语义、绕过安全边界或扩大 Tauri default capability。

Breaking change 必须：提出显式设计、升级 schema version、提供双 Consumer 兼容/迁移测试、经 integration review，并同步两个 handoff。

## Evolving Shared Zone

双方可独立新增各自 namespace 的事件/命令、Capability descriptor 和 Resource source kind，也可提供既有 trait 的新 provider。新增必须保持 Envelope 与默认安全语义；Mock 必须明确 simulated，不得伪装执行成功。

冲突时以领域所有权为先，以 Shared 最小化为原则。任何跨域数据库表或直接内部 import 都应退回到 Projection/Command/Event 设计。
