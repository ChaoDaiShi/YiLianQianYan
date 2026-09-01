# Integration Gates

## Gate 0 — Foundation（本标签必须通过）

- StandaloneHost → WorkspaceSurface；
- DesktopHostSkeleton → DesktopSurfaceSkeleton → WorkspaceSurface；
- backend EventHub emit → `/api/events` SSE → frontend parser/subscriber；
- frontend Command → `/api/commands` → router → functional/mock handler → structured result；
- file bytes → safe storage → Resource ID → query；
- VoiceSession lifecycle、deterministic STT/TTS、interrupt、Presence；
- Mock TaskProjection、DesktopContext 与 desktop commands；
- fresh/v0.9/idempotent migrations。

自动化入口是 `backend/tests/shared_foundation.rs`、Shared 单元测试和前端 `sharedSpine.test.ts`、`surfaceHost.test.ts`、`voiceProjection.test.ts`。

## Gate 1 — Early Integration（后续）

v1 Task summary 通过 Event/TaskProjection 在 v2 Surface 显示；v1 working 状态通过 Presence 被 v2 小涟消费。Mock 可先替换其中一端，但必须标记成熟度。

## Gate 2 — Cross-domain Action（后续）

v1 经 `desktop.app.open` 调用 v2 的真实安全执行 adapter；v2 经 TaskProjection 显示 v1 Task。Command request 不替代 Authorization，真实桌面副作用必须进入安全网关。

## Gate 3 — Voice（后续）

Voice intent 经 Command Router 分别路由到 `task.pause` 和 `desktop.app.open`。Voice Core 不得直接依赖 Task 或 Desktop 实现。

每个 Gate 必须报告 PASS/FAILED/PARTIAL/BLOCKED；Mock pass 与真实 Provider pass 要分开，不使用“基本完成”等模糊结论。
