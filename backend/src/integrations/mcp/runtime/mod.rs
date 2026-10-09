//! MCP runtime — server lifecycle, catalogues and response cache.
//!
//! `manager` owns the per-server state machine and is the only place allowed to
//! reach a transport; `cache` bounds repeated resource reads. Execution still
//! flows through the Security Execution Gateway, never from here.

pub mod cache;
pub mod manager;
