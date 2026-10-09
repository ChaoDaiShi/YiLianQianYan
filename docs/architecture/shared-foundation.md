# Shared Foundation

本文冻结 v1 Task World 与 v2 Desktop World 的最小共享基础。两条产品线必须从标签 `shared-foundation-v1-v2` 指向的同一提交起步。

## 边界

Shared 只包含稳定的基础契约：版本迁移、API 领域边界、Surface Host、Event、Command、Resource、Voice、Presence、ContextProvider 与 Projection。它不包含 TaskGraph、DesktopSpace、Task Supervisor、App Mount、Wallpaper、Widget 或任何完整领域模型。

跨域协作只走以下接口：

| 目的 | 接口 |
| --- | --- |
| 发现能力 | Capability descriptor |
| 请求动作 | Command |
| 观察结果 | Product Event |
| 读取有限状态 | Projection |
| 共享输入 | Resource |
| 跨 Surface 交互状态 | Voice / Presence |

严禁 v1 import v2 内部模块、v2 import Task Supervisor、跨域直接查询私有表、建立巨型 shared state，或把 Workspace 发展成万能领域对象。

## 性能路径

`/api/events` 只传低频 Product Event。Chat token 继续走 Chat SSE，音频走 Voice media path，高频桌面遥测与系统采样使用各自专用路径。音频字节、屏幕帧和逐 token 内容不得塞入 EventHub。

## 安全不变量

- Command request 不等于授权；现实副作用仍必须进入 SecurityExecutionGateway/Permission/Approval 路径。
- Capability discovered 不等于 Capability granted。
- Resource ingested 不等于 Resource executable。
- Desktop context available 不等于 Agent 获得无限桌面控制。
- 未知或无法判定的高风险动作 fail closed。

现有 SecurityExecutionGateway、Approval Store、MCP runtime、Agent Runtime、Memory、Workflow DAG、Workspace、Task execution、Artifact、Tauri embedded backend 与 Chat SSE 均保持原实现；Shared 只增加兼容适配层。

## 成熟度

- L0 Contract：协议存在。
- L1 Mock：可测试且明确标记 simulated/mock。
- L2 Functional：领域线提供真实实现。
- L3 Stable：权限、错误、兼容与测试稳定。

达到 L1 即允许另一条产品线继续开发，不要求等待 L3。
