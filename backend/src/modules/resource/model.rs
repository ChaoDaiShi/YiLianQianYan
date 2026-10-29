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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presentation: Option<Presentation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Presentation {
    pub blocks: Vec<PreviewBlock>,
    pub sheets: Vec<PreviewSheet>,
    pub slides: Vec<PreviewSlide>,
    pub entries: Vec<PreviewEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_count: Option<usize>,
}
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct PreviewBlock {
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runs: Option<Vec<PreviewRun>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rows: Option<Vec<Vec<String>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub src: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alt: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct PreviewRun {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreviewSheet {
    pub name: String,
    pub rows: Vec<Vec<String>>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreviewSlide {
    pub number: usize,
    pub blocks: Vec<PreviewBlock>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreviewEntry {
    pub name: String,
    pub size: u64,
    pub directory: bool,
}
