# Compatibility Facades

> Scope: the `refactor/v1-architecture-semantic` line.
> Status: **closed** — no compatibility glob remains at the crate root.

## Why they existed

The R1 structural refactor moved code into `modules/` and `integrations/`. Every
move left a one-line module at the old path:

```rust
// backend/src/task.rs, before R2
pub use crate::modules::task::*;
```

That kept the move behaviour-preserving: a hundred call sites kept compiling
while the tree was reorganised, and each shim carried a comment naming the
module it pointed at. The cost was twelve extra names for the same items, so a
reader could not tell from a path which module actually owned a type.

R1 recorded them as migration scaffolding, not as API. R2 finishes the
migration.

## Classification

A shim is kept only if something **outside** this crate names it. The only
consumer outside `yilian-backend` is the `src-tauri` crate (`yi-lian-qian-yan`),
and it names exactly two paths:

```rust
yilian_backend::safety::ControlSession::generate()
yilian_backend::serve_in_background_with_control_token(token)
```

`safety` is a real module and `serve_in_background_with_control_token` is a
curated re-export declared in `lib.rs`. Neither is a shim.

**Class A (public API): none.** No shim had a consumer outside the crate.
**Class B (internal): all twelve.** Each was drained.

## Drained

Every reference — in the library (`crate::…`) and in `backend/tests/`
(`yilian_backend::…`) — now names the owning module:

| Old path | Owner |
|---|---|
| `crate::capability` | `crate::modules::capability` |
| `crate::llm` | `crate::integrations::llm` |
| `crate::mcp` | `crate::integrations::mcp::legacy_stdio` |
| `crate::mcp_runtime` | `crate::integrations::mcp` |
| `crate::memory_skill` | `crate::modules::memory_skill` |
| `crate::resource_input` | `crate::modules::resource` |
| `crate::secret` | `crate::integrations::secret` |
| `crate::server` | `crate::app::state` |
| `crate::skill_management` | `crate::modules::memory_skill::store` |
| `crate::task` | `crate::modules::task` |
| `crate::voice` | `crate::modules::voice` |
| `crate::workflow` | `crate::modules::workflow` |

`crate::server::mcp_transport_config` was the one item whose owner was not the
module its shim pointed at; it moved to `crate::integrations::mcp::config` in
the same stage.

Three commits, each independently revertible:

```text
refactor(core): drain eight compatibility facades
refactor(core): drain the llm and secret facades
refactor(core): drain the server and task facades
```

## What counts as the crate's public surface now

`backend/src/lib.rs` declares only real modules plus four deliberate
re-exports:

```rust
pub use app::bootstrap::create_server;
pub use app::lifecycle::{serve, serve_in_background, serve_in_background_with_control_token};
pub use app::state::AppServer;
```

Everything else is reached through the module that owns it. Adding a new shim
would undo this stage: if a type needs a shorter path, give it a shorter path in
its own module rather than a second name at the root.

## Adding a module move in future

A move inside a module needs no shim — `modules/<name>/mod.rs` is already the
public surface. A move **between** top-level modules does, but only for as long
as it takes to rewrite the call sites in the same branch; do not land a shim and
a follow-up ticket, because the follow-up is what stops happening.

## Not in scope

- `crate::shared::*` and `crate::safety::*` are Protected Kernel surfaces, not
  compatibility layers. They are expected to keep their paths.
- The twelve removals are a Rust-visible API change for the `yilian-backend`
  library target. `docs/refactor/r2-final-report.md` records it under public API
  changes; the `src-tauri` consumer is unaffected.
