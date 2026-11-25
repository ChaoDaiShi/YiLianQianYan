use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, thiserror::Error)]
pub(crate) enum PageError {
    #[error("{0}")]
    Invalid(&'static str),
    #[error("{0}")]
    Conflict(&'static str),
    #[error("页面或工作空间不存在")]
    Missing,
    #[error("归档的工作空间仅可阅读")]
    Archived,
    #[error(transparent)]
    Sql(#[from] rusqlite::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

pub(super) type Result<T> = std::result::Result<T, PageError>;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Page {
    pub id: String,
    pub workspace_id: String,
    pub parent_page_id: Option<String>,
    pub title: String,
    pub icon: Option<String>,
    pub cover_resource_id: Option<String>,
    pub sort_key: i64,
    pub favorite: bool,
    pub last_visited_at: Option<i64>,
    pub revision: i64,
    pub created_at: i64,
    pub updated_at: i64,
    pub deleted_at: Option<i64>,
    pub has_children: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct PageDocument {
    #[serde(flatten)]
    pub page: Page,
    pub blocks: Vec<PageBlock>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct PageBlock {
    pub id: String,
    #[serde(flatten)]
    pub content: BlockContent,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum BlockContent {
    Markdown {
        text: String,
    },
    PageLink {
        page_id: String,
    },
    KnowledgeLink {
        document_id: String,
        name: String,
        version: String,
        end_char: usize,
    },
    Resource {
        resource_id: String,
        name: String,
        mime_type: String,
    },
    Rich {
        conversation_id: String,
        message_id: String,
        snapshot: Value,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum BlockOperation {
    Insert {
        block: PageBlock,
        before: Option<String>,
    },
    Replace {
        block: PageBlock,
    },
    Remove {
        id: String,
    },
    Move {
        id: String,
        before: Option<String>,
    },
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NewPage {
    pub workspace_id: String,
    pub parent_page_id: Option<String>,
    pub title: String,
    pub markdown: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PageMetadata {
    pub revision: i64,
    pub title: Option<String>,
    pub icon: Option<String>,
    pub cover_resource_id: Option<String>,
    pub favorite: Option<bool>,
    #[serde(default, deserialize_with = "present_parent")]
    pub parent_page_id: Option<Option<String>>,
    pub before_page_id: Option<String>,
}

fn present_parent<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<Option<Option<String>>, D::Error> {
    Option::<String>::deserialize(deserializer).map(Some)
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct PageQuery {
    pub workspace_id: String,
    pub parent_page_id: Option<String>,
    pub limit: Option<i64>,
    #[serde(default)]
    pub offset: i64,
    pub q: Option<String>,
    pub view: Option<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct PageList {
    pub pages: Vec<Page>,
    pub next_offset: Option<i64>,
}

#[derive(Debug, Serialize)]
pub(crate) struct Revision {
    pub revision: i64,
    pub source: String,
    pub created_at: i64,
}
