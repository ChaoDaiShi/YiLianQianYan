//! MCP operation results — `tools/call`, `prompts/get`, `resources/read`.
//!
//! These sit opposite the `registry` parsers: `registry` reads what a server
//! advertises, `result` reads what a single operation returned, including the
//! multi-round-trip (`resultType: input_required`) outcome and the size bounds
//! that keep a hostile payload from reaching a caller unbounded.

pub mod prompt;
pub mod resource;
pub mod tool_call;
