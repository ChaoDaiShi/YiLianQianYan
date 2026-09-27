//! Task application layer — the use cases behind the HTTP surface.
//!
//! Route handlers extract, authenticate and validate at the transport
//! boundary; the decisions live here. Services take `&AppServer` because it is
//! this crate's composition root, matching `modules::settings::application`.
//!
//! Nothing in this layer knows about axum, `StatusCode` or `Response` — the
//! errors are typed and `api::mapping` turns them into HTTP.

pub mod capability_binding;
pub mod execution_service;
pub mod graph_service;
pub mod review_service;

pub mod capability_execution;
