//! One model invocation delegated by an active, gateway-authorized local MCP call.
use super::{
    chat::runtime::{explicit_chat_model_profile, resolve_scoped_model_source, ChatModelSource},
    plugin_services::PluginMcpBinding,
};
use crate::{
    agent::delegation::{current_scope, ExecutionScope},
    app::state::AppServer,
    execution::{
        context::{chat_turn_model, ChatTurnModel},
        owner::current_owner,
        ExecutionOwner,
    },
    integrations::llm::{
        client::{LlmClient, LlmError},
        types::ChatMessage,
    },
    safety::{
        grant::AuthorizedResource,
        subject::{PluginServicePrincipal, ToolInvocation},
    },
    tools::trait_def::ToolExecutionContext,
};
use axum::{
    extract::{DefaultBodyLimit, State},
    http::{header, HeaderMap, StatusCode},
    routing::post,
    Json, Router,
};
use parking_lot::Mutex;
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, HashMap},
    sync::{Arc, Weak},
    time::{Duration, Instant},
};
use tokio::sync::OnceCell;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

mod authority;
mod budget;
use authority::{active_workspace, reserve_direct_goal_model, still_authorized, ModelAuthority};

use budget::{budget_unavailable, BudgetCharge};

const PATH: &str = "/api/plugin-model/invoke";
const MAX_INPUT_BYTES: usize = 16 * 1024;
const MAX_OUTPUT_TOKENS: u32 = 2_048;

type Error = (StatusCode, Json<Value>);

fn problem(status: StatusCode, code: &'static str, message: &'static str) -> Error {
    (status, Json(json!({"code":code,"error":message})))
}

#[derive(Default)]
pub(crate) struct PluginModelProxy {
    listener: OnceCell<Listener>,
    grants: Mutex<HashMap<String, ModelGrant>>,
}

struct Listener {
    url: String,
    server: tokio::task::JoinHandle<()>,
}

impl Drop for Listener {
    fn drop(&mut self) {
        self.server.abort();
    }
}

struct ModelGrant {
    service: Arc<PluginMcpBinding>,
    owner: ExecutionOwner,
    workspace_id: Option<String>,
    turn_model: Option<ChatTurnModel>,
    child_model: Option<crate::agent::subagent_runtime::ChildModel>,
    invocation: ToolInvocation,
    authority: ModelAuthority,
    scope: Option<ExecutionScope>,
    cancelled: CancellationToken,
    instance_ended: CancellationToken,
}

impl ModelGrant {
    fn source_invocation(&self) -> &ToolInvocation {
        self.authority.configuration_invocation(&self.invocation)
    }
}

pub(crate) struct ModelCallLease {
    proxy: Arc<PluginModelProxy>,
    token: String,
    endpoint: String,
    cancelled: CancellationToken,
}

impl ModelCallLease {
    pub(crate) fn add_headers(&self, headers: &mut BTreeMap<String, String>) {
        headers.insert("X-Yilian-Model-Endpoint".into(), self.endpoint.clone());
        headers.insert("X-Yilian-Model-Token".into(), self.token.clone());
    }
}

impl Drop for ModelCallLease {
    fn drop(&mut self) {
        self.cancelled.cancel();
        self.proxy.grants.lock().remove(&self.token);
    }
}

