//! Product-owned bounded extraction, independent from the shared Resource DTO.
//!
//! Layout: `limits` holds the upload policy, `parsers` turn bounded bytes into
//! normalized content, `preview` projects a stored resource into the read-only
//! DTO held by `model`, and `ingest` / `binding` are the two use cases.
//!
//! Parsers never modify a Task. `binding` is the only place that reads task
//! bindings, and it only reads them.

mod binding;
mod ingest;
mod limits;
mod model;
mod parsers;
mod preview;

pub use binding::bound_node_resources;
pub use ingest::ingest_resource;
pub use limits::{validate_upload, MAX_INPUT_BYTES, MAX_INPUT_FILES, MAX_PREVIEW_CHARS};
pub use model::ResourcePreview;
pub use preview::{extract_preview, resource_preview};
