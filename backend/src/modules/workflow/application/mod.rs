//! Workflow application layer — the use cases behind the HTTP surface.
//!
//! Route handlers extract and validate at the transport boundary; the decisions
//! live here. Services take `&AppServer` (the composition root) or explicit
//! dependencies, matching `modules::task::application`. Nothing here knows
//! about axum, `StatusCode` or `Response` — errors are typed and
//! `api::mapping` turns them into HTTP.

pub mod approval_service;
pub mod graph_service;
pub mod run_service;
