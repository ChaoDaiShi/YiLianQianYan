//! Admission of user uploads into the shared resource store.
//!
//! Policy is applied before anything is written: the upload must pass
//! `validate_upload`, and the preview is computed from the same bytes so the
//! record and its projection can never disagree about what arrived.

use crate::shared::resource::{Resource, ResourceService};

use super::{limits::validate_upload, preview::extract_preview};

pub fn ingest_resource(
    service: &ResourceService,
    name: &str,
    mime: &str,
    bytes: &[u8],
) -> Result<Resource, String> {
    let mime = validate_upload(name, mime, bytes.len(), 1)?;
    let preview = extract_preview(name, &mime, bytes);
    service
        .ingest(
            name,
            &mime,
            bytes,
            serde_json::json!({"source":"user-upload","preview":preview}),
        )
        .map_err(|_| "资源存储失败，请重试".into())
}