pub(crate) async fn authorize_call(
    service: &Arc<PluginMcpBinding>,
    server_id: &str,
    tool_name: &str,
    context: &ToolExecutionContext,
    instance_ended: &CancellationToken,
) -> Option<ModelCallLease> {
    if !service.is_local() {
        return None;
    }
    let server = service.server.upgrade()?;
    if !server
        .plugin_services
        .allows_model(&service.plugin, &service.service, service.instance)
    {
        return None;
    }
    let invocation = context.invocation()?.clone();
    let owner = current_owner()?;
    let scope = current_scope();
    let evidence = context
        .authorized_resources
        .iter()
        .find_map(|resource| match resource {
            AuthorizedResource::Mcp {
                server_id: id,
                tool_name: name,
                grant_id,
                one_shot_approval,
            } if id == server_id && name == tool_name => {
                Some((grant_id.clone(), *one_shot_approval))
            }
            _ => None,
        })?;
    let authority = ModelAuthority {
        server_id: server_id.into(),
        tool_name: tool_name.into(),
        grant_id: evidence.0,
        one_shot_approval: evidence.1,
        delegation: context.delegation().cloned(),
    };
    let source = authority.configuration_invocation(&invocation);
    let workspace_id = active_workspace(&server, &owner, source, scope.as_ref()).ok()?;
    let turn_model = chat_turn_model(&source.conversation_id);
    let child_model = match &authority.delegation {
        Some(delegation) => Some(crate::agent::subagent_runtime::child_model_for_handoff(
            &delegation.handoff_id,
        )?),
        None => None,
    };
    still_authorized(&server, &invocation, &authority).ok()?;

    let proxy = Arc::clone(&server.plugin_services.model_proxy);
    let listener = proxy
        .listener
        .get_or_try_init(|| async {
            let tcp = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).await?;
            let url = format!("http://{}{PATH}", tcp.local_addr()?);
            let router = Router::new()
                .route(PATH, post(invoke))
                .layer(DefaultBodyLimit::max(MAX_INPUT_BYTES * 6))
                .with_state(Arc::downgrade(&server));
            let server = tokio::spawn(async move {
                let _ = axum::serve(tcp, router).await;
            });
            Ok::<_, std::io::Error>(Listener { url, server })
        })
        .await
        .ok()?;
    let endpoint = listener.url.clone();
    let token = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    let cancelled = CancellationToken::new();
    proxy.grants.lock().insert(
        token.clone(),
        ModelGrant {
            service: Arc::clone(service),
            owner,
            workspace_id,
            turn_model,
            child_model,
            invocation,
            authority,
            scope,
            cancelled: cancelled.clone(),
            instance_ended: instance_ended.clone(),
        },
    );
    Some(ModelCallLease {
        proxy,
        token,
        endpoint,
        cancelled,
    })
}

#[derive(Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Output {
    #[default]
    Text,
    JsonObject,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ModelRequest {
    purpose: String,
    input: String,
    max_output_tokens: u32,
    #[serde(default)]
    output: Output,
}

async fn invoke(
    State(server): State<Weak<AppServer>>,
    headers: HeaderMap,
    Json(request): Json<ModelRequest>,
) -> Result<Json<Value>, Error> {
    let unauthorized = || {
        problem(
            StatusCode::UNAUTHORIZED,
            "invalid_model_lease",
            "模型请求未关联可用的服务任务",
        )
    };
    let server = server.upgrade().ok_or_else(unauthorized)?;
    let bearer = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or_else(unauthorized)?;
    if request.purpose != "chat"
        || request.input.trim().is_empty()
        || request.input.len() > MAX_INPUT_BYTES
        || !(1..=MAX_OUTPUT_TOKENS).contains(&request.max_output_tokens)
    {
        return Err(problem(
            StatusCode::BAD_REQUEST,
            "invalid_model_request",
            "请提交 chat 用途、文本输入及有效的输出 token 上限",
        ));
    }
    let grant = server
        .plugin_services
        .model_proxy
        .grants
        .lock()
        .remove(bearer)
        .ok_or_else(unauthorized)?;
    if grant.cancelled.is_cancelled()
        || !grant.service.current(&server)
        || !server.plugin_services.allows_model(
            &grant.service.plugin,
            &grant.service.service,
            grant.service.instance,
        )
    {
        return Err(unauthorized());
    }
    still_authorized(&server, &grant.invocation, &grant.authority).map_err(|_| unauthorized())?;
    let workspace = active_workspace(
        &server,
        &grant.owner,
        grant.source_invocation(),
        grant.scope.as_ref(),
    )
    .map_err(|_| unauthorized())?;
    if workspace != grant.workspace_id {
        return Err(unauthorized());
    }
    let started = Instant::now();
    let mut client = Route::resolve(&server, &grant, workspace.as_deref())?;
    let mut goal_reservation =
        reserve_direct_goal_model(&server, &grant.owner, grant.scope.as_ref())?;
    let scope = goal_reservation
        .as_ref()
        .map(|(_, scope)| scope)
        .or(grant.scope.as_ref());
    if scope.is_some_and(ExecutionScope::is_bounded) {
        client.client = client.client.with_one_pool_attempt();
    }
    if matches!(request.output, Output::JsonObject) {
        client.client = client.client.with_json_object_output();
    }
    let messages = [ChatMessage::user(request.input)];
    let cap = request
        .max_output_tokens
        .min(client.client.max_output_tokens());
    let (cap, charge) = match scope {
        Some(scope) => BudgetCharge::begin(scope, &messages, cap)?,
        None => (cap, None),
    };
    let service_principal =
        PluginServicePrincipal::new(&grant.service.plugin, &grant.service.service);
    let call = client.client.invoke_with_max_tokens(&messages, &[], cap);
    let result = tokio::select! {
        biased;
        () = grant.cancelled.cancelled() => return Err(cancelled()),
        () = grant.instance_ended.cancelled() => return Err(cancelled()),
        () = scope_cancelled(scope) => return Err(cancelled()),
        result = tokio::time::timeout(Duration::from_secs(60), call) => result.unwrap_or(Err(LlmError::Timeout)),
    };
    if let Some(charge) = charge {
        let accounting = charge.finish(result.as_ref().ok().and_then(|value| value.usage.as_ref()));
        if result.is_ok() {
            accounting?;
        }
    }
    if let Some((reservation, _)) = &mut goal_reservation {
        reservation.finish().map_err(|_| budget_unavailable())?;
    }
    let completion = result.map_err(provider_problem)?;
    let choice = completion.choices.first().ok_or_else(invalid_output)?;
    let content = choice.message.content.as_ref().ok_or_else(invalid_output)?;
    let output = match request.output {
        Output::Text => json!(content),
        Output::JsonObject => {
            if choice.finish_reason.as_deref() == Some("length") {
                return Err(invalid_output());
            }
            let object: Value = serde_json::from_str(content).map_err(|_| invalid_output())?;
            if !object.is_object() {
                return Err(invalid_output());
            }
            object
        }
    };
    let usage = completion.usage.as_ref().map(|usage| json!({
        "input_tokens":usage.prompt_tokens,"output_tokens":usage.completion_tokens,"total_tokens":usage.total_tokens,
    }));
    let elapsed = started.elapsed().as_millis() as u64;
    let source = client.source(&completion.model)?;
    tracing::info!(subject = %service_principal.subject().subject_id, caller = %grant.invocation.subject.subject_id,
        call_id = %grant.invocation.tool_call_id, latency_ms = elapsed, "Plugin model invocation completed");
    Ok(Json(json!({
        "purpose":"chat", "execution_owner":grant.owner,"tool_call_id":grant.invocation.tool_call_id,
        "turn_id":grant.turn_model.as_ref().map(|model| &model.turn_id),
        "source":source,"usage":usage,"latency_ms":elapsed,"finish_reason":choice.finish_reason,"output":output,
    })))
}

