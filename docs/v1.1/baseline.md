# YiLianQianYan v1.1 Development Baseline

日期：2026-10-06

## 冻结基线

v1.1 的冻结公共代码基线为：

```text
branch: v1.1/base
commit: 012dc010ebb730148a3583200f8275dcf9a0fa03
source: feat/v1-context-rail-layout
```

实施分支与独立工作树为：

```text
branch: feat/v1-1-user-cloud-experience
worktree: F:\项目开发\忆涟千言\YiLian-v1.1-user-cloud-experience
```

该基线完整包含 Architecture R2、Canvas Stability、Canvas Studio C2、Workbench U2、Chat Reliability、Generic MCP Canvas U3、MCP Resource Grant U3-C、Typed Dependency Output Binding U3.1、Settings UI Polish 和 Context Rail Layout。`feat/v1-typed-data-binding-u31 @ 9f935304` 是冻结基线的祖先，因此本轮不会为了遵循旧文档字段而回退已经完成的体验改造。

## 冻结产品原则

- Local-first：游客与离线状态拥有完整本地核心能力。
- Optional Identity：登录只增强同步、恢复和跨设备能力。
- Explicit Upload：已有本地数据不得因登录静默上传。
- Secret Local-only：LLM、Voice、Search、MCP 凭据默认只进入本机 SecretStore。
- Incremental Sync：同步使用 outbox、cursor、revision、tombstone 和 conflict，不镜像覆盖本地 SQLite。
- Private Boundary：服务器、Admin、Worker 和内部部署源码只能存在于公共 Git Root 之外的私有本地仓。
- Graceful Degradation：云端不可用时客户端回到 local-only，不阻止启动与本地工作。

## 公共与私有仓边界

```text
F:\项目开发\忆涟千言\
├─ YiLian-v1.1-user-cloud-experience\   PUBLIC implementation worktree
└─ YiLianCloud-Private\                 PRIVATE local Git repository
```

公共仓允许出现公开 Cloud Contract、客户端适配器、同步状态机、Account/Sync/Privacy/Search/Setup 界面和公开安全文档。私有仓承载 cloud-api、admin-api、admin-web、workers、PostgreSQL migration 和 deployment artifacts。两侧只通过版本化公开契约交互，不使用源码软链接。

## 版本目标

本轮目标版本为 `1.1.0-rc.1`。只有公共客户端、私有服务、同步、隐私生命周期、搜索、首次配置、Admin、自动化门禁和 Windows/Tauri 人工门禁均形成证据后，才能将 RC 状态标记为完整通过。
