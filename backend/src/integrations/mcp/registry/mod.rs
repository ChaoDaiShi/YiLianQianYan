//! MCP catalogue parsing — what a server advertises.
//!
//! One file per catalogue kind, each parsing a `*/list` page into descriptors
//! plus a `nextCursor`. Malformed entries are skipped rather than failing the
//! whole page, so one bad tool cannot take a server offline.
//!
//! Operation results (`tools/call`, `prompts/get`, `resources/read`) are a
//! different shape and live in `result`.

pub mod prompts;
pub mod resources;
pub mod tools;
