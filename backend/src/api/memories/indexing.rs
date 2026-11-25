use std::sync::Arc;

use axum::{extract::State, http::StatusCode, Json};
use parking_lot::Mutex;
use serde::Serialize;
use tokio_util::sync::CancellationToken;

use crate::{app::state::AppServer, integrations::llm::client::LlmClient};

use super::StorageApiError;

#[derive(Clone, Copy, Default, Serialize)]
#[serde(rename_all = "snake_case")]
enum Phase {
    #[default]
    Idle,
    Running,
    Completed,
    Cancelled,
    Failed,
}

#[derive(Clone, Default, Serialize)]
pub struct IndexStatus {
    phase: Phase,
    indexed: usize,
    model: Option<String>,
    error: Option<String>,
}

struct Job {
    id: uuid::Uuid,
    cancel: CancellationToken,
    status: IndexStatus,
}

#[derive(Default)]
pub(crate) struct MemoryIndexer {
    job: Arc<Mutex<Option<Job>>>,
}

impl MemoryIndexer {
    pub(crate) fn start(&self, server: &AppServer) -> Result<IndexStatus, StorageApiError> {
        let config = server.config.read().model.clone();
        if !config.has_embedding() {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(
                    serde_json::json!({"code":"embedding_not_configured","error":"请先配置向量服务"}),
                ),
            ));
        }
        if !config.has_local_embedding()
            && !crate::modules::onboarding::allowed(&server.db, "external_model")
        {
            return Err((
                StatusCode::FORBIDDEN,
                Json(
                    serde_json::json!({"code":"external_model_disabled","error":"当前资料档案已关闭外部模型调用"}),
                ),
            ));
        }
        let identity = crate::db::memory_governance::embedding_identity(&config);
        let mut job = self.job.lock();
        if let Some(previous) = job.as_ref() {
            if matches!(previous.status.phase, Phase::Running)
                && !previous.cancel.is_cancelled()
                && previous.status.model.as_deref() == Some(identity.as_str())
            {
                return Ok(previous.status.clone());
            }
            previous.cancel.cancel();
        }
        let cancel = server.cloud_revocation_retry_cancel.child_token();
        let status = IndexStatus {
            phase: Phase::Running,
            model: Some(identity.clone()),
            ..Default::default()
        };
        let id = uuid::Uuid::new_v4();
        *job = Some(Job {
            id,
            cancel: cancel.clone(),
            status: status.clone(),
        });
        drop(job);

        let job = Arc::clone(&self.job);
        let pressure = Arc::clone(&server.memory_pressure);
        let db = server.db.clone_connection();
        let current = server.config.clone();
        let client = LlmClient::new(&config, server.secret_resolver.clone())
            .with_profile_permissions(db.clone_connection());
        tokio::spawn(async move {
            loop {
                let result = async {
                    let records = db.memories_needing_embedding(&identity, 16)?;
                    if records.is_empty() {
                        return Ok(false);
                    }
                    for memory in records {
                        if cancel.is_cancelled()
                            || crate::db::memory_governance::embedding_identity(&current.read().model) != identity
                            || (!config.has_local_embedding()
                                && !crate::modules::onboarding::allowed(&db, "external_model"))
                        {
                            cancel.cancel();
                            return Ok(false);
                        }
                        let ticket = {
                            let config = current.read();
                            if crate::db::memory_governance::embedding_identity(&config.model) != identity {
                                cancel.cancel();
                                return Ok(false);
                            }
                            db.begin_memory_embedding(&memory, &identity)
                        };
                        let ticket = match ticket {
                            Err(error) if error == "memory_revision_conflict" => continue,
                            result => result?,
                        };
                        let vector = tokio::select! {
                            _ = cancel.cancelled() => return Ok(false),
                            result = client.embed(&memory.content) => result.map_err(|_| "向量服务调用失败，请检查连接后继续重建".to_string())?
                        };
                        match db.complete_memory_embedding(&ticket, &vector) {
                            Ok(()) => {
                                if let Some(job) = job.lock().as_mut().filter(|job| job.id == id && !cancel.is_cancelled()) {
                                    job.status.indexed += 1;
                                }
                            }
                            Err(error) if error == "stale_embedding_result" => continue,
                            Err(error) => return Err(error),
                        }
                        tokio::select! {
                            _ = cancel.cancelled() => return Ok(false),
                            _ = tokio::time::sleep(crate::modules::resource::policy::background_delay(&db,pressure.load(std::sync::atomic::Ordering::Acquire))) => {}
                        }
                    }
                    Ok::<_, String>(true)
                }.await;
                if matches!(result, Ok(true)) {
                    continue;
                }
                if let Some(active) = job.lock().as_mut().filter(|active| active.id == id) {
                    active.status.phase = match result {
                        _ if cancel.is_cancelled() => Phase::Cancelled,
                        Ok(_) => Phase::Completed,
                        Err(error) => {
                            active.status.error = Some(error);
                            Phase::Failed
                        }
                    };
                }
                break;
            }
        });
        Ok(status)
    }

    pub(crate) fn cancel(&self) -> IndexStatus {
        let mut current = self.job.lock();
        if let Some(job) = current.as_mut() {
            if matches!(job.status.phase, Phase::Running) {
                job.cancel.cancel();
                job.status.phase = Phase::Cancelled;
            }
        }
        current
            .as_ref()
            .map(|job| job.status.clone())
            .unwrap_or_default()
    }

    pub(crate) fn status(&self) -> IndexStatus {
        self.job
            .lock()
            .as_ref()
            .map(|job| job.status.clone())
            .unwrap_or_default()
    }
}

pub(crate) async fn start(
    State(server): State<Arc<AppServer>>,
) -> Result<Json<IndexStatus>, StorageApiError> {
    server.memory_indexer.start(&server).map(Json)
}

pub(crate) async fn status(State(server): State<Arc<AppServer>>) -> Json<IndexStatus> {
    Json(server.memory_indexer.status())
}

pub(crate) async fn cancel(State(server): State<Arc<AppServer>>) -> Json<IndexStatus> {
    Json(server.memory_indexer.cancel())
}
