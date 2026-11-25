use std::{path::PathBuf, sync::Arc};

use axum::{http::StatusCode, Router};
use secrecy::SecretString;
use serde_json::{json, Value};

use crate::{
    app::state::AppServer,
    integrations::secret::{InMemorySecretStore, SecretRef, SecretStore, CHAT_KEY_REF},
    safety::ControlSession,
};

use super::workspace_chat_tests::{request, TOKEN};

struct Fixture {
    root: PathBuf,
    server: Arc<AppServer>,
    router: Router,
    store: Arc<InMemorySecretStore>,
}

impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("settings-policy-{}", uuid::Uuid::new_v4()));
        let store = Arc::new(InMemorySecretStore::new());
        let server = Self::open(&root, store.clone());
        let router = crate::app::router::build_router(server.clone());
        Self {
            root,
            server,
            router,
            store,
        }
    }

    fn open(root: &std::path::Path, store: Arc<InMemorySecretStore>) -> Arc<AppServer> {
        Arc::new(
            AppServer::new_with_control_session_and_store(
                &root.join("settings.db"),
                root.to_str().unwrap(),
                ControlSession::new(TOKEN).unwrap(),
                store,
            )
            .unwrap(),
        )
    }

    async fn draft(&self) -> Value {
        let (_, mut config) = request(&self.router, "GET", "/api/settings", Value::Null).await;
        config["expected_revision"] = config["revision"].clone();
        config
    }

    async fn save(&self, body: Value) -> (StatusCode, Value) {
        request(&self.router, "PUT", "/api/settings", body).await
    }
}

#[tokio::test]
async fn settings_update_policy_rejects_modified_invalid_values_before_credential_mutation() {
    let f = Fixture::new();
    let key = SecretRef::new(CHAT_KEY_REF);
    let baseline = f.draft().await;
    for (section, property, invalid) in [
        ("model", "temperature", json!(2.1)),
        ("model", "max_tokens", json!(0)),
        ("model", "invoke_timeout_ms", json!(0)),
        ("compaction", "context_window", json!(0)),
        ("compaction", "trigger_threshold", json!(1.5)),
    ] {
        let mut draft = baseline.clone();
        draft[section][property] = invalid;
        draft["model"]["api_key"] = json!("isolated-key-update");
        let (status, response) = f.save(draft).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{response}");
        assert_eq!(response["code"], "invalid_settings");
        assert_eq!(response["field"], format!("{section}.{property}"));
        assert!(f.store.get(&key).await.unwrap().is_none());
    }
    let persisted = f.draft().await;
    assert_eq!(persisted["revision"], baseline["revision"]);
    for section in ["model", "compaction"] {
        assert_eq!(persisted[section], baseline[section]);
    }
}

#[tokio::test]
async fn settings_update_policy_keeps_legacy_revoke_and_disable_actions_and_accepts_zero_threshold()
{
    let f = Fixture::new();
    let key = SecretRef::new(CHAT_KEY_REF);
    f.store
        .put(&key, SecretString::from("isolated-existing-key"))
        .await
        .unwrap();
    {
        let mut config = f.server.config.write();
        config.model.api_key_ref = Some(key.clone());
        config.model.invoke_timeout_ms = 0;
        config.compaction.context_window = 0;
        config.compaction.trigger_threshold = 1.5;
        f.server.db.save_settings(&config).unwrap();
    }
    let mut legacy = f.draft().await;
    legacy["model"]["clear_api_key"] = json!(true);
    legacy["compaction"]["enabled"] = json!(false);
    legacy["permissions"]["mode"] = json!("full_approval");
    let (status, saved) = f.save(legacy).await;
    assert_eq!(status, StatusCode::OK, "{saved}");
    assert_eq!(saved["status"], "saved");
    assert!(f.store.get(&key).await.unwrap().is_none());
    let config = f.draft().await;
    assert_eq!(config["compaction"]["enabled"], false);
    assert_eq!(config["permissions"]["mode"], "full_approval");

    let mut reenabled = config.clone();
    reenabled["compaction"]["enabled"] = json!(true);
    let (status, invalid) = f.save(reenabled).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{invalid}");
    assert_eq!(invalid["field"], "compaction.context_window");

    let mut repaired = config;
    repaired["model"]["temperature"] = json!(2.0);
    repaired["model"]["max_tokens"] = json!(1);
    repaired["model"]["invoke_timeout_ms"] = json!(1000);
    repaired["compaction"]["context_window"] = json!(32_000);
    repaired["compaction"]["trigger_threshold"] = json!(0.0);
    repaired["compaction"]["enabled"] = json!(true);
    let (status, saved) = f.save(repaired).await;
    assert_eq!(status, StatusCode::OK, "{saved}");
    assert_eq!(saved["status"], "saved");
    let repaired = f.draft().await;
    assert_eq!(
        repaired["compaction"]["trigger_threshold"].as_f64(),
        Some(0.0)
    );
    assert_eq!(repaired["compaction"]["enabled"], true);
}

#[tokio::test]
async fn settings_update_policy_reports_the_real_directory_application_actions() {
    let f = Fixture::new();
    let skill = f.root.join("probe-skills/settings-policy-skill");
    let subagent = f.root.join("probe-agents/settings-policy-subagent");
    std::fs::create_dir_all(&skill).unwrap();
    std::fs::create_dir_all(&subagent).unwrap();
    std::fs::write(skill.join("SKILL.md"),
        "---\nname: settings-policy-skill\ndescription: Fixture skill\n---\nFixture instructions.\n").unwrap();
    std::fs::write(subagent.join("AGENT.md"),
        "---\nname: settings-policy-subagent\ndescription: Fixture agent\n---\nFixture instructions.\n").unwrap();
    let mut draft = f.draft().await;
    draft["skills"]["directories"] = json!(["probe-skills"]);
    draft["subagents"]["directories"] = json!(["probe-agents"]);
    let (status, saved) = f.save(draft).await;
    assert_eq!(status, StatusCode::OK, "{saved}");
    assert_eq!(saved["application"], "next_read");
    assert_eq!(
        saved["pending_application"],
        json!([
            {"field":"skills.directories","action":"refresh_skill_discovery"},
            {"field":"subagents.directories","action":"restart_application"}
        ])
    );
    let visible = |report: &Value| {
        report["sources"].as_array().unwrap().iter().any(|source| {
            source["path"].as_str().unwrap().contains("probe-skills")
                && source["skill_count"].as_u64() == Some(1)
        })
    };
    let (_, stale) = request(&f.router, "GET", "/api/skills/discovery", Value::Null).await;
    assert!(!visible(&stale));
    let (status, refreshed) =
        request(&f.router, "POST", "/api/skills/discovery", Value::Null).await;
    assert_eq!(status, StatusCode::OK);
    assert!(visible(&refreshed));
    assert!(f
        .server
        .subagents
        .iter()
        .all(|agent| agent.name != "settings-policy-subagent"));
    let (_, same) = f.save(f.draft().await).await;
    assert!(same.get("pending_application").is_none());
    let restarted = Fixture::open(&f.root, f.store.clone());
    assert!(restarted
        .subagents
        .iter()
        .any(|agent| agent.name == "settings-policy-subagent"));
}
