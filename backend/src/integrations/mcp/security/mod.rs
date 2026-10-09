//! MCP security boundaries.
//!
//! Two rules are enforced here and nowhere else:
//!
//! * `header` — every outbound header value derived from tool arguments is
//!   either a legal ASCII field value or base64-wrapped, and CR/LF never
//!   survive (header injection defense).
//! * `header_schema` — `x-mcp-header` annotations are scanned with a strictly
//!   bounded traversal, so a hostile `inputSchema` cannot make the client walk
//!   an arbitrarily deep or cyclic document.

pub mod header;
pub mod header_schema;
