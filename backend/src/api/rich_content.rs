use std::{
    collections::{BTreeMap, HashSet},
    sync::Arc,
};

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::{
    api::workspace_pages::{page_error, Error},
    app::state::AppServer,
    db::{
        rich_content::ComponentState,
        workspace_pages::{
            BlockContent, BlockOperation, NewPage, PageBlock, PageDocument, PageError,
        },
    },
};

mod protocol;

mod schema;
pub(crate) use schema::PROMPT;
use schema::{parse_message, Block, PageChange, Part};

fn server_error(error: impl ToString) -> Error {
    (StatusCode::INTERNAL_SERVER_ERROR, error.to_string())
}

fn message_source(server: &AppServer, conversation: &str, message: &str) -> Result<String, Error> {
    server
        .db
        .rich_message_source(conversation, message)
        .map_err(server_error)?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                "没有找到当前对话中的该助手消息".into(),
            )
        })
}

fn known_workspace(server: &AppServer, conversation: &str) -> Result<Option<String>, Error> {
    server
        .db
        .conversation_workspace(conversation)
        .map_err(server_error)
}

fn available_page(server: &AppServer, workspace: Option<&str>, id: &str) -> Option<PageDocument> {
    server
        .db
        .workspace_page(id, false)
        .ok()
        .filter(|document| Some(document.page.workspace_id.as_str()) == workspace)
}

fn proposed_page_id(message: &str, component: &str) -> String {
    let digest = Sha256::digest(format!("page-proposal:{message}:{component}"));
    format!("{:x}", digest)
}

fn proposal_source(message: &str, component: &str) -> String {
    format!("assistant_proposal:{message}:{component}")
}

fn project_block(
    server: &AppServer,
    conversation: &str,
    message: &str,
    block: Block,
) -> Result<Value, Error> {
    let action = block.action();
    let id = block.id().to_owned();
    let mut value = match &block {
        Block::Image { resource_id, .. }
        | Block::File { resource_id, .. }
        | Block::Artifact { resource_id, .. } => {
            let owned = server
                .db
                .conversation_owns_rich_resource(conversation, resource_id)
                .map_err(server_error)?;
            let resource = if owned {
                server.db.get_resource(resource_id).map_err(server_error)?
            } else {
                None
            };
            match resource.filter(|r| {
                !matches!(block, Block::Image { .. }) || r.mime_type.starts_with("image/")
            }) {
                Some(resource) => {
                    let mut value = serde_json::to_value(&block).map_err(server_error)?;
                    value["resource"] = json!({"id":resource.id,"name":resource.name,"mime_type":resource.mime_type,
                        "size":resource.size,"width":resource.metadata.get("width"),"height":resource.metadata.get("height"),
                        "source":resource.source});
                    value
                }
                None => json!({"type":"unavailable","id":id,"message":"当前对话没有该资源"}),
            }
        }
        Block::TaskStatus { task_id, .. } => {
            let id = crate::modules::task::TaskId::new(task_id.clone());
            let task = match id {
                Ok(id) => server.db.get_task(&id).map_err(server_error)?,
                Err(_) => None,
            };
            let workspace = known_workspace(server, conversation)?;
            match task.filter(|task| Some(task.workspace_id.as_str()) == workspace.as_deref()) {
                Some(task) => {
                    json!({"type":"task_status","id":block.id(),"task_id":task_id,"workspace_id":task.workspace_id.as_str(),"title":task.title,"status":task.status.to_string()})
                }
                None => {
                    json!({"type":"unavailable","id":block.id(),"message":"当前工作空间没有该任务"})
                }
            }
        }
        Block::PageProposal {
            page_id, changes, ..
        } => {
            let workspace = known_workspace(server, conversation)?;
            let Some(page) = available_page(server, workspace.as_deref(), page_id) else {
                return Ok(
                    json!({"type":"unavailable","id":id,"message":"当前工作空间没有该页面","available_actions":[]}),
                );
            };
            let mut value = serde_json::to_value(&block).map_err(server_error)?;
            if let Some(revision) = server
                .db
                .page_proposal_revision(page_id, &proposal_source(message, &id))
                .map_err(page_error)?
            {
                value["applied_page"] =
                    json!({"id":page_id,"workspace_id":page.page.workspace_id,"revision":revision});
            } else {
                for (index, change) in changes.iter().enumerate() {
                    if let PageChange::Reference { reference_page_id } = change {
                        let Some(target) =
                            available_page(server, workspace.as_deref(), reference_page_id)
                        else {
                            return Ok(
                                json!({"type":"unavailable","id":id,"message":"当前工作空间没有提案引用的页面","available_actions":[]}),
                            );
                        };
                        value["changes"][index]["reference_title"] = json!(target.page.title);
                    }
                }
            }
            value
        }
        Block::PageCreateProposal { parent_page_id, .. } => {
            let workspace = known_workspace(server, conversation)?;
            if workspace.is_none() {
                return Ok(
                    json!({"type":"unavailable","id":id,"message":"请先选择对话工作空间","available_actions":[]}),
                );
            }
            let mut value = serde_json::to_value(&block).map_err(server_error)?;
            if let Some(page) = available_page(
                server,
                workspace.as_deref(),
                &proposed_page_id(message, &id),
            ) {
                value["applied_page"] =
                    json!({"id":page.page.id,"workspace_id":page.page.workspace_id,"revision":1});
            } else if let Some(parent) = parent_page_id {
                let Some(target) = available_page(server, workspace.as_deref(), parent) else {
                    return Ok(
                        json!({"type":"unavailable","id":id,"message":"当前工作空间没有该父页面","available_actions":[]}),
                    );
                };
                value["parent_title"] = json!(target.page.title);
            }
            value
        }
        _ => serde_json::to_value(&block).map_err(server_error)?,
    };
    value["component_id"] = json!(id);
    value["component_type"] = value["type"].clone();
    value["schema_version"] = json!(1);
    value["permissions"] = if matches!(
        block,
        Block::PageProposal { .. } | Block::PageCreateProposal { .. }
    ) {
        json!(["user_confirmed_page_edit"])
    } else if action.is_some() {
        json!(["write_component_state"])
    } else {
        json!([])
    };
    value["available_actions"] = if value.get("applied_page").is_some() {
        json!([])
    } else {
        json!(action.into_iter().collect::<Vec<_>>())
    };
    Ok(value)
}

