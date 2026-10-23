use crate::{
    app::state::AppServer,
    modules::learning::{self, Choice, Citation, LearningExercise, LearningSource},
};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post, put},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;
type Error = (StatusCode, Json<Value>);
fn error(message: impl Into<String>) -> Error {
    let message = message.into();
    let status = if message.ends_with("conflict") {
        StatusCode::CONFLICT
    } else {
        StatusCode::BAD_REQUEST
    };
    (status, Json(json!({"error":message})))
}
pub fn router() -> Router<Arc<AppServer>> {
    Router::new()
        .route(
            "/api/workspaces/:workspace/learning",
            get(list).post(create),
        )
        .route("/api/workspaces/:workspace/learning/:session", get(detail))
        .route(
            "/api/workspaces/:workspace/learning/:session/cancel",
            post(cancel),
        )
        .route(
            "/api/workspaces/:workspace/learning/:session/notes",
            put(save_note),
        )
        .route(
            "/api/workspaces/:workspace/learning/:session/notes/:revision",
            get(note_version),
        )
        .route(
            "/api/workspaces/:workspace/learning/:session/exercises/:exercise/answers",
            post(answer),
        )
}

async fn list(
    State(server): State<Arc<AppServer>>,
    Path(workspace): Path<String>,
) -> Result<Json<Value>, Error> {
    Ok(Json(
        json!({"sessions":server.db.learning_sessions(&workspace).map_err(error)?}),
    ))
}
#[derive(Deserialize)]
struct Create {
    title: String,
    document_ids: Vec<String>,
}
async fn create(
    State(server): State<Arc<AppServer>>,
    Path(workspace): Path<String>,
    Json(body): Json<Create>,
) -> Result<Json<Value>, Error> {
    if server.db.get_active_llm_model().map_err(error)?.is_none() {
        return Err(error("请先配置并选择可用的对话模型"));
    }
    let documents = server.db.knowledge_documents().map_err(error)?;
    let mut sources = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for id in body.document_ids {
        if !seen.insert(id.clone()) {
            return Err(error("资料选择重复"));
        }
        let document = documents
            .iter()
            .find(|d| d.id == id)
            .ok_or_else(|| error("所选知识资料不存在"))?;
        sources.push(LearningSource {
            document_id: id,
            resource_id: document.resource_id.clone(),
            name: document.name.clone(),
            hash: document.hash.clone(),
        });
    }
    let session = server
        .db
        .create_learning_session(&workspace, &body.title, &sources)
        .map_err(error)?;
    let generated = session.clone();
    tokio::spawn(async move {
        let stopped = async {
            loop {
                tokio::time::sleep(std::time::Duration::from_millis(250)).await;
                match server
                    .db
                    .learning_session(&generated.workspace_id, &generated.id)
                {
                    Ok(session) if session.status == "generating" => {}
                    Ok(_) => return Ok(()),
                    Err(error) => return Err(error),
                }
            }
        };
        let result = tokio::select! {result=learning::generate(&server,&generated)=>result,result=stopped=>result};
        if let Err(failure) = result {
            if let Err(storage) = server.db.fail_learning_session(&generated.id, &failure) {
                tracing::error!(error=%storage,"failed to persist learning error");
            }
        }
    });
    Ok(Json(json!({"session":session})))
}
async fn cancel(
    State(server): State<Arc<AppServer>>,
    Path((workspace, id)): Path<(String, String)>,
) -> Result<Json<Value>, Error> {
    if !server
        .db
        .cancel_learning_session(&workspace, &id)
        .map_err(error)?
    {
        return Err(error("learning_state_conflict"));
    }
    Ok(Json(json!({"cancelled":true})))
}
#[derive(Deserialize)]
struct PublicExercise {
    question: String,
    options: Vec<Choice>,
    sources: Vec<Citation>,
}
fn exercises(server: &AppServer, session: &str) -> Result<Vec<LearningExercise>, Error> {
    server
        .db
        .learning_exercise_resources(session)
        .map_err(error)?
        .into_iter()
        .map(|(id, resource_id)| {
            let bytes = server
                .resource_service
                .read_bytes(&resource_id)
                .map_err(|e| error(e.to_string()))?;
            let public: PublicExercise =
                serde_json::from_slice(&bytes).map_err(|e| error(e.to_string()))?;
            Ok(LearningExercise {
                id,
                resource_id,
                question: public.question,
                options: public.options,
                sources: public.sources,
            })
        })
        .collect()
}
fn resource_text(server: &AppServer, id: &str) -> Result<String, Error> {
    String::from_utf8(
        server
            .resource_service
            .read_bytes(id)
            .map_err(|e| error(e.to_string()))?,
    )
    .map_err(|e| error(e.to_string()))
}
async fn detail(
    State(server): State<Arc<AppServer>>,
    Path((workspace, id)): Path<(String, String)>,
) -> Result<Json<Value>, Error> {
    let session = server.db.learning_session(&workspace, &id).map_err(error)?;
    let notes = server.db.learning_notes(&id).map_err(error)?;
    let content = notes
        .first()
        .map(|note| resource_text(&server, &note.resource_id))
        .transpose()?;
    Ok(Json(
        json!({"session":session,"notes":notes,"content":content,"exercises":exercises(&server,&id)?,"answers":server.db.learning_answers(&id).map_err(error)?}),
    ))
}
#[derive(Deserialize)]
struct SaveNote {
    content: String,
    expected_revision: i64,
}
async fn save_note(
    State(server): State<Arc<AppServer>>,
    Path((workspace, id)): Path<(String, String)>,
    Json(body): Json<SaveNote>,
) -> Result<Json<Value>, Error> {
    let session = server.db.learning_session(&workspace, &id).map_err(error)?;
    if session.status != "ready" || session.revision != body.expected_revision {
        return Err(error("learning_revision_conflict"));
    }
    if body.content.trim().is_empty() || body.content.chars().count() > 50000 {
        return Err(error("笔记不能为空且不能超过50000字"));
    }
    let parent = server
        .db
        .learning_notes(&id)
        .map_err(error)?
        .first()
        .map(|note| note.resource_id.clone());
    let resource=server.resource_service.ingest("learning-notes.md","text/markdown",body.content.as_bytes(),json!({"source":"learning-user-edit","workspace_id":workspace,"session_id":id,"revision":session.revision+1,"parent_resource_id":parent})).map_err(|e|error(e.to_string()))?;
    server
        .db
        .save_learning_note(&id, body.expected_revision, &resource.id)
        .map_err(error)?;
    Ok(Json(
        json!({"session":server.db.learning_session(&workspace,&id).map_err(error)?}),
    ))
}
async fn note_version(
    State(server): State<Arc<AppServer>>,
    Path((workspace, id, revision)): Path<(String, String, i64)>,
) -> Result<Json<Value>, Error> {
    server.db.learning_session(&workspace, &id).map_err(error)?;
    let note = server
        .db
        .learning_notes(&id)
        .map_err(error)?
        .into_iter()
        .find(|note| note.revision == revision)
        .ok_or_else(|| error("笔记版本不存在"))?;
    Ok(Json(
        json!({"content":resource_text(&server,&note.resource_id)?,"note":note}),
    ))
}
#[derive(Deserialize)]
struct Answer {
    request_id: String,
    option_id: String,
}
async fn answer(
    State(server): State<Arc<AppServer>>,
    Path((workspace, session, exercise)): Path<(String, String, String)>,
    Json(body): Json<Answer>,
) -> Result<Json<Value>, Error> {
    let active = server
        .db
        .learning_session(&workspace, &session)
        .map_err(error)?;
    if active.status != "ready" {
        return Err(error("练习尚未生成"));
    }
    let question = exercises(&server, &session)?
        .into_iter()
        .find(|item| item.id == exercise)
        .ok_or_else(|| error("练习不属于当前学习记录"))?;
    if !question
        .options
        .iter()
        .any(|option| option.id == body.option_id)
    {
        return Err(error("请选择有效选项"));
    }
    Ok(Json(
        json!({"answer":server.db.answer_learning_exercise(&session,&exercise,&body.request_id,&body.option_id).map_err(error)?}),
    ))
}
