use super::{cloud_base_url, identity_handler, ApiResult, AuthPayload};
use crate::api::cloud_errors::*;
use crate::app::state::AppServer;
use crate::modules::cloud::{
    client::CloudApiClient, domain::CloudAuthRequest, session::CloudSessionService,
};
use axum::{extract::State, http::StatusCode, Json};
use std::sync::Arc;

pub async fn login_handler(
    State(server): State<Arc<AppServer>>,
    Json(payload): Json<AuthPayload>,
) -> ApiResult {
    authenticate(server, payload, false).await
}

pub async fn register_handler(
    State(server): State<Arc<AppServer>>,
    Json(payload): Json<AuthPayload>,
) -> ApiResult {
    authenticate(server, payload, true).await
}

pub async fn refresh_handler(State(server): State<Arc<AppServer>>) -> ApiResult {
    let _lock = server.cloud_operation_lock.try_lock().map_err(|_| {
        api_error(
            StatusCode::CONFLICT,
            "cloud_operation_in_progress",
            "当前正在同步，请完成后重试",
        )
    })?;
    refresh_session(&server).await?;
    identity_handler(State(Arc::clone(&server))).await
}

pub async fn logout_handler(State(server): State<Arc<AppServer>>) -> ApiResult {
    let _lock = server.cloud_operation_lock.try_lock().map_err(|_| {
        api_error(
            StatusCode::CONFLICT,
            "cloud_operation_in_progress",
            "当前正在同步，请完成后重试",
        )
    })?;

    let data_dir = server.app_data_dir.clone();
    let lease = server.scoped_db.lease();

    // Persist reauthentication_required = true for current active account
    let registry = crate::modules::cloud::profile::ProfileManager::load_or_initialize(&data_dir)
        .map_err(|e| {
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "profile_error",
                &e.to_string(),
            )
        })?;
    let active_profile_id = registry.active_profile_id.clone();
    let is_account = registry
        .profiles
        .get(&active_profile_id)
        .map(|e| e.kind == crate::modules::cloud::profile::ProfileKind::Account)
        .unwrap_or(false);

    if is_account {
        crate::modules::cloud::profile::ProfileManager::set_reauthentication_required(
            &data_dir,
            &active_profile_id,
            true,
        )
        .map_err(|e| {
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "profile_error",
                &e.to_string(),
            )
        })?;
    }

    let conn_config = server
        .db
        .cloud_connection_config()
        .map_err(internal_error)?;
    let session_id = server
        .db
        .cloud_current_session_id()
        .map_err(internal_error)?;
    let identity = server.db.cloud_identity().map_err(internal_error)?;
    let account_id = identity.as_ref().and_then(|id| id.account_id.as_deref());

    let service = CloudSessionService::new(
        server.db.clone_connection(),
        Arc::clone(&server.secret_store),
    );

    let mut report = service
        .logout_with_report(
            &lease,
            session_id.as_deref(),
            conn_config.as_ref().map(|c| c.0.as_str()),
            account_id,
        )
        .await;

    if report.local_cleanup == crate::modules::cloud::revocation::LocalCleanup::Failed {
        report.local_signed_out = false;
        return Err(local_logout_incomplete(report));
    }

    let activation_res =
        crate::modules::cloud::profile::ProfileManager::prepare_guest_activation(&data_dir);
    let activation = match activation_res {
        Ok(plan) => {
            let act = crate::modules::cloud::profile::ProfileManager::activate_prepared_profile(
                &data_dir,
                &server,
                &plan,
                std::time::Duration::from_secs(5),
            )
            .await;
            match act {
                Ok(()) => Some(plan),
                Err(_) => {
                    report.error_codes.push("GUEST_ACTIVATION_FAILED".into());
                    report.local_cleanup = crate::modules::cloud::revocation::LocalCleanup::Failed;
                    None
                }
            }
        }
        Err(_) => {
            report.error_codes.push("GUEST_ACTIVATION_FAILED".into());
            report.local_cleanup = crate::modules::cloud::revocation::LocalCleanup::Failed;
            None
        }
    };

    report.error_codes.sort();
    report.error_codes.dedup();
    report.local_signed_out = report.local_cleanup
        == crate::modules::cloud::revocation::LocalCleanup::Cleared
        && activation.is_some();

    if !report.local_signed_out {
        return Err(local_logout_incomplete(report));
    }

    Ok(Json(serde_json::json!({
        "report": report,
        "activation": activation.unwrap(),
        "status": "awaiting_restart"
    })))
}

pub(super) async fn refresh_session(
    server: &Arc<AppServer>,
) -> Result<(), (StatusCode, Json<serde_json::Value>)> {
    let base_url = cloud_base_url(server)?;
    CloudSessionService::new(
        server.db.clone_connection(),
        Arc::clone(&server.secret_store),
    )
    .refresh(&base_url)
    .await
    .map_err(session_error)
}

async fn authenticate(server: Arc<AppServer>, payload: AuthPayload, register: bool) -> ApiResult {
    let _lock = server.cloud_operation_lock.try_lock().map_err(|_| {
        api_error(
            StatusCode::CONFLICT,
            "cloud_operation_in_progress",
            "当前正在同步，请完成后重试",
        )
    })?;
    if payload.email.trim().is_empty() || payload.password.is_empty() {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "invalid_credentials",
            "请输入邮箱和密码",
        ));
    }
    let device_id = uuid::Uuid::new_v4().to_string();
    let client = CloudApiClient::new(&payload.cloud_base_url).map_err(cloud_client_error)?;
    let request = CloudAuthRequest {
        email: payload.email.trim().to_string(),
        password: payload.password,
        display_name: payload.display_name,
        device_id: device_id.clone(),
        device_label: "Windows device".to_string(),
        client_version: env!("CARGO_PKG_VERSION").to_string(),
    };
    let session = if register {
        client
            .register(request, &uuid::Uuid::new_v4().to_string())
            .await
    } else {
        client.login(request).await
    }
    .map_err(cloud_client_error)?;
    let data_dir = server.app_data_dir.clone();
    let scope_key = crate::modules::cloud::contracts::ScopeKey {
        service_origin: payload.cloud_base_url.trim_end_matches('/').to_string(),
        account_id: session.account_id.clone(),
    };
    let plan = crate::modules::cloud::profile::ProfileManager::prepare_profile(
        &data_dir,
        &scope_key,
        crate::modules::cloud::profile::ActivationChoice::OpenAccountOnly,
    )
    .map_err(profile_error)?;
    let target_db = crate::db::Database::new(
        &crate::modules::cloud::profile::ProfileManager::profile_db_path(&data_dir, &plan),
    )
    .map_err(internal_error)?;
    target_db
        .set_cloud_device_id(&device_id)
        .map_err(internal_error)?;
    CloudSessionService::new(target_db.clone(), Arc::clone(&server.secret_store))
        .persist_authenticated_session_scoped(session, &plan.lease.scope_id)
        .await
        .map_err(session_error)?;
    target_db
        .save_cloud_configuration(scope_key.service_origin.as_str(), "undecided")
        .map_err(internal_error)?;
    crate::modules::cloud::profile::ProfileManager::set_reauthentication_required(
        &data_dir,
        &plan.profile_id,
        false,
    )
    .map_err(profile_error)?;
    crate::modules::cloud::profile::ProfileManager::activate_prepared_profile(
        &data_dir,
        &server,
        &plan,
        std::time::Duration::from_secs(5),
    )
    .await
    .map_err(profile_error)?;
    Ok(Json(serde_json::json!({
        "activation": plan,
        "status": "awaiting_restart"
    })))
}