#[derive(Serialize)]
struct RichMessage {
    parts: Vec<Value>,
    states: BTreeMap<String, ComponentState>,
}

fn read_message(
    server: &AppServer,
    conversation: &str,
    message: &str,
) -> Result<RichMessage, Error> {
    let source = message_source(server, conversation, message)?;
    let parts = parse_message(&source)
        .into_iter()
        .map(|part| match part {
            Part::Text(text) => Ok(json!({"type":"markdown","text":text})),
            Part::Rich(blocks) => {
                let blocks = blocks
                    .into_iter()
                    .map(|block| project_block(server, conversation, message, block))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(json!({"type":"rich","blocks":blocks}))
            }
        })
        .collect::<Result<Vec<_>, Error>>()?;
    let states = server.db.component_states(message).map_err(server_error)?;
    Ok(RichMessage { parts, states })
}

pub(crate) fn router() -> Router<Arc<AppServer>> {
    Router::new()
        .route(
            "/api/v1/conversations/:conversation/messages/:message/rich-content",
            get(detail),
        )
        .route(
            "/api/v1/conversations/:conversation/messages/:message/rich-content/:component/actions",
            post(dispatch),
        )
        .route(
            "/api/v1/conversations/:conversation/messages/:message/save-to-page",
            post(save_to_page),
        )
}