async fn scope_cancelled(scope: Option<&ExecutionScope>) {
    if let Some(scope) = scope {
        tokio::select! {
            () = scope.cancel.cancelled() => (),
            () = async {
                match scope.deadline {
                    Some(deadline) => tokio::time::sleep_until(deadline).await,
                    None => std::future::pending().await,
                }
            } => (),
        }
    } else {
        std::future::pending().await
    }
}

fn cancelled() -> Error {
    problem(
        StatusCode::CONFLICT,
        "model_lease_ended",
        "服务任务已结束或取消",
    )
}

fn invalid_output() -> Error {
    problem(
        StatusCode::BAD_GATEWAY,
        "invalid_model_output",
        "模型未返回请求的输出结构",
    )
}

fn unavailable() -> Error {
    problem(
        StatusCode::SERVICE_UNAVAILABLE,
        "model_unavailable",
        "请检查当前生效的对话模型配置",
    )
}

fn provider_problem(error: LlmError) -> Error {
    match error {
        LlmError::Timeout => problem(
            StatusCode::GATEWAY_TIMEOUT,
            "model_timeout",
            "模型服务响应超时",
        ),
        LlmError::NoApiKey | LlmError::SecretUnavailable => unavailable(),
        LlmError::Provider {
            status: 401 | 403, ..
        } => problem(
            StatusCode::BAD_GATEWAY,
            "provider_authentication",
            "模型供应商凭据不可用",
        ),
        LlmError::Provider { status: 429, .. } => problem(
            StatusCode::TOO_MANY_REQUESTS,
            "provider_rate_limit",
            "模型服务请求受限",
        ),
        LlmError::Parse(_) => invalid_output(),
        _ => problem(
            StatusCode::BAD_GATEWAY,
            "model_request_failed",
            "当前模型调用失败",
        ),
    }
}

struct Route {
    client: LlmClient,
    profile: Option<String>,
    pool: Option<PoolRoute>,
    child_name: Option<String>,
}

