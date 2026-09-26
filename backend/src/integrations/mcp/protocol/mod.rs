//! MCP wire protocol — JSON-RPC framing, the typed model, and protocol era.
//!
//! Nothing here performs I/O; `transport` owns the connections and `runtime`
//! owns the server lifecycle. Header-value safety is a security boundary and
//! lives in `security::header`, not here.

pub mod jsonrpc;
pub mod model;
pub mod version;