async fn detail(
    State(server): State<Arc<AppServer>>,
    Path((conversation, message)): Path<(String, String)>,
) -> Result<Json<RichMessage>, Error> {
    Ok(Json(read_message(&server, &conversation, &message)?))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Action {
    action: String,
    state_version: i64,
    #[serde(default)]
    values: Value,
}

fn invalid_action(message: &str) -> Error {
    (StatusCode::BAD_REQUEST, message.into())
}

fn action_payload(block: &Block, input: &Value) -> Result<Value, Error> {
    match block {
        Block::Chart { values, .. } => {
            let selected = input
                .get("selected")
                .and_then(Value::as_u64)
                .filter(|index| *index < values.len() as u64)
                .ok_or_else(|| invalid_action("请选择图表中有效的数据项"))?;
            Ok(json!({"selected":selected}))
        }
        Block::Table { columns, .. } => {
            let column = input
                .get("column")
                .and_then(Value::as_u64)
                .filter(|column| *column < columns.len() as u64)
                .ok_or_else(|| invalid_action("表格排序列无效"))?;
            let descending = input
                .get("descending")
                .and_then(Value::as_bool)
                .ok_or_else(|| invalid_action("表格排序方向无效"))?;
            Ok(json!({"column":column,"descending":descending}))
        }
        Block::Form { fields, .. } => {
            let input = input
                .as_object()
                .ok_or_else(|| invalid_action("表单数据应为对象"))?;
            if input
                .keys()
                .any(|key| !fields.iter().any(|field| &field.name == key))
            {
                return Err(invalid_action("表单包含未声明的字段"));
            }
            let mut values = serde_json::Map::new();
            for field in fields {
                let value = match input.get(&field.name) {
                    Some(Value::String(value)) => value.as_str(),
                    None => "",
                    _ => return Err(invalid_action("表单只接受文本或选项值")),
                };
                if (field.required && value.trim().is_empty())
                    || value.chars().count() > 4096
                    || (!value.is_empty()
                        && !field.choices.is_empty()
                        && !field.choices.iter().any(|choice| choice == value))
                {
                    return Err(invalid_action(&format!(
                        "请检查「{}」的填写内容",
                        field.label
                    )));
                }
                values.insert(field.name.clone(), json!(value));
            }
            Ok(Value::Object(values))
        }
        Block::InteractiveCard { .. } => Ok(json!({"acknowledged":true})),
        _ => Err(invalid_action("组件不支持此操作")),
    }
}

async fn dispatch(
    State(server): State<Arc<AppServer>>,
    Path((conversation, message, component)): Path<(String, String, String)>,
    Json(input): Json<Action>,
) -> Result<Json<Value>, Error> {
    let source = message_source(&server, &conversation, &message)?;
    let block = parse_message(&source)
        .into_iter()
        .filter_map(|part| match part {
            Part::Rich(blocks) => Some(blocks),
            _ => None,
        })
        .flatten()
        .find(|block| block.id() == component)
        .ok_or_else(|| (StatusCode::NOT_FOUND, "该助手消息没有此组件".into()))?;
    if block.action() != Some(input.action.as_str()) {
        return Err(invalid_action("该组件未声明此操作"));
    }
    if let Block::PageCreateProposal {
        title,
        markdown,
        parent_page_id,
        ..
    } = block
    {
        let workspace_id = known_workspace(&server, &conversation)?
            .ok_or_else(|| (StatusCode::FORBIDDEN, "请先选择对话工作空间".into()))?;
        let blocks = vec![PageBlock {
            id: uuid::Uuid::new_v4().to_string(),
            content: BlockContent::Markdown { text: markdown },
        }];
        let document = server
            .db
            .create_page_with_id(
                NewPage {
                    workspace_id,
                    title,
                    parent_page_id,
                    markdown: None,
                },
                blocks,
                "assistant_proposal",
                &proposed_page_id(&message, &component),
            )
            .map_err(page_error)?;
        return Ok(Json(json!({"page":document})));
    }
    if let Block::PageProposal {
        page_id,
        revision,
        changes,
        ..
    } = block
    {
        let page = server
            .db
            .workspace_page(&page_id, false)
            .map_err(page_error)?;
        if known_workspace(&server, &conversation)?.as_deref()
            != Some(page.page.workspace_id.as_str())
        {
            return Err((StatusCode::FORBIDDEN, "页面不属于当前对话工作空间".into()));
        }
        let source = proposal_source(&message, &component);
        if server
            .db
            .page_proposal_revision(&page_id, &source)
            .map_err(page_error)?
            .is_some()
        {
            return Ok(Json(json!({"page":page})));
        }
        let mut operations = Vec::new();
        for change in changes {
            let (block_id, content) = match change {
                PageChange::Markdown { block_id, markdown } => {
                    (block_id, BlockContent::Markdown { text: markdown })
                }
                PageChange::Reference { reference_page_id } => {
                    if available_page(&server, Some(&page.page.workspace_id), &reference_page_id)
                        .is_none()
                    {
                        return Err((StatusCode::FORBIDDEN, "页面引用应位于当前工作空间".into()));
                    }
                    (
                        None,
                        BlockContent::PageLink {
                            page_id: reference_page_id,
                        },
                    )
                }
            };
            let operation = if let Some(id) = block_id {
                if !page.blocks.iter().any(|block| {
                    block.id == id && matches!(block.content, BlockContent::Markdown { .. })
                }) {
                    return Err(invalid_action("提案只能替换现有的 Markdown 块"));
                }
                BlockOperation::Replace {
                    block: PageBlock { id, content },
                }
            } else {
                BlockOperation::Insert {
                    block: PageBlock {
                        id: uuid::Uuid::new_v4().to_string(),
                        content,
                    },
                    before: None,
                }
            };
            operations.push(operation);
        }
        let document = server
            .db
            .patch_page_blocks(&page_id, revision, operations, &source)
            .map_err(page_error)?;
        return Ok(Json(json!({"page":document})));
    }
    let state = action_payload(&block, &input.values)?;
    let saved = server
        .db
        .save_component_state(&message, &component, input.state_version, state)
        .map_err(server_error)?
        .ok_or_else(|| {
            (
                StatusCode::CONFLICT,
                "组件状态已更新，请重新加载最新状态".into(),
            )
        })?;
    Ok(Json(serde_json::to_value(saved).map_err(server_error)?))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Save {
    workspace_id: String,
    title: Option<String>,
    parent_page_id: Option<String>,
    page_id: Option<String>,
    revision: Option<i64>,
    #[serde(default)]
    image_ids: Vec<String>,
}

async fn save_to_page(
    State(server): State<Arc<AppServer>>,
    Path((conversation, message)): Path<(String, String)>,
    Json(input): Json<Save>,
) -> Result<Json<PageDocument>, Error> {
    let mut snapshot = read_message(&server, &conversation, &message)?;
    let mut seen = snapshot
        .parts
        .iter()
        .filter_map(|part| part["blocks"].as_array())
        .flatten()
        .filter_map(|block| block["resource"]["id"].as_str().map(str::to_owned))
        .collect::<HashSet<_>>();
    let mut images = Vec::new();
    for resource_id in input.image_ids {
        if !seen.insert(resource_id.clone()) {
            continue;
        }
        let projected = project_block(
            &server,
            &conversation,
            &message,
            Block::Image {
                id: uuid::Uuid::new_v4().to_string(),
                resource_id,
            },
        )?;
        if projected["type"] != "image" {
            return Err(invalid_action("该图片没有登记在当前对话中"));
        }
        images.push(projected);
    }
    if !images.is_empty() {
        snapshot.parts.push(json!({"type":"rich", "blocks":images}));
    }
    let block = PageBlock {
        id: uuid::Uuid::new_v4().to_string(),
        content: BlockContent::Rich {
            conversation_id: conversation,
            message_id: message,
            snapshot: serde_json::to_value(snapshot).map_err(server_error)?,
        },
    };
    let result = if let Some(id) = input.page_id {
        let page = server.db.workspace_page(&id, false).map_err(page_error)?;
        if page.page.workspace_id != input.workspace_id {
            return Err(page_error(PageError::Invalid("所选页面不属于目标工作空间")));
        }
        let revision = input
            .revision
            .ok_or_else(|| invalid_action("需要提供页面版本"))?;
        server.db.patch_page_blocks(
            &id,
            revision,
            vec![BlockOperation::Insert {
                block,
                before: None,
            }],
            "chat_artifact",
        )
    } else {
        server.db.create_page_with_blocks(
            NewPage {
                workspace_id: input.workspace_id,
                parent_page_id: input.parent_page_id,
                title: input.title.unwrap_or_else(|| "聊天成果".into()),
                markdown: None,
            },
            vec![block],
            "chat_artifact",
        )
    };
    result.map(Json).map_err(page_error)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_and_unclosed_fences_remain_markdown() {
        for source in ["```yilian-rich\n{}\n```", "```yilian-rich\n{}"] {
            assert!(
                matches!(parse_message(source).as_slice(), [Part::Text(value)] if value == source)
            );
        }
    }

    #[test]
    fn foreign_fences_and_multibyte_text_are_not_treated_as_descriptors() {
        let source = "你好\n~~~~markdown\n```yilian-rich\n{\"schema_version\":1,\"blocks\":[{\"type\":\"text\",\"id\":\"one\",\"text\":\"Hi\"}]}\n```\n~~~~";
        assert!(matches!(parse_message(source).as_slice(), [Part::Text(value)] if value == source));
    }
}