struct PoolRoute {
    id: String,
    revision: u64,
    selected: Arc<Mutex<Option<String>>>,
    identities: BTreeMap<String, (String, String)>,
}

impl Route {
    fn resolve(
        server: &AppServer,
        grant: &ModelGrant,
        workspace: Option<&str>,
    ) -> Result<Self, Error> {
        let model = server.config.read().model.clone();
        let settings_owner = grant.source_invocation();
        let source = if let Some(child) = &grant.child_model {
            ChatModelSource::Fixed {
                config: child.config.clone(),
                row: None,
            }
        } else if let Some(profile_id) = grant
            .turn_model
            .as_ref()
            .and_then(|turn| turn.profile_id.as_deref())
        {
            let row =
                explicit_chat_model_profile(&server.db, profile_id).map_err(|_| unavailable())?;
            ChatModelSource::Fixed {
                config: row.to_model_config(),
                row: Some(row),
            }
        } else {
            resolve_scoped_model_source(
                &server.db,
                &model,
                settings_owner.agent_id.as_deref(),
                workspace,
            )
            .map_err(|_| unavailable())?
        };
        match source {
            ChatModelSource::Fixed { config, row } => {
                if config.invoke_timeout_ms == 0
                    || config.max_tokens == 0
                    || config.name.trim().is_empty()
                    || !crate::modules::settings::domain::policy::valid_provider_url(
                        &config.base_url,
                    )
                    || row.as_ref().is_some_and(|row| row.api_format != "openai")
                {
                    return Err(unavailable());
                }
                let recorder = row.as_ref().map(|row| {
                    Arc::new(crate::integrations::llm::usage::DatabaseUsageRecorder::new(
                        server.db.clone_connection(),
                        &row.id,
                        &row.provider,
                        &row.model,
                        "plugin_service",
                    ))
                        as Arc<dyn crate::integrations::llm::client::UsageRecorder>
                });
                let mut client = LlmClient::new_with_usage_recorder(
                    &config,
                    Arc::clone(&server.secret_resolver),
                    recorder,
                );
                if let Some(child) = &grant.child_model {
                    client = client.with_generation_overrides(child.generation);
                }
                Ok(Self {
                    client,
                    profile: row.map(|row| row.id),
                    pool: None,
                    child_name: grant.child_model.as_ref().map(|child| child.name.clone()),
                })
            }
            ChatModelSource::Pool(pool) => {
                let selected = Arc::new(Mutex::new(None));
                let observer = selected.clone();
                let routed = crate::integrations::llm::model_pool::load_model_pool_runtime(
                    &server.db,
                    Arc::clone(&server.secret_resolver),
                    &pool.id,
                    &settings_owner.conversation_id,
                    Arc::new(move |event| {
                        *observer.lock() = event.to_profile_id;
                    }),
                )
                .map_err(|_| unavailable())?;
                let identities = routed
                    .candidates
                    .iter()
                    .map(|candidate| {
                        let (provider, model) = candidate.client.model_identity();
                        (
                            candidate.profile_id.clone(),
                            (provider.to_owned(), model.to_owned()),
                        )
                    })
                    .collect();
                let pool = PoolRoute {
                    id: routed.pool.id.clone(),
                    revision: routed.pool.revision,
                    selected,
                    identities,
                };
                Ok(Self {
                    client: LlmClient::new(&model, Arc::clone(&server.secret_resolver))
                        .with_model_pool(routed),
                    profile: None,
                    pool: Some(pool),
                    child_name: None,
                })
            }
        }
    }

    fn source(&self, reported_model: &Option<String>) -> Result<Value, Error> {
        if let Some(pool) = &self.pool {
            let selected = pool.selected.lock();
            let id = selected.as_ref().ok_or_else(unavailable)?;
            let (provider, model) = pool.identities.get(id).ok_or_else(unavailable)?;
            return Ok(
                json!({"kind":"pool","pool_id":pool.id,"pool_revision":pool.revision,
                "profile_id":id,"provider":provider,"model":model,"reported_model":reported_model}),
            );
        }
        let (provider, model) = self.client.model_identity();
        let kind = if self.child_name.is_some() {
            "subagent_config"
        } else if self.profile.is_some() {
            "profile"
        } else {
            "global_config"
        };
        Ok(json!({"kind":kind,"subagent_name":self.child_name,
            "profile_id":self.profile,"provider":provider,"model":model,"reported_model":reported_model}))
    }
}
