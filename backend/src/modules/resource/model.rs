//! Read-only projection of a stored resource, safe to hand to a surface.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourcePreview {
    pub status: String,
    pub kind: String,
    pub mime_type: String,
    pub text: Option<String>,
    pub truncated: bool,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub error: Option<String>,
}
